//! Rebuilds `src/books/diagram_prototypes.bin` from labelled book diagrams and synthetic boards.
//!
//! `cargo run --release --locked --example train_diagrams -- <dir with pgN.epub>`
//!   [--report] prints per-image recognition statistics with the bundled prototypes.
//!   [--dump <book> <out>] writes located square images (u8, 64x32x32 per board) for labelling.
//!
//! Labels live in tests/books/diagram-labels/pgN.txt: image file suffix, then eight ranks
//! ('.' empty, 'x' excluded). The 15 transcribed pg33870 test diagrams are never listed.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
#[path = "support/diagram_render.rs"]
mod render;
use std::{collections::BTreeMap, io::Read, path::Path};
use woodpeck::books::diagram::{self, Prototypes};

type Sample = (u8, bool, Vec<f32>);
type Images = Vec<(String, Vec<u8>)>;

fn xorshift(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn images(epub: &Path) -> Result<Images, Box<dyn std::error::Error>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub)?)?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name()?.into_owned();
        if [".jpg", ".jpeg", ".png"].iter().any(|e| name.ends_with(e)) {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            out.push((name, bytes));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn labelled(dir: &Path) -> Result<Vec<Sample>, Box<dyn std::error::Error>> {
    let labels = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/books/diagram-labels");
    let mut samples = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(labels)?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for file in files {
        let book = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let images = images(&dir.join(format!("{book}.epub")))?;
        let mut used = 0;
        for line in std::fs::read_to_string(&file)?.lines() {
            let Some((suffix, board)) = line.split_once(' ') else {
                continue;
            };
            let squares: Vec<u8> = board.bytes().filter(|b| *b != b'/').collect();
            let Some((_, bytes)) = images
                .iter()
                .find(|(name, _)| name.ends_with(&format!("_{suffix}")))
            else {
                return Err(format!("{book}: missing {suffix}").into());
            };
            let Some(features) = diagram::board_features(bytes) else {
                eprintln!("{book}: no board in {suffix}");
                continue;
            };
            used += 1;
            for (i, (label, f)) in squares.iter().zip(features).enumerate() {
                if *label != b'x' {
                    samples.push((*label, (i / 8 + i % 8) % 2 == 1, f));
                }
            }
        }
        eprintln!("{book}: {used} diagrams");
    }
    Ok(samples)
}

/// Random placements in both synthetic looks; the seed differs from the eval fixtures.
fn synthetic() -> Vec<Sample> {
    let mut samples = Vec::new();
    let mut seed = 0x9e37_79b9_7f4a_7c15;
    for n in 0..48 {
        let mut placement = String::new();
        let mut squares = Vec::new();
        for row in 0..8 {
            let mut empty = 0;
            for _ in 0..8 {
                let piece = (xorshift(&mut seed) % 100 < 40)
                    .then(|| b"PNBRQKpnbrqk"[(xorshift(&mut seed) % 12) as usize]);
                squares.push(piece.unwrap_or(b'.'));
                if let Some(p) = piece {
                    if empty > 0 {
                        placement.push_str(&empty.to_string());
                        empty = 0;
                    }
                    placement.push(char::from(p));
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                placement.push_str(&empty.to_string());
            }
            if row < 7 {
                placement.push('/');
            }
        }
        let style = if n % 2 == 0 {
            render::Style::Screen
        } else {
            render::Style::Printed
        };
        let png = render::render(&placement, style, 28 + (n % 5) * 6, seed);
        let features = diagram::board_features(&png).expect("synthetic board is located");
        for (i, (label, f)) in squares.iter().zip(features).enumerate() {
            samples.push((*label, (i / 8 + i % 8) % 2 == 1, f));
        }
    }
    samples
}

fn distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// Deterministic k-means++ and Lloyd iterations.
fn centroids(points: &[&Vec<f32>], k: usize, seed: &mut u64) -> Vec<Vec<f32>> {
    if points.len() <= k {
        return points.iter().map(|p| (*p).clone()).collect();
    }
    let mut centers = vec![points[(xorshift(seed) % points.len() as u64) as usize].clone()];
    while centers.len() < k {
        let d: Vec<f32> = points
            .iter()
            .map(|p| {
                centers
                    .iter()
                    .map(|c| distance(p, c))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let total: f32 = d.iter().sum();
        let mut target = (xorshift(seed) % 1_000_000) as f32 / 1_000_000.0 * total;
        let mut pick = points.len() - 1;
        for (i, v) in d.iter().enumerate() {
            if target < *v {
                pick = i;
                break;
            }
            target -= v;
        }
        centers.push(points[pick].clone());
    }
    for _ in 0..25 {
        let mut sums = vec![vec![0.0f32; points[0].len()]; k];
        let mut counts = vec![0usize; k];
        for p in points {
            let best = (0..k)
                .min_by(|a, b| distance(p, &centers[*a]).total_cmp(&distance(p, &centers[*b])))
                .unwrap_or(0);
            counts[best] += 1;
            for (s, v) in sums[best].iter_mut().zip(p.iter()) {
                *s += v;
            }
        }
        for (c, (sum, n)) in centers.iter_mut().zip(sums.into_iter().zip(counts)) {
            if n > 0 {
                *c = sum.into_iter().map(|v| v / n as f32).collect();
            }
        }
    }
    centers
}

fn report(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let prototypes = diagram::builtin().ok_or("missing prototypes")?;
    let mut epubs: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    epubs.sort();
    for epub in epubs
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "epub"))
    {
        let Ok(images) = images(epub) else {
            eprintln!("skipping unreadable {}", epub.display());
            continue;
        };
        for (name, bytes) in images {
            let Some(squares) = prototypes.squares(&bytes) else {
                continue;
            };
            let mut d: Vec<f32> = squares.iter().map(|s| s.distance).collect();
            let mut m: Vec<f32> = squares.iter().map(|s| s.margin).collect();
            d.sort_by(f32::total_cmp);
            m.sort_by(f32::total_cmp);
            let raw: String = squares.iter().map(|s| char::from(s.label)).collect();
            let close: Vec<String> = squares
                .iter()
                .enumerate()
                .filter(|(_, s)| s.margin < 2.0)
                .map(|(i, s)| {
                    format!(
                        "{i}:{}/{}:{:.2}",
                        char::from(s.label),
                        char::from(s.second),
                        s.margin
                    )
                })
                .collect();
            println!(
                "{} {name} med_d={:.2} max_d={:.2} med_m={:.2} min_m={:.2} {} {raw} {}",
                epub.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default(),
                d[32],
                d[63],
                m[32],
                m[0],
                diagram::placement(&squares).unwrap_or_else(|| "-".into()),
                close.join(",")
            );
        }
    }
    Ok(())
}

fn dump(dir: &Path, book: &str, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut names = String::new();
    let mut bytes = Vec::new();
    for (name, image) in images(&dir.join(format!("{book}.epub")))? {
        if let Some(cells) = diagram::board_cells(&image) {
            names.push_str(&name);
            names.push('\n');
            for cell in cells {
                bytes.extend(
                    cell.iter()
                        .map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8),
                );
            }
        }
    }
    std::fs::write(out.with_extension("names"), names)?;
    std::fs::write(out.with_extension("u8"), bytes)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(args.first().ok_or("usage: train_diagrams <epub dir>")?);
    if args.iter().any(|a| a == "--report") {
        return report(dir);
    }
    if let Some(i) = args.iter().position(|a| a == "--dump") {
        let book = args.get(i + 1).ok_or("--dump <book> <out>")?;
        let out = args.get(i + 2).ok_or("--dump <book> <out>")?;
        return dump(dir, book, Path::new(out));
    }
    let mut samples = labelled(dir)?;
    samples.extend(synthetic());
    let mut groups: BTreeMap<(u8, bool), Vec<&Vec<f32>>> = BTreeMap::new();
    for (label, dark, f) in &samples {
        groups.entry((*label, *dark)).or_default().push(f);
    }
    let mut seed = 0x2545_f491_4f6c_dd1d;
    let mut records = Vec::new();
    for ((label, dark), points) in &groups {
        // Empty squares vary most (hatching phase, paper noise), so they get more prototypes.
        let k = if *label == b'.' { 32 } else { 8 };
        for c in centroids(points, k, &mut seed) {
            records.push((*label, *dark, c));
        }
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/books/diagram_prototypes.bin");
    std::fs::write(&out, Prototypes::encode(&records))?;
    eprintln!(
        "{} samples, {} prototypes -> {}",
        samples.len(),
        records.len(),
        out.display()
    );
    Ok(())
}
