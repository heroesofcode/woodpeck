//! Ground truth is authored independently of notation parsing. Only annotated ranges in sampled
//! books count toward precision; generated fixtures set `complete` to include all emitted moves.
use super::{document::BlockKind, import, model::Status, resolve};
use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Position, uci::UciMove};
use std::{error::Error, path::Path, time::Instant};

#[derive(Debug, Deserialize)]
pub struct Truth {
    pub file: String,
    pub notation: String,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub scanned: bool,
    #[serde(default)]
    pub fragments: Vec<ExpectedFragment>,
    #[serde(default)]
    pub diagrams: Vec<String>,
    /// Transcribed diagram images, matched by the end of their asset path.
    #[serde(default)]
    pub diagram_sources: Vec<DiagramSource>,
}
#[derive(Debug, Deserialize)]
pub struct DiagramSource {
    pub asset_suffix: String,
    pub fen: String,
}
#[derive(Debug, Deserialize)]
pub struct ExpectedFragment {
    pub page: usize,
    /// Unique exact excerpt identifying an annotated source range within this page.
    pub anchor: String,
    #[serde(default)]
    pub occurrence: Option<usize>,
    pub start_fen: String,
    pub moves: Vec<String>,
    /// Parent indices within this fragment; omission means a simple mainline.
    #[serde(default)]
    pub parents: Option<Vec<Option<usize>>>,
    #[serde(default)]
    pub unresolved: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rates {
    pub move_recall: f64,
    pub move_precision: f64,
    pub wrong_position_rate: f64,
    pub position_accuracy: f64,
    pub diagram_accuracy: f64,
    pub resolved_fraction: f64,
    pub partial_fraction: f64,
    pub unresolved_fraction: f64,
    pub unresolved_correctly: f64,
    pub ms_per_page: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    #[serde(default)]
    pub coverage: super::coverage::Coverage,
    pub name: String,
    pub notation: String,
    pub expected: usize,
    pub emitted: usize,
    pub correct: usize,
    pub wrong_positions: usize,
    pub expected_diagrams: usize,
    pub correct_diagrams: usize,
    /// Images in the book, and those read as diagrams.
    #[serde(default)]
    pub images: usize,
    #[serde(default)]
    pub images_recognized: usize,
    /// Transcribed images: read exactly, read differently (must stay 0), squares matching.
    #[serde(default)]
    pub labelled_images: usize,
    #[serde(default)]
    pub labelled_correct: usize,
    #[serde(default)]
    pub labelled_wrong: usize,
    #[serde(default)]
    pub labelled_squares: usize,
    pub resolved: usize,
    pub partial: usize,
    pub unresolved: usize,
    pub broken: usize,
    pub broken_correct: usize,
    pub pages: usize,
    pub import_ms: f64,
    pub failures: Vec<String>,
    pub fallbacks: std::collections::BTreeMap<String, usize>,
    pub rates: Rates,
}
impl Metrics {
    pub fn recall(&self) -> f64 {
        ratio(self.correct, self.expected)
    }
    pub fn precision(&self) -> f64 {
        ratio(self.correct, self.emitted)
    }
    fn refresh(&mut self) {
        let total = self.resolved + self.partial + self.unresolved;
        self.rates = Rates {
            move_recall: self.recall(),
            move_precision: self.precision(),
            wrong_position_rate: if self.emitted == 0 {
                0.0
            } else {
                ratio(self.wrong_positions, self.emitted)
            },
            position_accuracy: ratio(self.correct, self.emitted),
            diagram_accuracy: ratio(self.correct_diagrams, self.expected_diagrams),
            resolved_fraction: if total == 0 {
                0.0
            } else {
                ratio(self.resolved, total)
            },
            partial_fraction: if total == 0 {
                0.0
            } else {
                ratio(self.partial, total)
            },
            unresolved_fraction: if total == 0 {
                0.0
            } else {
                ratio(self.unresolved, total)
            },
            unresolved_correctly: ratio(self.broken_correct, self.broken),
            ms_per_page: self.import_ms / self.pages.max(1) as f64,
        };
    }
    fn add(&mut self, other: &Self) {
        self.coverage.add(&other.coverage);
        self.expected += other.expected;
        self.emitted += other.emitted;
        self.correct += other.correct;
        self.wrong_positions += other.wrong_positions;
        self.expected_diagrams += other.expected_diagrams;
        self.correct_diagrams += other.correct_diagrams;
        self.images += other.images;
        self.images_recognized += other.images_recognized;
        self.labelled_images += other.labelled_images;
        self.labelled_correct += other.labelled_correct;
        self.labelled_wrong += other.labelled_wrong;
        self.labelled_squares += other.labelled_squares;
        self.resolved += other.resolved;
        self.partial += other.partial;
        self.unresolved += other.unresolved;
        self.broken += other.broken;
        self.broken_correct += other.broken_correct;
        self.pages += other.pages;
        self.import_ms += other.import_ms;
    }
}
/// Board squares of a FEN placement, rank 8 first.
fn squares(fen: &str) -> Vec<char> {
    let placement = fen.split(' ').next().unwrap_or_default();
    let mut out = Vec::with_capacity(64);
    for c in placement.chars().filter(|c| *c != '/') {
        match c.to_digit(10) {
            Some(n) => out.extend(std::iter::repeat_n('.', n as usize)),
            None => out.push(c),
        }
    }
    out
}
fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 1.0 } else { n as f64 / d as f64 }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub overall: Metrics,
    pub fixtures: Vec<Metrics>,
    pub notations: Vec<Metrics>,
}
impl Report {
    pub fn markdown(&self) -> String {
        let mut s = String::from(
            "| Fixture | Notation | Expected | Emitted | Recall | Precision | Wrong positions | Diagrams | R/P/U | Broken caught | ms/page |\n|---|---|---:|---:|---:|---:|---:|---:|---|---|---:|\n",
        );
        for m in std::iter::once(&self.overall)
            .chain(&self.fixtures)
            .chain(&self.notations)
        {
            use std::fmt::Write;
            writeln!(
                s,
                "| {} | {} | {} | {} | {:.1}% | {:.1}% | {} | {}/{} | {}/{}/{} | {}/{} | {:.2} |",
                m.name,
                m.notation,
                m.expected,
                m.emitted,
                100.0 * m.recall(),
                100.0 * m.precision(),
                m.wrong_positions,
                m.correct_diagrams,
                m.expected_diagrams,
                m.resolved,
                m.partial,
                m.unresolved,
                m.broken_correct,
                m.broken,
                m.import_ms / m.pages.max(1) as f64
            )
            .expect("write to string");
        }
        s.push_str("\nWhole-book surface-token coverage (independent conservative detector; fragment membership, not move accuracy):\n\n| Book | Move-like tokens | Resolved | Partial | Unresolved |\n|---|---:|---:|---:|---:|\n");
        for m in &self.fixtures {
            use std::fmt::Write;
            let c = &m.coverage;
            writeln!(
                s,
                "| {} | {} | {} ({:.2}%) | {} ({:.2}%) | {} ({:.2}%) |",
                m.name,
                c.tokens,
                c.resolved,
                100.0 * ratio(c.resolved, c.tokens.max(1)),
                c.partial,
                100.0 * ratio(c.partial, c.tokens.max(1)),
                c.unresolved,
                100.0 * ratio(c.unresolved, c.tokens.max(1))
            )
            .expect("string write");
        }
        s.push_str("\nDiagram images (read from pixels; labelled = transcribed images, wrong must be 0):\n\n| Book | Images | Read as diagram | Labelled | Correct | Wrong | Abstained | Square accuracy |\n|---|---:|---:|---:|---:|---:|---:|---:|\n");
        for m in std::iter::once(&self.overall).chain(&self.fixtures) {
            use std::fmt::Write;
            let read = m.labelled_correct + m.labelled_wrong;
            writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} | {:.1}% |",
                m.name,
                m.images,
                m.images_recognized,
                m.labelled_images,
                m.labelled_correct,
                m.labelled_wrong,
                m.labelled_images - read,
                100.0 * ratio(m.labelled_squares, read * 64)
            )
            .expect("string write");
        }
        s.push_str("\nRanked failure categories (partial/unresolved surface tokens; mutually exclusive):\n\n");
        let mut categories: Vec<_> = self.overall.coverage.reasons.iter().collect();
        categories.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (reason, count) in categories {
            use std::fmt::Write;
            writeln!(s, "- {reason}: {count}").expect("string write");
            if let Some(examples) = self.overall.coverage.examples.get(reason) {
                for example in examples {
                    writeln!(s, "  - `{}`", example.replace('`', "'")).expect("string write");
                }
            }
        }
        s.push_str("\nExact diagnostic reason counts per book are in metrics.json under coverage.diagnostics. Non-detected tokens are included as unresolved; years and bare page numbers are excluded. Coordinate references in prose can remain in this denominator. Partial denotes the status of the containing fragment, including its validated prefix.\n");
        s
    }
}
pub fn run(dir: &Path) -> Result<Report, Box<dyn Error>> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name().is_none_or(|n| n != "metrics.json")
        })
        .collect();
    paths.sort();
    let mut report = Report {
        overall: Metrics {
            name: "Overall".into(),
            ..Metrics::default()
        },
        fixtures: vec![],
        notations: vec![],
    };
    for path in paths {
        let truth: Truth = serde_json::from_slice(&std::fs::read(path)?)?;
        let metrics = evaluate(&dir.join(&truth.file), &truth)?;
        report.overall.add(&metrics);
        report.fixtures.push(metrics);
    }
    // Unannotated books still participate in whole-book coverage. No oracle is invented.
    let mut unannotated: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "epub" || e == "pdf"))
        .filter(|p| {
            p.file_name().is_some_and(|name| {
                !report
                    .fixtures
                    .iter()
                    .any(|m| m.name == name.to_string_lossy())
            })
        })
        .collect();
    unannotated.sort();
    for path in unannotated {
        let truth = Truth {
            file: path
                .file_name()
                .ok_or("Missing filename")?
                .to_string_lossy()
                .into_owned(),
            notation: "unannotated coverage only".into(),
            complete: false,
            scanned: false,
            fragments: vec![],
            diagrams: vec![],
            diagram_sources: vec![],
        };
        let metrics = evaluate(&path, &truth)?;
        report.overall.add(&metrics);
        report.fixtures.push(metrics);
    }
    if report.fixtures.is_empty() {
        return Err("No EPUB/PDF books or ground truth JSON files found".into());
    }
    let mut groups = std::collections::BTreeMap::new();
    for metrics in &report.fixtures {
        groups
            .entry(metrics.notation.clone())
            .or_insert_with(|| Metrics {
                name: format!("Notation: {}", metrics.notation),
                ..Metrics::default()
            })
            .add(metrics);
    }
    report.notations = groups
        .into_values()
        .map(|mut m| {
            m.refresh();
            m
        })
        .collect();
    report.overall.refresh();
    std::fs::write(dir.join("report.md"), report.markdown())?;
    std::fs::write(
        dir.join("metrics.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
#[allow(clippy::too_many_lines)] // Keep annotation scoring and independent edge validation together.
pub fn evaluate(path: &Path, truth: &Truth) -> Result<Metrics, Box<dyn Error>> {
    let started = Instant::now();
    let imported = import::load(path);
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let mut metrics = Metrics {
        name: truth.file.clone(),
        notation: truth.notation.clone(),
        import_ms: elapsed,
        ..Metrics::default()
    };
    if truth.scanned {
        metrics.broken = 1;
        metrics.broken_correct =
            usize::from(matches!(imported, Err(import::ImportError::NoTextLayer)));
        metrics.refresh();
        return Ok(metrics);
    }
    let book = imported?;
    metrics.pages = book.document.pages.len();
    let mut scoped = std::collections::BTreeSet::new();
    let mut oracle_ranges = Vec::new();
    for expected in &truth.fragments {
        metrics.expected += expected.moves.len();
        let page = book
            .document
            .pages
            .get(expected.page)
            .ok_or("Ground truth page out of range")?;
        let Some(start) = page
            .text
            .match_indices(&expected.anchor)
            .nth(expected.occurrence.unwrap_or(0))
            .map(|(i, _)| i)
        else {
            metrics.failures.push(format!(
                "Missing anchor on page {}: {}",
                expected.page, expected.anchor
            ));
            continue;
        };
        if expected.occurrence.is_none() && page.text.matches(&expected.anchor).count() != 1 {
            return Err(format!("Nonunique ground truth anchor: {}", expected.anchor).into());
        }
        let end = start + expected.anchor.len();
        if oracle_ranges
            .iter()
            .any(|&(page, lo, hi)| page == expected.page && lo < end && start < hi)
        {
            return Err("Overlapping ground truth excerpts would double-count moves".into());
        }
        oracle_ranges.push((expected.page, start, end));
        let nodes: Vec<_> = book
            .nodes
            .iter()
            .filter(|n| n.page == expected.page && n.span.start >= start && n.span.end <= end)
            .collect();
        for n in &nodes {
            scoped.insert(n.id);
        }
        let mut positions = Vec::new();
        let mut expected_ids = Vec::new();
        for (i, uci) in expected.moves.iter().enumerate() {
            let parent = expected
                .parents
                .as_ref()
                .map_or_else(|| i.checked_sub(1), |p| p[i]);
            let mut pos = if let Some(parent) = parent {
                positions
                    .get(parent)
                    .cloned()
                    .ok_or("Invalid oracle parent index")?
            } else {
                resolve::position(&expected.start_fen).ok_or("Invalid oracle starting FEN")?
            };
            let m = uci
                .parse::<UciMove>()?
                .to_move(&pos)
                .map_err(|_| format!("Illegal oracle move {} at {}", uci, resolve::fen(&pos)))?;
            pos.play_unchecked(m);
            if let Some(actual) = nodes.get(i) {
                let parent_matches =
                    parent.is_none_or(|p| expected_ids.get(p).copied().flatten() == actual.parent);
                if actual.uci == *uci && parent_matches {
                    if actual.fen == resolve::fen(&pos) {
                        metrics.correct += 1;
                    } else {
                        metrics.wrong_positions += 1;
                        metrics
                            .failures
                            .push(format!("Wrong position: {uci} page {}", expected.page));
                    }
                } else {
                    if actual.fen != resolve::fen(&pos) {
                        metrics.wrong_positions += 1;
                    }
                    metrics.failures.push(format!(
                        "Move mismatch page {} index {i}: expected {uci}, got {}",
                        expected.page, actual.uci
                    ));
                }
                expected_ids.push(Some(actual.id));
            } else {
                expected_ids.push(None);
                metrics.failures.push(format!(
                    "Missing move page {}: {uci} ({})",
                    expected.page, expected.anchor
                ));
            }
            positions.push(pos);
        }
        if expected.unresolved {
            metrics.broken += 1;
            metrics.broken_correct += usize::from(book.fragments.iter().any(|f| {
                f.page == expected.page
                    && f.span.start < end
                    && f.span.end > start
                    && f.status != Status::Resolved
            }));
        }
    }
    metrics.emitted = if truth.complete {
        book.nodes.len()
    } else {
        scoped.len()
    };
    for f in &book.fragments {
        if let Status::Partial(reason) | Status::Unresolved(reason) = &f.status {
            *metrics.fallbacks.entry(reason.clone()).or_default() += 1;
        }
        match f.status {
            Status::Resolved => metrics.resolved += 1,
            Status::Partial(_) => metrics.partial += 1,
            Status::Unresolved(_) => metrics.unresolved += 1,
        }
    }
    super::coverage::measure(&book, &mut metrics);
    metrics.expected_diagrams = truth.diagrams.len();
    let blocks = || book.document.pages.iter().flat_map(|p| &p.blocks);
    for expected in &truth.diagrams {
        metrics.correct_diagrams += usize::from(blocks().any(|b| {
            matches!(&b.kind, BlockKind::Diagram { fen } | BlockKind::RecognizedDiagram { fen, .. } if fen == expected)
        }));
    }
    for block in blocks() {
        match &block.kind {
            BlockKind::Image { .. } => metrics.images += 1,
            BlockKind::RecognizedDiagram { .. } => {
                metrics.images += 1;
                metrics.images_recognized += 1;
            }
            _ => {}
        }
    }
    for source in &truth.diagram_sources {
        metrics.labelled_images += 1;
        let read = blocks().find_map(|b| match &b.kind {
            BlockKind::RecognizedDiagram { path, fen, .. }
                if path.ends_with(&source.asset_suffix) =>
            {
                Some(fen)
            }
            _ => None,
        });
        if let Some(fen) = read {
            if *fen == source.fen {
                metrics.labelled_correct += 1;
            } else {
                metrics.labelled_wrong += 1;
                metrics
                    .failures
                    .push(format!("{} read as {fen}", source.asset_suffix));
            }
            metrics.labelled_squares += squares(fen)
                .iter()
                .zip(squares(&source.fen))
                .filter(|(a, b)| **a == *b)
                .count();
        } else {
            metrics
                .failures
                .push(format!("{} not read (abstained)", source.asset_suffix));
        }
    }
    // Recheck every emitted edge, including moves outside annotated excerpts.
    for node in &book.nodes {
        let before = node
            .parent
            .map(|p| book.nodes[p].fen.as_str())
            .or_else(|| {
                book.fragments
                    .iter()
                    .find(|f| f.nodes.contains(&node.id))
                    .map(|f| f.start_fen.as_str())
            })
            .ok_or("Missing root FEN")?;
        let pos = resolve::position(before).ok_or("Invalid emitted FEN")?;
        let m = node.uci.parse::<UciMove>()?.to_move(&pos)?;
        let after = pos.play(m)?;
        if resolve::fen(&after) != node.fen {
            return Err(format!(
                "Invalid emitted tree edge: page {} {} from {before} gives {}",
                node.page, node.uci, node.fen
            )
            .into());
        }
        if m.to_uci(CastlingMode::Standard).to_string() != node.uci {
            return Err("Noncanonical UCI".into());
        }
    }
    metrics.refresh();
    Ok(metrics)
}
