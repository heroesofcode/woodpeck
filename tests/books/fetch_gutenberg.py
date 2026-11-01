#!/usr/bin/env python3
"""Download public-domain evaluation inputs into ignored target storage; verify pinned bytes.
Pass --from-dir DIR to use an existing download without network access.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import tomllib
import urllib.request

base = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--from-dir', type=Path)
args = parser.parse_args()
cache = base.parents[1] / 'target' / 'books-gutenberg'
cache.mkdir(parents=True, exist_ok=True)
for entry in tomllib.loads((base / 'gutenberg.toml').read_text())['books']:
    name = f"pg{entry['id']}.epub"
    destination = cache / name
    if args.from_dir:
        data = (args.from_dir / name).read_bytes()
    elif destination.exists():
        data = destination.read_bytes()
    else:
        with urllib.request.urlopen(entry['url'], timeout=90) as response:
            data = response.read()
    if hashlib.sha256(data).hexdigest() != entry['sha256']:
        raise SystemExit(f'{name}: SHA-256 mismatch; downloaded edition changed, review annotations before updating pin')
    destination.write_bytes(data)
for source in (base / 'gutenberg').glob('*.json'):
    if source.name != 'metrics.json':
        shutil.copyfile(source, cache / source.name)
print(f'Evaluate with: woodpeck book eval {cache.relative_to(base.parents[1])}')
