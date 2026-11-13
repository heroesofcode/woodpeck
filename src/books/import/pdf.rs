//! Reconstruct lines from glyph coordinates rather than PDF painting order. A repeated central
//! gutter orders two text columns top-to-bottom; short White/Black move tables stay row-major.
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use std::path::Path;
#[derive(Default)]
struct Layout {
    pages: Vec<Vec<Glyph>>,
}
struct Glyph {
    x: f64,
    y: f64,
    end: f64,
    size: f64,
    text: String,
}
impl OutputDev for Layout {
    fn begin_page(
        &mut self,
        _: u32,
        _: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.pages.push(vec![]);
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _: f64,
        size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        let size = (size * trm.m11).abs().max(1.0);
        if trm.m31.is_finite() && trm.m32.is_finite() && width.is_finite() {
            self.pages.last_mut().expect("page begun").push(Glyph {
                x: trm.m31,
                y: trm.m32,
                end: trm.m31 + width * size,
                size,
                text: text.into(),
            });
        }
        Ok(())
    }
}
/// One reconstructed text line with its dominant glyph size and baseline height.
pub(super) struct Line {
    pub text: String,
    pub size: f64,
    pub y: f64,
    pub x: f64,
    /// Right edge of the last glyph, in the same page space as `x`.
    pub end: f64,
    /// Number of separately spaced cells on the row (tables, side-by-side columns).
    pub cells: usize,
}
/// Text lines per page, plus the outline (level, title, 0-based page) and Info title/author.
pub(super) struct Extracted {
    pub pages: Vec<Vec<Line>>,
    pub outline: Vec<(u8, String, usize)>,
    pub title: Option<String>,
    pub author: Option<String>,
    /// The largest picture on the first page (a scanned or printed cover), as image bytes.
    pub cover: Option<Vec<u8>>,
    /// Pictures per page, top to bottom.
    pub pictures: Vec<Vec<super::picture::Placed>>,
    /// Board crops found by rendering pages (scans, drawings), included in `pictures`.
    pub boards: usize,
}
/// Fewest letter-bearing words for a `pdftotext` page to replace a silent page.
const MIN_FALLBACK_WORDS: usize = 8;

fn has_text(lines: &[Line]) -> bool {
    lines
        .iter()
        .any(|line| line.text.chars().any(char::is_alphanumeric))
}

/// Pages whose glyphs decoded to no text at all (some embedded CID fonts) take their lines
/// from `pdftotext` word boxes instead, so moves printed around a diagram are still read.
/// Pages that already have text, and pages `pdftotext` cannot read either (scans), are left
/// alone. Missing `pdftotext` is a no-op.
pub(super) fn fill_silent_pages(pdf: &Path, extracted: &mut Extracted) {
    if extracted.pages.iter().all(|lines| has_text(lines)) {
        return;
    }
    let Some(words) = super::caption::bbox_document(pdf) else {
        return;
    };
    fill_from_words(&mut extracted.pages, &words);
}

fn fill_from_words(pages: &mut [Vec<Line>], words: &[Vec<super::caption::TextBox>]) {
    for (lines, page_words) in pages.iter_mut().zip(words) {
        if has_text(lines) {
            continue;
        }
        let fillable = page_words
            .iter()
            .filter(|word| word.text.chars().any(char::is_alphabetic))
            .count();
        if fillable < MIN_FALLBACK_WORDS {
            continue;
        }
        let glyphs: Vec<Glyph> = page_words
            .iter()
            .filter_map(|word| {
                let height = word.rect.top - word.rect.bottom;
                if !(height.is_finite() && height > 0.0) {
                    return None;
                }
                // A word box spans descender to ascender; the baseline sits about a fifth up.
                Some(Glyph {
                    x: word.rect.left,
                    y: word.rect.bottom + 0.2 * height,
                    end: word.rect.right,
                    size: height,
                    text: word.text.clone(),
                })
            })
            .collect();
        if !glyphs.is_empty() {
            *lines = order(glyphs);
        }
    }
}

pub(super) fn extract(bytes: &[u8]) -> Result<Extracted, OutputError> {
    let doc = lopdf::Document::load_mem(bytes)?;
    let mut layout = Layout::default();
    // pdf-extract panics on some malformed content streams (a path op with no current
    // point); such a page is kept as an empty page instead of losing the whole book.
    for (i, page) in doc.get_pages().into_keys().enumerate() {
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_extract::output_doc_page(&doc, &mut layout, page)
        }));
        if let Ok(result) = read {
            result?;
        } else {
            layout.pages.truncate(i);
            layout.pages.push(vec![]);
        }
    }
    let outline = doc
        .get_toc()
        .map(|toc| {
            toc.toc
                .into_iter()
                .filter(|t| t.page > 0 && !t.title.trim().is_empty())
                .map(|t| {
                    (
                        u8::try_from(t.level.clamp(1, 6)).unwrap_or(6),
                        t.title.split_whitespace().collect::<Vec<_>>().join(" "),
                        t.page - 1,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let info = |key: &[u8]| {
        let info = doc.trailer.get(b"Info").ok()?;
        let info = match info {
            lopdf::Object::Reference(id) => doc.get_object(*id).ok()?,
            other => other,
        };
        let value = lopdf::decode_text_string(info.as_dict().ok()?.get(key).ok()?).ok()?;
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        (!value.is_empty()).then_some(value)
    };
    let fonts: Vec<Vec<super::fontboard::FontBoard>> = layout
        .pages
        .iter()
        .map(|glyphs| {
            let marks: Vec<super::fontboard::Mark> = glyphs
                .iter()
                .filter_map(|g| {
                    let mut chars = g.text.chars();
                    let ch = chars.next()?;
                    chars.next().is_none().then_some(super::fontboard::Mark {
                        x: g.x,
                        y: g.y,
                        size: g.size,
                        ch,
                    })
                })
                .collect();
            super::fontboard::boards(&marks)
        })
        .collect();
    // A diagram-font board becomes a picture; its glyphs (and its frame rows) are not text.
    for (glyphs, boards) in layout.pages.iter_mut().zip(&fonts) {
        glyphs.retain(|g| !boards.iter().any(|b| b.covers(g.x, g.y, g.size)));
    }
    let (mut pictures, drawn) = pictures(&doc, bytes, &fonts);
    for ((glyphs, placed), drawn) in layout.pages.iter().zip(pictures.iter_mut()).zip(drawn) {
        if drawn {
            rendered_board_led_moves(glyphs, placed);
        }
    }
    let pages: Vec<_> = layout.pages.into_iter().map(order).collect();
    Ok(Extracted {
        outline: chapters(outline, pages.len()),
        pages,
        title: info(b"Title"),
        author: info(b"Author"),
        cover: first_page_picture(&doc),
        boards: pictures.iter().flatten().filter(|p| p.rendered).count(),
        pictures,
    })
}

/// Bottom of the move line over a rendered board, at most this many board widths above
/// the board's top edge.
const LED_GAP_MAX: f64 = 0.3;
/// A gap wider than this many board widths splits a row into text columns.
const COLUMN_GAP: f64 = 0.08;
/// A board's own text column reaches at most this many board widths past either edge.
const COLUMN_REACH: f64 = 0.35;

/// The move that led to each board cropped from a drawn page: the last ply of the line the
/// text layer prints right above it, in its own column, when that line holds only numbered
/// moves (`11.a3 h4 12.Kh1`). Stored like the move read over a scan; the resolver checks that
/// the mover's piece stands on the destination. A line nearer another board's bottom than
/// this board's top abstains, as does a row that runs past the board's column or splits into
/// more than one run inside it.
fn rendered_board_led_moves(glyphs: &[Glyph], placed: &mut [super::picture::Placed]) {
    let boards: Vec<(f64, f64, f64, f64)> = placed
        .iter()
        .map(|p| (p.left, p.right, p.bottom, p.top))
        .collect();
    for (index, picture) in placed.iter_mut().enumerate() {
        if !picture.rendered || picture.scan_side.is_some() || picture.scan_move.is_some() {
            continue;
        }
        let (left, right, _, top) = boards[index];
        let width = right - left;
        if width <= 0.0 {
            continue;
        }
        // The board's own text column: within reach of its edges.
        let (from, to) = (left - COLUMN_REACH * width, right + COLUMN_REACH * width);
        let within = |g: &Glyph| (from..to).contains(&f64::midpoint(g.x, g.end));
        // The nearest baseline above the board in that column.
        let Some(baseline) = glyphs
            .iter()
            .filter(|g| {
                g.y > top
                    && g.y - top < LED_GAP_MAX * width
                    && within(g)
                    && !g.text.trim().is_empty()
            })
            .map(|g| g.y)
            .min_by(f64::total_cmp)
        else {
            continue;
        };
        let near_other = boards
            .iter()
            .enumerate()
            .any(|(other, &(l, r, bottom, _))| {
                other != index
                    && r > left
                    && l < right
                    && bottom > baseline
                    && bottom - baseline <= baseline - top
            });
        if near_other {
            continue;
        }
        let mut row: Vec<&Glyph> = glyphs
            .iter()
            .filter(|g| (g.y - baseline).abs() < 0.4 * g.size && !g.text.trim().is_empty())
            .collect();
        row.sort_by(|a, b| a.x.total_cmp(&b.x));
        // Split the row at wide gaps and keep the one run in the board's column.
        let mut runs: Vec<Vec<&Glyph>> = Vec::new();
        for glyph in row {
            match runs.last_mut() {
                Some(run)
                    if run
                        .last()
                        .is_some_and(|last| glyph.x - last.end <= COLUMN_GAP * width) =>
                {
                    run.push(glyph);
                }
                _ => runs.push(vec![glyph]),
            }
        }
        // The runs in the board's column; a justified line may leave wide spaces inside it.
        let over: Vec<&Vec<&Glyph>> = runs
            .iter()
            .filter(|run| run.iter().any(|g| within(g)))
            .collect();
        // A run reaching past the board's column is a line across the page, not its own.
        if over.is_empty()
            || over.iter().any(|run| {
                run.first().is_some_and(|first| first.x < from)
                    || run.last().is_some_and(|last| last.end > to)
            })
        {
            continue;
        }
        let column: Vec<&Glyph> = over.into_iter().flatten().copied().collect();
        if let Some((white, piece, square)) = led_move(&column_text(&column)) {
            // The OCR replay of a drawn page is weaker than its own text layer: one that stops
            // short of the board says nothing against the move printed over it.
            picture.scan_replay.clear();
            picture.scan_move = Some((
                white,
                format!("{}{piece}{square}", super::scan_caption::PLAYED_MOVE),
            ));
        }
    }
}

/// The text of one column of a row, with a space wherever the glyphs leave a gap.
fn column_text(column: &[&Glyph]) -> String {
    let mut text = String::new();
    let mut end: Option<f64> = None;
    for glyph in column {
        if end.is_some_and(|e| glyph.x - e > 0.15 * glyph.size) {
            text.push(' ');
        }
        text.push_str(&glyph.text);
        end = Some(glyph.end);
    }
    text
}

/// The last ply of a line of numbered moves set with figurines (`9.N f3 Q h5 10.N ×c3`): the
/// figurine and the capture sign are joined to their square first. A line may open with the
/// reply to a move on the line before; that reply is skipped. Any other word abstains.
fn led_move(text: &str) -> Option<(bool, char, String)> {
    let text = text.replace('\u{d7}', "x");
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let glued = words.last().is_some_and(|last| {
            let tail = last.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
            matches!(tail, "K" | "Q" | "R" | "B" | "N") || last.ends_with(['x', '-'])
        });
        match words.last_mut() {
            Some(last) if glued || word.starts_with('x') => last.push_str(word),
            _ => words.push(word.to_owned()),
        }
    }
    let first = words.iter().position(|w| {
        let digits = w.chars().take_while(char::is_ascii_digit).count();
        (1..=3).contains(&digits) && w[digits..].starts_with('.')
    })?;
    super::scan_caption::played_move(&words[first..].join(" "))
}

/// Re-encode diagram bytes as PNG so [`crate::books::diagram::with_side`] can store the turn.
pub(super) fn diagram_png(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(bytes.to_vec());
    }
    let image = image::load_from_memory(bytes).ok()?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// Unambiguous White/Black to move|play in `pdftotext` layout output.
fn pdftotext_turn(text: &str) -> Option<bool> {
    let text = text.to_ascii_lowercase();
    let white = text.matches("white to move").count() + text.matches("white to play").count();
    let black = text.matches("black to move").count() + text.matches("black to play").count();
    match (white > 0, black > 0) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// When pdf-extract left a page without usable text but the PDF still has a text layer
/// (common with CID fonts), read "White/Black to move|play" via `pdftotext` and stamp
/// [`crate::books::diagram::with_side`] on that page's pictures. Unambiguous pages only;
/// dual-side pages abstain; the cover image is left alone. Missing `pdftotext` is a no-op.
pub(super) fn stamp_pdftotext_sides(
    pdf: &std::path::Path,
    pages: &[Vec<Line>],
    pictures: &mut [Vec<super::picture::Placed>],
    cover: Option<&[u8]>,
) {
    for (i, lines) in pages.iter().enumerate() {
        let Some(placed) = pictures.get_mut(i) else {
            break;
        };
        if placed.is_empty() {
            continue;
        }
        let alnum = lines
            .iter()
            .flat_map(|l| l.text.chars())
            .filter(|c| c.is_alphanumeric())
            .count();
        // Pages that already carry real extract text keep the resolve-time caption path.
        if alnum >= 8 {
            continue;
        }
        let pdf_arg = match pdf.to_str() {
            Some(s) if !s.is_empty() => s,
            _ => return,
        };
        let Ok(output) = std::process::Command::new("pdftotext")
            .args([
                "-f",
                &(i + 1).to_string(),
                "-l",
                &(i + 1).to_string(),
                "-layout",
                pdf_arg,
                "-",
            ])
            .output()
        else {
            return; // binary missing or unrunnable: stop trying further pages
        };
        if !output.status.success() {
            continue;
        }
        let Some(side) = pdftotext_turn(&String::from_utf8_lossy(&output.stdout)) else {
            continue;
        };
        for pic in placed.iter_mut() {
            // Keep cover bytes unchanged so TextImporter can still skip the cover asset.
            if cover.is_some_and(|c| c == pic.bytes.as_slice()) {
                continue;
            }
            if crate::books::diagram::stated_side(&pic.bytes).is_some() {
                continue;
            }
            // Side is stored as a PNG tEXt chunk; JPEG (and other) embeds must be re-encoded.
            let Some(png) = diagram_png(&pic.bytes) else {
                continue;
            };
            pic.bytes = crate::books::diagram::with_side(&png, side);
        }
    }
}

/// Pictures per page. A page with no picture, or whose picture is the whole page (a scan), is
/// rendered and searched for boards; board crops found there replace the page scan. Each page
/// also says whether it embeds no picture at all, so its boards are drawn and its text layer
/// is the book's own text rather than an OCR layer over a scan.
fn pictures(
    doc: &lopdf::Document,
    bytes: &[u8],
    fonts: &[Vec<super::fontboard::FontBoard>],
) -> (Vec<Vec<super::picture::Placed>>, Vec<bool>) {
    let renderer = super::raster::Renderer::new(bytes);
    doc.get_pages()
        .values()
        .enumerate()
        .map(|(i, &page)| {
            let mut placed = super::picture::pictures(doc, page);
            let drawn = placed.is_empty();
            let Some((renderer, area)) = renderer.as_ref().and_then(|r| Some((r, r.area(i)?)))
            else {
                return (placed, drawn);
            };
            let scan = |p: &super::picture::Placed| p.area >= 0.5 * area;
            let fonts = fonts.get(i).map_or(&[][..], Vec::as_slice);
            let scanned = placed.iter().all(scan);
            if scanned || !fonts.is_empty() {
                let mut boards = renderer.boards(i, fonts);
                // Beside embedded pictures, only boards confirmed by their glyphs are added.
                if !scanned {
                    boards.retain(|b| crate::books::diagram::stated_placement(&b.bytes).is_some());
                }
                if !boards.is_empty() {
                    if scanned {
                        placed.clear();
                    }
                    placed.extend(boards);
                    placed.sort_by(|a, b| b.top.total_cmp(&a.top));
                }
            }
            (placed, drawn)
        })
        .unzip()
}
/// The largest image on page 1 when it is big enough to be a cover: JPEG streams as they
/// are, 8-bit gray or RGB streams re-encoded as PNG. Other encodings are skipped.
fn first_page_picture(doc: &lopdf::Document) -> Option<Vec<u8>> {
    let (_, page) = doc.get_pages().into_iter().next()?;
    let mut images = doc.get_page_images(page).ok()?;
    images.retain(|i| i.width >= 200 && i.height >= 200);
    images.sort_by_key(|i| std::cmp::Reverse(i.width * i.height));
    images.into_iter().find_map(|image| {
        let filters = image.filters.clone().unwrap_or_default();
        if filters.iter().any(|f| f == "DCTDecode") && filters.len() == 1 {
            return Some(image.content.to_vec());
        }
        if image.bits_per_component != Some(8) || !filters.iter().all(|f| f == "FlateDecode") {
            return None;
        }
        let pixels = doc
            .get_object(image.id)
            .ok()?
            .as_stream()
            .ok()?
            .decompressed_content()
            .ok()?;
        let (width, height) = (
            u32::try_from(image.width).ok()?,
            u32::try_from(image.height).ok()?,
        );
        let picture = match image.color_space.as_deref() {
            Some("DeviceGray") => {
                image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(width, height, pixels)?)
            }
            Some("DeviceRGB") => {
                image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(width, height, pixels)?)
            }
            _ => return None,
        };
        let mut png = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut png, image::ImageFormat::Png).ok()?;
        Some(png.into_inner())
    })
}
/// The glyph size most characters are set in, so one bold move does not make a row a title.
fn dominant(sizes: &[(f64, usize)]) -> f64 {
    sizes
        .iter()
        .max_by_key(|(_, chars)| *chars)
        .map_or(0.0, |(size, _)| *size)
}
/// Scanner and converter outlines often bookmark every page ("Image 0026", "Page 12");
/// those are not chapters.
fn chapters(outline: Vec<(u8, String, usize)>, pages: usize) -> Vec<(u8, String, usize)> {
    let pattern = |t: &str| {
        t.chars()
            .filter(|c| !c.is_ascii_digit())
            .collect::<String>()
    };
    let mut counts = std::collections::HashMap::new();
    for (_, title, _) in &outline {
        *counts.entry(pattern(title)).or_insert(0_usize) += 1;
    }
    // Scanner bookmarks repeat one placeholder ("Image 0001", "Page 2", "12"); numbered
    // chapters or games ("Game 1", "Chapter 8") are real contents.
    let placeholder = |p: &str| {
        let words = p
            .chars()
            .filter(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_ascii_lowercase();
        [
            "", "image", "img", "page", "pg", "p", "scan", "bookmark", "untitled",
        ]
        .contains(&words.as_str())
    };
    // A numbered series ("Game 1" ... "Game 90") may well have a bookmark on every page.
    let series = counts
        .iter()
        .filter(|(p, _)| !placeholder(p))
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(0);
    if outline.len() * 5 > pages * 4 && series * 2 < outline.len() {
        return Vec::new();
    }
    // Repeated placeholders are scanner stubs; real titles beside them stay.
    outline
        .into_iter()
        .filter(|(_, title, _)| {
            let p = pattern(title);
            !(placeholder(&p) && counts[&p] > 3)
        })
        .collect()
}
/// The x of a gutter between two prose columns: a gap wider than a word space that lines up
/// on at least a quarter of the lines, with sentence-length text on both sides (so tab-stop
/// tables such as "12. Nf3   Nc6" are not taken for columns).
fn gutter(lines: &[Vec<Glyph>]) -> Option<f64> {
    let mut gaps = Vec::new();
    for line in lines {
        let mut line: Vec<&Glyph> = line.iter().collect();
        line.sort_by(|a, b| a.x.total_cmp(&b.x));
        for pair in line.windows(2) {
            let gap = pair[1].x - pair[0].end;
            if gap > pair[1].size.max(6.0) * 0.9 {
                gaps.push(f64::midpoint(pair[0].end, pair[1].x));
            }
        }
    }
    let mut best: Option<(usize, f64)> = None;
    for &x in &gaps {
        let near: Vec<f64> = gaps
            .iter()
            .copied()
            .filter(|g| (g - x).abs() < 6.0)
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let mean = near.iter().sum::<f64>() / near.len() as f64;
        if best.is_none_or(|(n, _)| near.len() > n) {
            best = Some((near.len(), mean));
        }
    }
    let (count, x) = best?;
    if count < 3 || count * 4 < lines.len() {
        return None;
    }
    let mut sides = [Vec::new(), Vec::new()];
    for line in lines {
        if line.iter().any(|g| g.x < x && g.end > x) {
            continue;
        }
        for (side, chars) in sides.iter_mut().zip([
            line.iter()
                .filter(|g| g.x < x)
                .map(|g| g.text.chars().count())
                .sum::<usize>(),
            line.iter()
                .filter(|g| g.x >= x)
                .map(|g| g.text.chars().count())
                .sum(),
        ]) {
            if chars > 0 {
                side.push(chars);
            }
        }
    }
    let median = |v: &mut Vec<usize>| {
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };
    let [mut l, mut r] = sides;
    (median(&mut l) >= 15 && median(&mut r) >= 15).then_some(x)
}
/// Splits one baseline into cells at wide gaps, each with its dominant glyph size.
fn cells(mut line: Vec<Glyph>) -> Vec<Line> {
    line.sort_by(|a, b| a.x.total_cmp(&b.x));
    let y = line[0].y;
    // Parts split at wide gaps: (text, characters per glyph size, left, right).
    let mut parts = vec![(
        String::new(),
        Vec::<(f64, usize)>::new(),
        line[0].x,
        line[0].end,
    )];
    let mut end = None;
    for glyph in line {
        if let Some(last) = end {
            let gap = glyph.x - last;
            if gap > 48.0 {
                parts.push((String::new(), Vec::new(), glyph.x, glyph.end));
            } else if gap > glyph.size * 0.15 {
                parts.last_mut().expect("text part").0.push(' ');
            }
        }
        let part = parts.last_mut().expect("text part");
        part.0.push_str(&glyph.text);
        part.3 = glyph.end;
        let chars = glyph.text.trim().chars().count();
        if chars > 0 {
            if let Some(entry) = part
                .1
                .iter_mut()
                .find(|(size, _)| (size - glyph.size).abs() < 0.05)
            {
                entry.1 += chars;
            } else {
                part.1.push((glyph.size, chars));
            }
        }
        end = Some(glyph.end);
    }
    parts
        .into_iter()
        .map(|(s, sizes, x, end)| {
            (
                s.split_whitespace().collect::<Vec<_>>().join(" "),
                dominant(&sizes),
                x,
                end,
            )
        })
        .filter(|(s, _, _, _)| !s.is_empty())
        .map(|(text, size, x, end)| Line {
            x,
            end,
            text,
            size,
            y,
            cells: 1,
        })
        .collect()
}
/// Whether a line is one run of text across the gutter: a glyph covers it, or the text on
/// both sides is only a word space apart (an empty gutter is narrower than the column gap).
fn spans(line: &[Glyph], gutter: f64) -> bool {
    if line.iter().any(|g| g.x < gutter && g.end > gutter) {
        return true;
    }
    let before = line
        .iter()
        .filter(|g| g.x < gutter)
        .max_by(|a, b| a.end.total_cmp(&b.end));
    let after = line
        .iter()
        .filter(|g| g.x >= gutter)
        .min_by(|a, b| a.x.total_cmp(&b.x));
    match (before, after) {
        (Some(a), Some(b)) => b.x - a.end <= b.size.max(6.0) * 0.9,
        _ => false,
    }
}
fn order(mut glyphs: Vec<Glyph>) -> Vec<Line> {
    glyphs.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Vec<Glyph>> = Vec::new();
    for glyph in glyphs {
        if let Some(line) = lines
            .last_mut()
            .filter(|l| (l[0].y - glyph.y).abs() < glyph.size * 0.4)
        {
            line.push(glyph);
        } else {
            lines.push(vec![glyph]);
        }
    }
    if let Some(gutter) = gutter(&lines) {
        // Two text columns: read the left column down, then the right one. A line that
        // runs across the gutter (running head, wide title) ends the block above it.
        let mut out = Vec::new();
        let mut left = Vec::new();
        let mut right = Vec::new();
        for line in lines {
            if spans(&line, gutter) {
                out.append(&mut left);
                out.append(&mut right);
                out.extend(cells(line));
                continue;
            }
            let (l, r): (Vec<_>, Vec<_>) = line.into_iter().partition(|g| g.x < gutter);
            if !l.is_empty() {
                left.extend(cells(l));
            }
            if !r.is_empty() {
                right.extend(cells(r));
            }
        }
        out.append(&mut left);
        out.append(&mut right);
        return out;
    }
    let mut rows = Vec::new();
    for line in lines {
        let parts = cells(line);
        if !parts.is_empty() {
            rows.push(parts);
        }
    }
    let joined = |r: &[Line]| {
        r.iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let split = rows.iter().filter(|r| r.len() == 2).count();
    let table =
        rows.iter()
            .any(|r| joined(r).contains("White") && joined(r).contains("Black"))
            || rows.iter().filter(|r| r.len() == 2).all(|r| {
                r[1].text.len() < 12 && !r[1].text.starts_with(|c: char| c.is_ascii_digit())
            });
    let columns = split >= 2 && split * 2 >= rows.len() && !table;
    if columns {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for mut row in rows {
            if row.len() > 1 {
                right.extend(row.drain(1..));
            }
            left.extend(row.drain(..1));
        }
        left.extend(right);
        left
    } else {
        rows.into_iter()
            .map(|row| Line {
                text: joined(&row),
                size: dominant(
                    &row.iter()
                        .map(|l| (l.size, l.text.chars().count()))
                        .collect::<Vec<_>>(),
                ),
                y: row[0].y,
                x: row[0].x,
                end: row.iter().map(|l| l.end).fold(row[0].x, f64::max),
                cells: row.len(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_page_takes_its_lines_from_word_boxes() {
        use super::super::caption::{Rect, TextBox};
        let word = |text: &str, left: f64, bottom: f64| TextBox {
            text: text.into(),
            rect: Rect {
                left,
                bottom,
                right: left + 6.0 * text.len() as f64,
                top: bottom + 15.0,
            },
        };
        let first = [
            "19...gxf6",
            "20",
            "Ng4",
            "Kg7",
            "and",
            "now",
            "White",
            "wins",
        ];
        let mut row = Vec::new();
        let mut x = 50.0;
        for text in first {
            row.push(word(text, x, 700.0));
            x += 6.0 * text.len() as f64 + 4.0;
        }
        row.push(word("21", 50.0, 400.0));
        row.push(word("Nxf6", 66.0, 400.0));
        let mut pages = vec![Vec::new(), Vec::new()];
        let few = vec![word("12", 50.0, 700.0)];
        fill_from_words(&mut pages, &[row, few]);
        let texts: Vec<&str> = pages[0].iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            ["19...gxf6 20 Ng4 Kg7 and now White wins", "21 Nxf6"]
        );
        assert!(pages[0][0].y > pages[0][1].y);
        // Too few words (a page number): the page stays empty.
        assert!(pages[1].is_empty());
        // A page that already has text keeps it.
        let mut kept = vec![vec![Line {
            text: "Chapter 1".into(),
            size: 12.0,
            y: 700.0,
            x: 50.0,
            end: 120.0,
            cells: 1,
        }]];
        fill_from_words(&mut kept, &[vec![word("other", 50.0, 500.0); 9]]);
        assert_eq!(kept[0].len(), 1);
        assert_eq!(kept[0][0].text, "Chapter 1");
    }

    #[test]
    fn page_by_page_bookmarks_are_not_chapters() {
        let every_page: Vec<_> = (0..40).map(|i| (1, format!("Image {i:04}"), i)).collect();
        assert_eq!(chapters(every_page, 40), Vec::<(u8, String, usize)>::new());
        let real = vec![
            (1, "Introduction".to_string(), 2),
            (1, "Openings".to_string(), 10),
            (2, "Endings".to_string(), 30),
        ];
        assert_eq!(chapters(real.clone(), 40), real);
        let games: Vec<_> = (1..=8).map(|i| (1, format!("Game {i}"), i * 4)).collect();
        assert_eq!(chapters(games.clone(), 40), games);
        let dense_games: Vec<_> = (1..=40).map(|i| (1, format!("Game {i}"), i - 1)).collect();
        assert_eq!(chapters(dense_games.clone(), 40), dense_games);
        let mut half: Vec<_> = (1..=20).map(|i| (1, format!("Game {i}"), i - 1)).collect();
        half.extend((1..=20).map(|i| (1, format!("Scan note {i}"), i + 19)));
        assert_eq!(chapters(half.clone(), 40), half);
        let pages: Vec<_> = (1..=8).map(|i| (1, format!("Page {i}"), i * 4)).collect();
        assert_eq!(chapters(pages, 40), Vec::<(u8, String, usize)>::new());
        let mut mixed: Vec<_> = (1..=10).map(|i| (1, format!("Page {i}"), i * 3)).collect();
        mixed.extend(real.clone());
        assert_eq!(chapters(mixed, 40), real);
    }

    fn word(x: f64, y: f64, text: &str) -> Glyph {
        Glyph {
            x,
            y,
            end: x + 5.0 * text.len() as f64,
            size: 10.0,
            text: text.to_string(),
        }
    }

    #[test]
    fn a_title_across_an_empty_gutter_stays_whole_and_in_place() {
        let mut glyphs = vec![word(197.0, 800.0, "MODERN"), word(231.0, 800.0, "IDEAS")];
        for row in 0..6 {
            let y = 780.0 - f64::from(row) * 12.0;
            glyphs.push(word(
                50.0,
                y,
                &format!("left column sentence {row} goes on"),
            ));
            glyphs.push(word(
                260.0,
                y,
                &format!("right column sentence {row} goes on"),
            ));
        }
        glyphs.push(word(197.0, 690.0, "SECOND"));
        glyphs.push(word(231.0, 690.0, "PART"));
        let text: Vec<_> = order(glyphs).into_iter().map(|l| l.text).collect();
        assert_eq!(text[0], "MODERN IDEAS");
        assert_eq!(text[1], "left column sentence 0 goes on");
        assert_eq!(text[7], "right column sentence 0 goes on");
        assert_eq!(text[13], "SECOND PART");
    }

    #[test]
    fn a_large_first_page_picture_is_the_cover() {
        use lopdf::{Object, Stream, dictionary};
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(240, 360, image::Rgb([120, 30, 30]))
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = jpeg.into_inner();
        let mut doc = lopdf::Document::with_version("1.5");
        let pages = doc.new_object_id();
        let mut image = |width: i64, height: i64, data: Vec<u8>| {
            doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => width,
                    "Height" => height, "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                },
                data,
            ))
        };
        let cover = image(240, 360, jpeg.clone());
        let icon = image(20, 20, jpeg.clone());
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => icon, "Im1" => cover } },
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 450.into()],
        });
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        assert_eq!(first_page_picture(&doc), Some(jpeg));
    }

    #[test]
    fn dominant_size_ignores_a_few_bold_glyphs() {
        assert!((dominant(&[(14.0, 3), (10.0, 40)]) - 10.0).abs() < f64::EPSILON);
    }
}

#[cfg(test)]
mod stamp_tests {
    use super::pdftotext_turn;

    #[test]
    fn pdftotext_turn_reads_unambiguous_captions() {
        assert_eq!(pdftotext_turn("White to move\n\n28\n"), Some(true));
        assert_eq!(pdftotext_turn("Black to play\n197\n"), Some(false));
        assert_eq!(pdftotext_turn("just a number\n28\n"), None);
        assert_eq!(
            pdftotext_turn("White to move\n28\nBlack to move\n29\n"),
            None
        );
    }
}

#[cfg(test)]
mod led_tests {
    use super::*;

    #[test]
    fn a_figurine_move_line_names_the_move_that_led_to_the_board() {
        assert_eq!(
            led_move("10.B d3 g6 11.a3 h4 12.K h1"),
            Some((true, 'K', "h1".into()))
        );
        assert_eq!(
            led_move("9.N f3 Q h5 10.N c3 N g6 11.d4 B f4 12.N e2"),
            Some((true, 'N', "e2".into()))
        );
        // A line that opens with the reply to a move on the line before.
        assert_eq!(
            led_move("g6 12.Q h6 N \u{d7}f1 13.Q g7 K d7 14.Q \u{d7}f7+ N e7"),
            Some((false, 'N', "e7".into()))
        );
        assert_eq!(
            led_move("14.N \u{d7}e7+ N \u{d7}e7"),
            Some((false, 'N', "e7".into()))
        );
        // A caption, a reply with no numbered move, or prose names nothing.
        assert_eq!(led_move("No. 4589 Redlich \u{2013} Treumann"), None);
        assert_eq!(led_move("K d8"), None);
        assert_eq!(led_move("12.K h1 and Black resigned"), None);
    }

    fn board_picture(left: f64, bottom: f64, side: f64) -> super::super::picture::Placed {
        let mut png = Vec::new();
        image::GrayImage::new(1, 1)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        super::super::picture::Placed {
            left,
            bottom,
            right: left + side,
            top: bottom + side,
            area: side * side,
            bytes: crate::books::diagram::with_placement(&png, "4k3/8/8/8/8/8/8/4K3"),
            rendered: true,
            scan_side: None,
            scan_move: None,
            scan_replay: Vec::new(),
        }
    }

    fn glyphs(x: f64, y: f64, text: &str) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .filter(|(_, c)| *c != ' ')
            .map(|(i, c)| {
                let x = x + 5.0 * i as f64;
                Glyph {
                    x,
                    y,
                    end: x + 4.0,
                    size: 10.0,
                    text: c.to_string(),
                }
            })
            .collect()
    }

    #[test]
    fn a_font_board_takes_the_move_line_right_above_it_in_its_own_column() {
        // Two columns: each board sits under the end of its own game score.
        let mut page = glyphs(50.0, 520.0, "11.a3 h4 12.Kh1");
        page.extend(glyphs(300.0, 520.0, "18.Bxe6 Qxe6"));
        let mut placed = vec![
            board_picture(50.0, 400.0, 110.0),
            board_picture(300.0, 400.0, 110.0),
        ];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[0].scan_move, Some((true, "^Kh1".into())));
        assert_eq!(placed[1].scan_move, Some((false, "^Qe6".into())));
        // A move line closer to the bottom of the board above it is that board's.
        let page = glyphs(50.0, 560.0, "1.e4 e5 2.Nf3");
        let mut placed = vec![
            board_picture(50.0, 565.0, 110.0),
            board_picture(50.0, 440.0, 110.0),
        ];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[1].scan_move, None);
        // A short last line set left of the board is still its column's last line.
        let mut page = glyphs(20.0, 532.0, "8.Nxh8 Nd4");
        page.extend(glyphs(20.0, 520.0, "9.Qd1"));
        let mut placed = vec![board_picture(50.0, 400.0, 110.0)];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[0].scan_move, Some((true, "^Qd1".into())));
        // A line running across the page is not the board's own column.
        let page = glyphs(0.0, 520.0, "11.a3 h4 12.Kh1 Nf6 13.Kg1 Ne4 14.Kh1");
        let mut placed = vec![board_picture(50.0, 400.0, 110.0)];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[0].scan_move, None);
        // A justified line may leave a wide space inside the board's own column.
        let mut page = glyphs(50.0, 520.0, "Qd3 10.Re1 Bc5+");
        page.extend(glyphs(140.0, 520.0, "11.Kf1"));
        let mut placed = vec![board_picture(50.0, 400.0, 110.0)];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[0].scan_move, Some((true, "^Kf1".into())));
        // A line too far above the board is left alone.
        let page = glyphs(50.0, 600.0, "11.a3 h4 12.Kh1");
        let mut placed = vec![board_picture(50.0, 400.0, 110.0)];
        rendered_board_led_moves(&page, &mut placed);
        assert_eq!(placed[0].scan_move, None);
    }
}
