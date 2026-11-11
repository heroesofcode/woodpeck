//! Certifying near-read boards against the book's own squares.
//!
//! A board the square nets read with one to three doubtful squares is accepted only when every
//! one of its 64 squares matches, by nearest neighbours, squares of boards from the same book
//! whose pieces are independently known: located boards whose exact placement occurred in
//! real games ([`super::game_sides::seen`]). A board no net at any scale reads differently
//! (a net may only fail to name a square) is also accepted when no square's neighbours name
//! another piece, every square some net failed to name is matched outright, and at least half
//! of its squares are matched outright. The templates never change a square: the nets' own
//! reading is kept or the board stays unread.

use super::{cnn, diagram};

/// Neighbours that must all agree with a square's reading.
const NEIGHBOURS: usize = 5;
/// A template square is dropped when this many of its neighbours from other boards differ.
const OUTLIER_VOTES: usize = 3;
/// Accepted distance, as a multiple of the book's 99.5th percentile within the class.
const GUARD: f32 = 1.5;
const PERCENTILE: f32 = 99.5;
/// Fewest clean template squares that calibrate a (piece, square colour) class.
const MIN_CLASS: usize = 5;
/// Pooled side of a square feature (the nets' 32×32 cell averaged in 2×2 blocks).
const SIDE: usize = diagram::CELL / 2;
const MAX_DOUBTFUL: usize = 3;
/// Fewest squares of a consistent board the templates must match outright.
const MIN_MATCHED: usize = 32;
/// Template boards used per book, spread evenly; bounds the quadratic cleaning step.
const MAX_TEMPLATES: usize = 300;

type Feature = Vec<f32>;
/// One located grid of nets-sized cells.
type Grid = Vec<Vec<f32>>;
/// A near reading: labels, pooled square features, and, when no net at any scale named a
/// square differently, which squares every net named alike.
type Near = (Vec<u8>, Vec<Feature>, Option<Vec<bool>>);

/// One template square: label, colour (0 = light), owning board, pixels.
struct Square {
    label: u8,
    dark: bool,
    board: usize,
    feature: Feature,
}

/// Pooled features of one located grid.
fn pooled(cells: &[Vec<f32>]) -> Vec<Feature> {
    cells
        .iter()
        .map(|cell| {
            let mut out = vec![0.0; SIDE * SIDE];
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let at = |dy: usize, dx: usize| cell[(2 * y + dy) * diagram::CELL + 2 * x + dx];
                    out[y * SIDE + x] = (at(0, 0) + at(0, 1) + at(1, 0) + at(1, 1)) / 4.0;
                }
            }
            out
        })
        .collect()
}

/// Features of a template board, taken from the grid (native or downscaled) on which the
/// nets agree most with its placement, so pixels and labels come from the same locate.
fn template_features(bytes: &[u8], labels: &[u8]) -> Option<Vec<Feature>> {
    let scales = diagram::board_cells_scales(bytes);
    let agreement = |cells: &Grid| {
        cnn::builtin().map_or(0, |net| {
            net.classify(cells)
                .iter()
                .zip(labels)
                .filter(|(s, l)| s.label == **l)
                .count()
        })
    };
    let mut best: Option<(usize, &Grid)> = None;
    for cells in &scales {
        let score = agreement(cells);
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, cells));
        }
    }
    best.map(|(_, cells)| pooled(cells))
}

fn dark(index: usize) -> bool {
    (index / 8 + index % 8) % 2 == 1
}

fn distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

/// The `k` nearest of `pool` to `query` as (distance, index), nearest first.
fn nearest<'a>(
    query: &[f32],
    pool: impl Iterator<Item = (usize, &'a Square)>,
    k: usize,
) -> Vec<(f32, usize)> {
    let mut best: Vec<(f32, usize)> = Vec::with_capacity(k + 1);
    for (index, square) in pool {
        let d = distance(query, &square.feature);
        if best.len() == k && d >= best[k - 1].0 {
            continue;
        }
        let at = best.partition_point(|(other, _)| *other <= d);
        best.insert(at, (d, index));
        best.truncate(k);
    }
    best
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn percentile(mut values: Vec<f32>, p: f32) -> f32 {
    values.sort_by(f32::total_cmp);
    let rank = p / 100.0 * (values.len() - 1) as f32;
    let (low, high) = (rank.floor() as usize, rank.ceil() as usize);
    values[low] + (values[high] - values[low]) * (rank - low as f32)
}

/// The nets' best reading of a board with one to three doubtful squares, if any, with the
/// features of the grid it was read from and whether every net at every scale read exactly
/// those 64 labels.
fn near_reading(bytes: &[u8]) -> Option<Near> {
    let nets = [cnn::builtin(), cnn::builtin_prior(), cnn::builtin_hatch()];
    let scales = diagram::board_cells_scales(bytes);
    let mut best: Option<(usize, Vec<u8>, &Grid)> = None;
    let mut readings: Vec<Vec<u8>> = Vec::new();
    for net in nets.into_iter().flatten() {
        for cells in &scales {
            let squares = net.classify(cells);
            readings.push(squares.iter().map(|s| s.label).collect());
            let doubtful = squares
                .iter()
                .filter(|s| {
                    let gate = if matches!(s.label, b'K' | b'Q' | b'k' | b'q') {
                        cnn::MIN_ROYAL_PROBABILITY
                    } else {
                        cnn::MIN_PROBABILITY
                    };
                    s.label == b'?' || s.probability < gate
                })
                .count();
            if best.as_ref().is_none_or(|(b, _, _)| doubtful < *b) {
                best = Some((doubtful, squares.iter().map(|s| s.label).collect(), cells));
            }
        }
    }
    let (doubtful, labels, cells) = best?;
    let consistent = readings.iter().all(|reading| {
        reading.len() == labels.len()
            && reading
                .iter()
                .zip(&labels)
                .all(|(read, label)| read == label || *read == b'?')
    });
    let firm = consistent.then(|| {
        (0..labels.len())
            .map(|index| {
                readings
                    .iter()
                    .all(|reading| reading[index] == labels[index])
            })
            .collect()
    });
    ((1..=MAX_DOUBTFUL).contains(&doubtful) && labels.len() == 64 && !labels.contains(&b'?'))
        .then(|| (labels, pooled(cells), firm))
}

/// Expands a FEN placement to 64 labels, rank 8 first.
fn labels_of(placement: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(64);
    for c in placement.bytes() {
        match c {
            b'/' => {}
            b'1'..=b'8' => out.extend(std::iter::repeat_n(b'.', usize::from(c - b'0'))),
            _ => out.push(c),
        }
    }
    (out.len() == 64).then_some(out)
}

/// Certified placements for `near` images, given template boards `(image, placement)` from
/// the same book. Each result is `None` unless every square passes.
pub fn certify(templates: &[(&[u8], &str)], near: &[&[u8]]) -> Vec<Option<String>> {
    let readings: Vec<Option<Near>> = near.iter().map(|bytes| near_reading(bytes)).collect();
    if readings.iter().all(Option::is_none) {
        return vec![None; near.len()];
    }
    let step = templates.len().div_ceil(MAX_TEMPLATES).max(1);
    let mut squares = Vec::new();
    for (board, (bytes, placement)) in templates.iter().step_by(step).enumerate() {
        let Some(labels) = labels_of(placement) else {
            continue;
        };
        let Some(cells) = template_features(bytes, &labels) else {
            continue;
        };
        for (index, feature) in cells.into_iter().enumerate() {
            squares.push(Square {
                label: labels[index],
                dark: dark(index),
                board,
                feature,
            });
        }
    }
    if squares.is_empty() {
        return vec![None; near.len()];
    }
    // Clean each colour class and calibrate the distance guard per (label, colour).
    let mut keep = vec![true; squares.len()];
    let mut within: std::collections::HashMap<(u8, bool), Vec<f32>> =
        std::collections::HashMap::new();
    for (index, square) in squares.iter().enumerate() {
        let others = squares
            .iter()
            .enumerate()
            .filter(|(_, other)| other.dark == square.dark && other.board != square.board);
        let neighbours = nearest(&square.feature, others, NEIGHBOURS);
        let differ = neighbours
            .iter()
            .filter(|(_, n)| squares[*n].label != square.label)
            .count();
        if differ >= OUTLIER_VOTES {
            keep[index] = false;
        } else if let Some((d, n)) = neighbours.first()
            && squares[*n].label == square.label
        {
            within
                .entry((square.label, square.dark))
                .or_default()
                .push(*d);
        }
    }
    let guard: std::collections::HashMap<(u8, bool), f32> = within
        .into_iter()
        .filter(|(_, d)| d.len() >= MIN_CLASS)
        .map(|(class, d)| (class, GUARD * percentile(d, PERCENTILE)))
        .collect();
    readings
        .into_iter()
        .map(|found| {
            let (reading, cells, firm) = found?;
            let mut matched = 0;
            for (index, feature) in cells.iter().enumerate() {
                let pool = squares
                    .iter()
                    .enumerate()
                    .filter(|(i, s)| keep[*i] && s.dark == dark(index));
                let neighbours = nearest(feature, pool, NEIGHBOURS);
                // The neighbours name a piece only when all agree and the nearest is within
                // that piece's guard; otherwise they abstain.
                let named = (neighbours.len() == NEIGHBOURS)
                    .then(|| squares[neighbours[0].1].label)
                    .filter(|voted| neighbours.iter().all(|(_, n)| squares[*n].label == *voted))
                    .filter(|voted| {
                        guard
                            .get(&(*voted, dark(index)))
                            .is_some_and(|limit| neighbours[0].0 <= *limit)
                    });
                match named {
                    Some(voted) if voted == reading[index] => matched += 1,
                    // Another piece named, or an abstention on a square some net failed to name
                    // or on a board the nets read differently.
                    Some(_) => return None,
                    None if !firm.as_ref().is_some_and(|firm| firm[index]) => return None,
                    None => {}
                }
            }
            (matched == 64 || matched >= MIN_MATCHED)
                .then(|| diagram::placement_of(&reading))
                .flatten()
                .filter(|p| plausible(p))
        })
        .collect()
}

/// One king each, no pawn on a back rank, and a legal position for some side to move.
fn plausible(placement: &str) -> bool {
    let Some(labels) = labels_of(placement) else {
        return false;
    };
    if labels.iter().filter(|l| **l == b'K').take(2).count() != 1
        || labels.iter().filter(|l| **l == b'k').take(2).count() != 1
    {
        return false;
    }
    if labels[..8]
        .iter()
        .chain(&labels[56..])
        .any(|l| matches!(l, b'P' | b'p'))
    {
        return false;
    }
    ["w", "b"]
        .iter()
        .any(|side| super::resolve::position(&format!("{placement} {side} - - 0 1")).is_some())
}

#[cfg(test)]
mod tests {
    use super::{labels_of, percentile, plausible};

    #[test]
    fn placements_expand_to_64_labels() {
        let start = labels_of("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR").unwrap();
        assert_eq!(start.len(), 64);
        assert_eq!(&start[..8], b"rnbqkbnr");
        assert_eq!(start[20], b'.');
        assert!(labels_of("8/8").is_none());
    }

    #[test]
    fn plausibility_needs_kings_and_no_back_rank_pawns() {
        assert!(plausible("4k3/8/8/8/8/8/4P3/4K3"));
        assert!(!plausible("8/8/8/8/8/8/4P3/4K3"));
        assert!(!plausible("4k2P/8/8/8/8/8/8/4K3"));
    }

    #[test]
    fn percentile_interpolates() {
        assert!((percentile(vec![0.0, 1.0, 2.0, 3.0, 4.0], 50.0) - 2.0).abs() < 1e-6);
        assert!((percentile(vec![0.0, 10.0], 99.5) - 9.95).abs() < 1e-4);
    }
}
