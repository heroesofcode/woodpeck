//! Bind a strict side-to-move caption to each diagram from poppler page geometry.
//!
//! The glyph extractor sometimes emits letters out of order ("White o t omve"), so the
//! resolver never sees the caption, and a page with two sides cannot take one page-level
//! answer. `pdftohtml -xml` (zoom 1, the same poppler family as the textless-page stamp)
//! places each text line. Image boxes come from the content stream, in the same page space.
//! The caption is the first strict side line just under that image, or the first line of the
//! next column or page when the diagram ends its column. A line closer to the next diagram
//! is left for that diagram. Two captions that disagree abstain. Move text is not a side.

use super::pdf::{PageBox, diagram_png};
use super::picture::Placed;
use crate::books::resolve::strict_side_line;
use shakmaty::Color;
use std::path::{Path, PathBuf};

/// How far a caption may sit above the image bottom and still count as under it.
const NEAR: f64 = 6.0;
/// How far below the image (or below the first line of the next column) a caption is sought.
const WINDOW: f64 = 72.0;
/// Lines inspected under one diagram before the zone is no longer "just below".
const MAX_LINES: usize = 6;
/// A side line this close to the next diagram introduces that diagram, not the previous one.
const ABOVE_CAPTION_GAP: f64 = 36.0;

/// Read poppler geometry and store each diagram's caption side on the picture.
/// Missing `pdftohtml`, a rotated page, or any doubt leaves that picture untouched.
pub(super) fn stamp_geometry_sides(
    pdf: &Path,
    pictures: &mut [Vec<Placed>],
    boxes: &[PageBox],
    cover: Option<&[u8]>,
) {
    let Some(xml) = pdftohtml_xml(pdf) else {
        return;
    };
    let bound = bind_xml(&xml, pictures, boxes);
    for (page, sides) in pictures.iter_mut().zip(bound) {
        for (pic, side) in page.iter_mut().zip(sides) {
            let Some(white) = side else {
                continue;
            };
            if cover.is_some_and(|c| c == pic.bytes.as_slice()) {
                continue;
            }
            let Some(png) = diagram_png(&pic.bytes) else {
                continue;
            };
            pic.bytes = crate::books::diagram::with_geometry_side(&png, white);
        }
    }
}

fn pdftohtml_xml(pdf: &Path) -> Option<String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("woodpeck-geo-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let _cleanup = DirGuard(dir.clone());
    let base = dir.join("pages");
    let ran = std::process::Command::new("pdftohtml")
        .args(["-xml", "-zoom", "1", "-nodrm", "-q", "-enc", "UTF-8"])
        .arg(pdf)
        .arg(&base)
        .output()
        .ok()?;
    let xml = std::fs::read_to_string(dir.join("pages.xml")).ok()?;
    (ran.status.success() || xml.contains("<pdf2xml"))
        .then_some(xml)
        .filter(|xml| xml.contains("<pdf2xml"))
}

struct DirGuard(PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f64,
    /// Top edge. Larger y is lower on the page, matching pdftohtml.
    y0: f64,
    x1: f64,
    y1: f64,
}

#[derive(Clone)]
struct TextLine {
    rect: Rect,
    text: String,
}

struct PopplerPage {
    number: usize,
    width: f64,
    height: f64,
    lines: Vec<TextLine>,
}

struct PageIn {
    width: f64,
    upright: bool,
    lines: Vec<TextLine>,
    /// `None` when this picture has no usable box.
    images: Vec<Option<Rect>>,
}

struct Item {
    rect: Rect,
    kind: Kind,
}

enum Kind {
    Image,
    Line(String),
}

struct Column {
    items: Vec<Item>,
}

struct Spot {
    page: usize,
    index: usize,
    flow: usize,
    rect: Rect,
}

fn bind_xml(xml: &str, pictures: &[Vec<Placed>], boxes: &[PageBox]) -> Vec<Vec<Option<bool>>> {
    let poppler = parse_poppler(xml);
    bind_pages(&pages_in(pictures, boxes, &poppler))
}

fn pages_in(pictures: &[Vec<Placed>], boxes: &[PageBox], poppler: &[PopplerPage]) -> Vec<PageIn> {
    pictures
        .iter()
        .enumerate()
        .map(|(i, pics)| {
            let abstain = || PageIn {
                width: 0.0,
                upright: false,
                lines: Vec::new(),
                images: vec![None; pics.len()],
            };
            let Some(pop) = poppler.iter().find(|p| p.number == i + 1) else {
                return abstain();
            };
            // A quarter turn swaps poppler's axes with the content stream. Abstain.
            let Some(crop) = boxes.get(i).filter(|box_| box_.upright) else {
                return abstain();
            };
            let Some((sx, sy, origin_x, origin_top)) = page_scale(crop, pop) else {
                return abstain();
            };
            PageIn {
                width: pop.width,
                upright: true,
                lines: merge_lines(pop.lines.clone()),
                images: pics
                    .iter()
                    .map(|pic| map_picture(pic, sx, sy, origin_x, origin_top))
                    .collect(),
            }
        })
        .collect()
}

/// `(scale x, scale y, crop left, crop top)` into pdftohtml's top-left space.
/// `None` when the crop box and the poppler page disagree enough that a caption could
/// attach to the wrong diagram. A missing crop falls back to a page rooted at the origin.
fn page_scale(crop: &PageBox, pop: &PopplerPage) -> Option<(f64, f64, f64, f64)> {
    let (pw, ph) = (crop.x1 - crop.x0, crop.y1 - crop.y0);
    if pw <= 1.0 || ph <= 1.0 {
        return Some((1.0, 1.0, 0.0, pop.height));
    }
    let (sx, sy) = (pop.width / pw, pop.height / ph);
    ((sx - 1.0).abs() <= 0.08 && (sy - 1.0).abs() <= 0.08).then_some((sx, sy, crop.x0, crop.y1))
}

fn map_picture(pic: &Placed, sx: f64, sy: f64, origin_x: f64, origin_top: f64) -> Option<Rect> {
    let (w, h) = (pic.right - pic.left, pic.top - pic.bottom);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    Some(Rect {
        x0: (pic.left - origin_x) * sx,
        x1: (pic.right - origin_x) * sx,
        y0: (origin_top - pic.top) * sy,
        y1: (origin_top - pic.bottom) * sy,
    })
}

fn bind_pages(pages: &[PageIn]) -> Vec<Vec<Option<bool>>> {
    let mut flow: Vec<Column> = Vec::new();
    let mut spots: Vec<Spot> = Vec::new();
    for (page_i, page) in pages.iter().enumerate() {
        if !page.upright {
            continue;
        }
        let (cols, placed) = split_columns(page);
        let base = flow.len();
        flow.extend(cols);
        for (index, col, rect) in placed {
            spots.push(Spot {
                page: page_i,
                index,
                flow: base + col,
                rect,
            });
        }
    }
    let mut out: Vec<Vec<Option<bool>>> = pages
        .iter()
        .map(|page| vec![None; page.images.len()])
        .collect();
    for spot in spots {
        out[spot.page][spot.index] = caption_for(&spot, &flow);
    }
    out
}

fn split_columns(page: &PageIn) -> (Vec<Column>, Vec<(usize, usize, Rect)>) {
    let gutter = find_gutter(page);
    let mut left = Column { items: Vec::new() };
    let mut right = Column { items: Vec::new() };
    let mut placed = Vec::new();
    let column_of = |rect: Rect| -> usize {
        match gutter {
            Some(g) if rect.x1 <= g + 6.0 => 0,
            Some(g) if rect.x0 >= g - 6.0 => 1,
            Some(g) => usize::from(mid_x(rect) >= g),
            None => 0,
        }
    };
    for (index, rect) in page.images.iter().enumerate() {
        let Some(rect) = *rect else {
            continue;
        };
        let column = column_of(rect);
        push_item(&mut left, &mut right, column, rect, Kind::Image);
        placed.push((index, column, rect));
    }
    for line in &page.lines {
        if gutter.is_some_and(|g| line.rect.x0 < g - 8.0 && line.rect.x1 > g + 8.0) {
            continue;
        }
        push_item(
            &mut left,
            &mut right,
            column_of(line.rect),
            line.rect,
            Kind::Line(line.text.clone()),
        );
    }
    sort_column(&mut left);
    sort_column(&mut right);
    let mut cols = Vec::new();
    if gutter.is_none() {
        cols.push(left);
        return (cols, placed);
    }
    let mut remap = [0, 0];
    if !left.items.is_empty() {
        remap[0] = cols.len();
        cols.push(left);
    }
    if !right.items.is_empty() {
        remap[1] = cols.len();
        cols.push(right);
    }
    for spot in &mut placed {
        spot.1 = remap[spot.1];
    }
    (cols, placed)
}

fn push_item(left: &mut Column, right: &mut Column, column: usize, rect: Rect, kind: Kind) {
    let item = Item { rect, kind };
    if column == 0 {
        left.items.push(item);
    } else {
        right.items.push(item);
    }
}

fn sort_column(column: &mut Column) {
    column.items.sort_by(|a, b| {
        a.rect
            .y0
            .total_cmp(&b.rect.y0)
            .then(a.rect.x0.total_cmp(&b.rect.x0))
    });
}

fn find_gutter(page: &PageIn) -> Option<f64> {
    let mut centers = Vec::new();
    for line in &page.lines {
        if line.rect.x1 - line.rect.x0 < page.width * 0.62 {
            centers.push(mid_x(line.rect));
        }
    }
    for rect in page.images.iter().flatten() {
        if rect.x1 - rect.x0 < page.width * 0.62 {
            centers.push(mid_x(*rect));
        }
    }
    if centers.len() < 3 {
        return None;
    }
    centers.sort_by(f64::total_cmp);
    let mut gaps: Vec<(f64, f64)> = centers
        .windows(2)
        .map(|pair| (pair[1] - pair[0], f64::midpoint(pair[0], pair[1])))
        .collect();
    gaps.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (gap, at) = *gaps.first()?;
    let second = gaps.get(1).map_or(0.0, |g| g.0);
    if second > gap * 0.7 || gap < (page.width * 0.15).max(40.0) {
        return None;
    }
    let left = centers.iter().filter(|c| **c < at).count();
    (left >= 1 && left < centers.len()).then_some(at)
}

fn caption_for(spot: &Spot, flow: &[Column]) -> Option<bool> {
    let column = flow.get(spot.flow)?;
    if let Some(side) = caption_below(spot.rect, column) {
        return Some(side);
    }
    if ends_column(spot.rect, column) {
        return flow.get(spot.flow + 1).and_then(wrapped_caption);
    }
    None
}

fn caption_below(image: Rect, column: &Column) -> Option<bool> {
    let mut lines = Vec::new();
    for item in &column.items {
        if item.rect.y0 < image.y1 - NEAR {
            continue;
        }
        if item.rect.y0 > image.y1 + WINDOW {
            break;
        }
        if matches!(item.kind, Kind::Image) {
            break;
        }
        if !near_x(image, item.rect) {
            continue;
        }
        lines.push(item);
        if lines.len() == MAX_LINES {
            break;
        }
    }
    let (side, caption) = first_side(&lines)?;
    if let Some(next) = next_image_below(image, column) {
        let gap_above = caption.y0 - image.y1;
        let gap_below = next.y0 - caption.y1;
        // Closer to the following diagram: that caption introduces it.
        if gap_above >= gap_below {
            return None;
        }
    }
    Some(side)
}

fn wrapped_caption(column: &Column) -> Option<bool> {
    let mut lines = Vec::new();
    let mut origin = None;
    for item in &column.items {
        if matches!(item.kind, Kind::Image) {
            break;
        }
        let start = *origin.get_or_insert(item.rect.y0);
        if item.rect.y0 > start + WINDOW {
            break;
        }
        lines.push(item);
        if lines.len() == MAX_LINES {
            break;
        }
    }
    let (side, caption) = first_side(&lines)?;
    let introduces_next = column.items.iter().any(|item| {
        matches!(item.kind, Kind::Image)
            && item.rect.y0 + NEAR >= caption.y1
            && item.rect.y0 - caption.y1 < ABOVE_CAPTION_GAP
    });
    (!introduces_next).then_some(side)
}

fn ends_column(image: Rect, column: &Column) -> bool {
    !column
        .items
        .iter()
        .any(|item| item.rect.y0 >= image.y1 - NEAR)
}

fn next_image_below(image: Rect, column: &Column) -> Option<Rect> {
    column.items.iter().find_map(|item| {
        (matches!(item.kind, Kind::Image) && item.rect.y0 >= image.y1 - NEAR).then_some(item.rect)
    })
}

/// The first strict side line in `lines`, skipping diagram numbers. Anything else ends the
/// zone, so a later caption (or a move) cannot name this diagram. Two sides abstain.
fn first_side(lines: &[&Item]) -> Option<(bool, Rect)> {
    let mut found = None;
    for item in lines {
        let Kind::Line(text) = &item.kind else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() || is_diagram_number(text) {
            continue;
        }
        if let Some(side) = strict_side_line(text) {
            let white = side == Color::White;
            if found.is_some_and(|(seen, _)| seen != white) {
                return None;
            }
            if found.is_none() {
                found = Some((white, item.rect));
            }
            continue;
        }
        break;
    }
    found
}

fn is_diagram_number(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 24 {
        return false;
    }
    let text = text.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '.'));
    let words: Vec<&str> = text.split_whitespace().collect();
    let digits = |word: &str| {
        let word = word.trim_matches(|c: char| matches!(c, '.' | ')' | '('));
        !word.is_empty() && word.len() <= 4 && word.chars().all(|c| c.is_ascii_digit())
    };
    match words.as_slice() {
        [n] => digits(n),
        [label, n] => {
            digits(n)
                && matches!(
                    label.trim_end_matches('.').to_ascii_lowercase().as_str(),
                    "no" | "nr" | "n" | "diagram" | "diag" | "fig" | "figure"
                )
        }
        _ => false,
    }
}

fn near_x(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 + 20.0 && b.x0 < a.x1 + 20.0
}

fn mid_x(rect: Rect) -> f64 {
    f64::midpoint(rect.x0, rect.x1)
}

fn merge_lines(mut lines: Vec<TextLine>) -> Vec<TextLine> {
    lines.sort_by(|a, b| {
        a.rect
            .y0
            .total_cmp(&b.rect.y0)
            .then(a.rect.x0.total_cmp(&b.rect.x0))
    });
    let mut out: Vec<TextLine> = Vec::new();
    for line in lines {
        if let Some(last) = out.last_mut() {
            let same_row = (last.rect.y0 - line.rect.y0).abs() < 3.0;
            let follows = line.rect.x0 >= last.rect.x0 - 2.0 && line.rect.x0 - last.rect.x1 < 16.0;
            if same_row && follows {
                if !last.text.ends_with(' ') && !line.text.starts_with(' ') {
                    last.text.push(' ');
                }
                last.text.push_str(line.text.trim());
                last.rect.x1 = last.rect.x1.max(line.rect.x1);
                last.rect.y0 = last.rect.y0.min(line.rect.y0);
                last.rect.y1 = last.rect.y1.max(line.rect.y1);
                continue;
            }
        }
        out.push(line);
    }
    out
}

fn parse_poppler(xml: &str) -> Vec<PopplerPage> {
    let mut pages = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<page ") {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let open = &rest[..=end];
        let Some(number) = attr(open, "number").and_then(|v| v.parse().ok()) else {
            rest = &rest[end + 1..];
            continue;
        };
        let width = attr(open, "width")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let height = attr(open, "height")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let body_at = end + 1;
        let Some(close) = rest[body_at..].find("</page>") else {
            break;
        };
        let body = &rest[body_at..body_at + close];
        pages.push(PopplerPage {
            number,
            width,
            height,
            lines: text_lines(body),
        });
        rest = &rest[body_at + close + "</page>".len()..];
    }
    pages
}

fn text_lines(body: &str) -> Vec<TextLine> {
    let mut lines = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("<text ") {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let open = &rest[..=end];
        let content_at = end + 1;
        let Some(close) = rest[content_at..].find("</text>") else {
            break;
        };
        if let Some(rect) = box_attrs(open) {
            let text = normalize(&plain_text(&decode_entities(
                &rest[content_at..content_at + close],
            )));
            if !text.is_empty() {
                lines.push(TextLine { rect, text });
            }
        }
        rest = &rest[content_at + close + "</text>".len()..];
    }
    lines
}

fn box_attrs(tag: &str) -> Option<Rect> {
    let top: f64 = attr(tag, "top")?.parse().ok()?;
    let left: f64 = attr(tag, "left")?.parse().ok()?;
    let width: f64 = attr(tag, "width")?.parse().ok()?;
    let height: f64 = attr(tag, "height")?.parse().ok()?;
    Some(Rect {
        x0: left,
        y0: top,
        x1: left + width,
        y1: top + height,
    })
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(rel) = tag[from..].find(&key) {
        let at = from + rel;
        let boundary = at == 0
            || tag
                .as_bytes()
                .get(at - 1)
                .is_some_and(u8::is_ascii_whitespace);
        if boundary {
            let value = &tag[at + key.len()..];
            let end = value.find('"')?;
            return Some(&value[..end]);
        }
        from = at + key.len();
    }
    None
}

fn plain_text(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '<' {
            for next in chars.by_ref() {
                if next == '>' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn decode_entities(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let token = &rest[1..end];
        let ch = if let Some(hex) = token
            .strip_prefix("#x")
            .or_else(|| token.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = token.strip_prefix('#') {
            dec.parse().ok().and_then(char::from_u32)
        } else {
            match token {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => None,
            }
        };
        if let Some(ch) = ch {
            out.push(ch);
        } else {
            out.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f64, y: f64, text: &str) -> TextLine {
        TextLine {
            rect: Rect {
                x0: x,
                y0: y,
                x1: x + 90.0,
                y1: y + 11.0,
            },
            text: text.to_string(),
        }
    }

    fn image(x: f64, y: f64) -> Rect {
        Rect {
            x0: x,
            y0: y,
            x1: x + 140.0,
            y1: y + 140.0,
        }
    }

    fn page(width: f64, images: Vec<Option<Rect>>, lines: Vec<TextLine>) -> PageIn {
        PageIn {
            width,
            upright: true,
            lines,
            images,
        }
    }

    #[test]
    fn each_diagram_on_a_two_side_page_keeps_its_own_caption() {
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 160.0)), Some(image(220.0, 160.0))],
            vec![
                line(50.0, 308.0, "White to move"),
                line(50.0, 326.0, "28"),
                line(230.0, 308.0, "Black to move"),
            ],
        )]);
        assert_eq!(bound, vec![vec![Some(true), Some(false)]]);
    }

    #[test]
    fn a_caption_closer_to_the_next_diagram_is_not_taken() {
        // Within the search window, but nearer the diagram below than the one above.
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0)), Some(image(40.0, 270.0))],
            vec![line(50.0, 250.0, "Black to move")],
        )]);
        assert_eq!(bound, vec![vec![None, None]]);
    }

    #[test]
    fn a_caption_just_under_the_diagram_binds_before_the_next_one() {
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0)), Some(image(40.0, 400.0))],
            vec![
                line(50.0, 228.0, "White to move"),
                line(50.0, 548.0, "Black to play"),
            ],
        )]);
        assert_eq!(bound, vec![vec![Some(true), Some(false)]]);
    }

    #[test]
    fn a_diagram_that_ends_a_column_takes_the_next_columns_first_caption() {
        let bound = bind_pages(&[page(
            500.0,
            vec![Some(image(40.0, 480.0)), Some(image(280.0, 220.0))],
            vec![
                line(40.0, 40.0, "A note on the ending"),
                line(40.0, 60.0, "continues here today"),
                line(280.0, 36.0, "Black to move"),
                line(280.0, 56.0, "The rook lift decides"),
            ],
        )]);
        assert_eq!(bound[0][0], Some(false));
    }

    #[test]
    fn a_wrapped_caption_that_introduces_the_next_diagram_abstains() {
        let bound = bind_pages(&[page(
            500.0,
            vec![Some(image(40.0, 480.0)), Some(image(280.0, 52.0))],
            vec![
                line(40.0, 40.0, "A note on the ending"),
                line(40.0, 60.0, "continues here today"),
                line(280.0, 36.0, "Black to move"),
            ],
        )]);
        assert_eq!(bound[0][0], None);
    }

    #[test]
    fn the_first_line_of_the_next_page_can_name_the_diagram() {
        let bound = bind_pages(&[
            page(
                400.0,
                vec![Some(image(40.0, 460.0))],
                vec![line(40.0, 40.0, "Chapter notes")],
            ),
            page(
                400.0,
                vec![],
                vec![
                    line(40.0, 40.0, "White to play"),
                    line(40.0, 60.0, "1. Qh5+"),
                ],
            ),
        ]);
        assert_eq!(bound[0], vec![Some(true)]);
    }

    #[test]
    fn two_captions_in_the_zone_that_disagree_abstain() {
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![
                line(50.0, 228.0, "White to move"),
                line(50.0, 244.0, "Black to move"),
            ],
        )]);
        assert_eq!(bound, vec![vec![None]]);
    }

    #[test]
    fn a_move_stops_the_zone_before_a_later_caption() {
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![
                line(50.0, 228.0, "1. Qh5+"),
                line(50.0, 244.0, "Black to move"),
            ],
        )]);
        assert_eq!(bound, vec![vec![None]]);
    }

    #[test]
    fn a_diagram_number_between_the_board_and_the_caption_is_skipped() {
        let bound = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![line(50.0, 226.0, "28"), line(50.0, 242.0, "White to move")],
        )]);
        assert_eq!(bound, vec![vec![Some(true)]]);
    }

    #[test]
    fn narrated_and_mate_phrases_are_not_side_captions() {
        for text in [
            "White to move the knight",
            "White mates in 2",
            "White moves",
            "It is White to move",
        ] {
            let bound = bind_pages(&[page(
                400.0,
                vec![Some(image(40.0, 80.0))],
                vec![line(50.0, 228.0, text)],
            )]);
            assert_eq!(bound, vec![vec![None]], "{text}");
        }
        let win = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![line(50.0, 228.0, "White to move and win")],
        )]);
        assert_eq!(win, vec![vec![Some(true)]]);
        let german = bind_pages(&[page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![line(50.0, 228.0, "Weiß am Zug")],
        )]);
        assert_eq!(german, vec![vec![Some(true)]]);
    }

    #[test]
    fn a_rotated_page_abstains() {
        let mut turned = page(
            400.0,
            vec![Some(image(40.0, 80.0))],
            vec![line(50.0, 228.0, "White to move")],
        );
        turned.upright = false;
        assert_eq!(bind_pages(&[turned]), vec![vec![None]]);
    }

    #[test]
    fn poppler_xml_binds_each_image_box_from_the_content_stream() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<pdf2xml producer="poppler" version="24.02.0">
<page number="1" position="absolute" top="0" left="0" height="600" width="400">
<text top="322" left="50" width="70" height="10" font="0">White to move</text>
<text top="342" left="50" width="12" height="10" font="0">28</text>
<text top="322" left="230" width="69" height="10" font="0">Black&apos;s move</text>
</page>
</pdf2xml>"#;
        let pictures = vec![vec![
            placed(40.0, 300.0, 180.0, 440.0),
            placed(220.0, 300.0, 360.0, 440.0),
        ]];
        let boxes = vec![PageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 400.0,
            y1: 600.0,
            upright: true,
        }];
        assert_eq!(
            bind_xml(xml, &pictures, &boxes),
            vec![vec![Some(true), Some(false)]]
        );
    }

    #[test]
    fn split_words_on_one_baseline_form_a_single_caption() {
        let xml = r#"<pdf2xml>
<page number="1" height="600" width="400">
<text top="228" left="50" width="40" height="11">White</text>
<text top="228" left="94" width="16" height="11">to</text>
<text top="229" left="114" width="36" height="11">move</text>
</page>
</pdf2xml>"#;
        let pictures = vec![vec![placed(40.0, 400.0, 180.0, 540.0)]];
        // Page height 600: image bottom 400 → y-down 200. The merged caption at 228 sits under it.
        let boxes = vec![PageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 400.0,
            y1: 600.0,
            upright: true,
        }];
        assert_eq!(bind_xml(xml, &pictures, &boxes), vec![vec![Some(true)]]);
    }

    fn placed(left: f64, bottom: f64, right: f64, top: f64) -> Placed {
        Placed {
            top,
            left,
            bottom,
            right,
            area: (right - left) * (top - bottom),
            bytes: Vec::new(),
            rendered: false,
        }
    }

    #[test]
    fn pdftohtml_geometry_binds_two_captions_when_the_binary_is_present() {
        if std::process::Command::new("pdftohtml")
            .arg("-v")
            .output()
            .is_err()
        {
            return;
        }
        let dir = std::env::temp_dir().join(format!("woodpeck-geo-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _cleanup = DirGuard(dir.clone());
        let pdf_path = dir.join("sides.pdf");
        std::fs::write(&pdf_path, two_caption_pdf()).unwrap();
        let mut pictures = vec![vec![
            placed_png(40.0, 300.0, 180.0, 440.0),
            placed_png(220.0, 300.0, 360.0, 440.0),
        ]];
        let boxes = vec![PageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 400.0,
            y1: 600.0,
            upright: true,
        }];
        stamp_geometry_sides(&pdf_path, &mut pictures, &boxes, None);
        let sides: Vec<_> = pictures[0]
            .iter()
            .map(|pic| crate::books::diagram::stated_geometry_side(&pic.bytes))
            .collect();
        assert_eq!(sides, vec![Some(true), Some(false)]);
    }

    fn placed_png(left: f64, bottom: f64, right: f64, top: f64) -> Placed {
        let mut pic = placed(left, bottom, right, top);
        let image = image::GrayImage::from_raw(1, 1, vec![0]).unwrap();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut pic.bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        pic
    }

    /// One page: "White to move" under the left diagram and "Black to move" under the right.
    fn two_caption_pdf() -> Vec<u8> {
        let content = b"BT /F1 11 Tf 50 270 Td (White to move) Tj ET\nBT /F1 11 Tf 230 270 Td (Black to move) Tj ET\n";
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content);
        stream.extend_from_slice(b"endstream");
        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
                .to_vec(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
            stream,
        ];
        let mut out = Vec::from(&b"%PDF-1.4\n"[..]);
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(object);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}
