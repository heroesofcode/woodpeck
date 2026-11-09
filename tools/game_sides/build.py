#!/usr/bin/env python3
"""Build src/books/game_sides.bin from public game databases.

For every game the board placement (FEN board field) after each ply is hashed with
FNV-1a 64 together with the side to move, counted once per game. Only positions with at
least 13 men in the first MAX_PLY plies are kept. A placement that occurred in at least
MIN_GAMES games goes into the table: with its side when no game had the other side to
move, with both sides otherwise (the app then abstains on the side but still knows the
position is real). The table holds hashes only, no positions.

Usage: build.py [--min-games K] [--max-ply N] [--workers W] [--counts out.npz]
                OUT.bin SOURCE [SOURCE ...]
A SOURCE is a local or https PGN file, plain, .zst or single-member .zip.
Needs python-chess, numpy and (for .zst) zstandard.
"""
import argparse, multiprocessing as mp, re, struct, sys, time, urllib.request, zlib
import chess
import numpy as np

MIN_MEN = 13
M64 = (1 << 64) - 1
TOKEN = re.compile(r"\{[^}]*\}|\([^)]*\)|\$\d+|\d+\.+|1-0|0-1|1/2-1/2|\*")


def fnv1a64(text):
    h = 0xCBF29CE484222325
    for byte in text.encode():
        h = ((h ^ byte) * 0x100000001B3) & M64
    return h


class Resuming:
    """An HTTP body that reconnects with a Range request when the connection stalls."""

    def __init__(self, url):
        self.url, self.offset, self.response = url, 0, None

    def read(self, size):
        for _ in range(20):
            try:
                if self.response is None:
                    headers = {"User-Agent": "woodpeck-game-sides"}
                    if self.offset:
                        headers["Range"] = f"bytes={self.offset}-"
                    request = urllib.request.Request(self.url, headers=headers)
                    self.response = urllib.request.urlopen(request, timeout=60)
                block = self.response.read(size)
                self.offset += len(block)
                return block
            except OSError:
                self.response = None
                time.sleep(5)
        raise OSError(f"giving up on {self.url} at byte {self.offset}")


def open_source(source):
    if source.startswith(("http://", "https://")):
        return Resuming(source)
    return open(source, "rb")


def chunks(source):
    raw = open_source(source)
    if source.endswith(".zst"):
        import zstandard

        reader = zstandard.ZstdDecompressor().stream_reader(raw)
        while block := reader.read(1 << 20):
            yield block
    elif source.endswith(".zip"):
        header = raw.read(30)
        name_len, extra_len = struct.unpack("<HH", header[26:30])
        raw.read(name_len + extra_len)
        inflate = zlib.decompressobj(-15)
        while not inflate.eof and (block := raw.read(1 << 20)):
            yield inflate.decompress(block)
    else:
        while block := raw.read(1 << 20):
            yield block


def movetexts(sources):
    for source in sources:
        pending, current = b"", []
        for block in chunks(source):
            lines = (pending + block).split(b"\n")
            pending = lines.pop()
            for line in lines:
                if line.startswith(b"["):
                    if current:
                        yield b" ".join(current).decode("latin1")
                        current = []
                elif line.strip():
                    current.append(line)
        if current:
            yield b" ".join(current).decode("latin1")


def batches(sources, size=400):
    batch = []
    for text in movetexts(sources):
        batch.append(text)
        if len(batch) == size:
            yield batch
            batch = []
    if batch:
        yield batch


def game_keys(args):
    batch, max_ply = args
    keys = []
    for text in batch:
        board, seen = chess.Board(), set()
        for san in TOKEN.sub(" ", text).split()[:max_ply]:
            try:
                board.push_san(san)
            except ValueError:
                break
            if chess.popcount(board.occupied) < MIN_MEN:
                break
            seen.add((fnv1a64(board.board_fen()) & ~1) | (0 if board.turn else 1))
        keys.extend(seen)
    return np.array(keys, dtype=np.uint64), len(batch)


def merge(keys, counts, fresh):
    keys = np.concatenate([keys, fresh])
    counts = np.concatenate([counts, np.ones(len(fresh), np.uint32)])
    order = np.argsort(keys, kind="stable")
    keys, counts = keys[order], counts[order]
    unique, first = np.unique(keys, return_index=True)
    return unique, np.add.reduceat(counts, first).astype(np.uint32)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--min-games", type=int, default=5)
    parser.add_argument("--max-ply", type=int, default=80)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--counts", help="also save keys seen in >= 2 games with partner flags")
    parser.add_argument("out")
    parser.add_argument("sources", nargs="+")
    args = parser.parse_args()
    start, games, pending, held = time.time(), 0, [], 0
    keys, counts = np.zeros(0, np.uint64), np.zeros(0, np.uint32)
    with mp.Pool(args.workers) as pool:
        work = ((batch, args.max_ply) for batch in batches(args.sources))
        for found, n in pool.imap_unordered(game_keys, work):
            games += n
            pending.append(found)
            held += len(found)
            if games % 100_000 < n:
                print(games, "games", round(time.time() - start), "s", flush=True)
            if held > 30_000_000:
                keys, counts = merge(keys, counts, np.concatenate(pending))
                pending, held = [], 0
                print(games, "games", len(keys), "keys", round(time.time() - start), "s", flush=True)
    if pending:
        keys, counts = merge(keys, counts, np.concatenate(pending))
    partner = np.isin(keys ^ np.uint64(1), keys)
    if args.counts:
        keep = counts >= 2
        np.savez(args.counts, keys=keys[keep], counts=counts[keep], partner=partner[keep],
                 singletons=np.array([int((counts == 1).sum())]), games=np.array([games]))
    # Per placement: games with White to move + games with Black to move.
    placement = keys & ~np.uint64(1)
    unique, first = np.unique(placement, return_index=True)
    total = np.add.reduceat(counts, first)
    common = np.isin(placement, unique[total >= args.min_games])
    chosen = keys[common]
    entries = np.unique(((chosen >> np.uint64(17)) << np.uint64(1)) | (chosen & np.uint64(1)))
    with open(args.out, "wb") as out:
        out.write(b"WPGS" + struct.pack("<BHI", 1, args.min_games, len(entries)))
        for entry in entries.tolist():
            out.write(entry.to_bytes(6, "big"))
    print("done", games, "games", len(keys), "keys", len(entries), "entries",
          round(time.time() - start), "s", flush=True)


if __name__ == "__main__":
    sys.exit(main())
