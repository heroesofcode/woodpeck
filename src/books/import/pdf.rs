//! Reconstruct lines from glyph coordinates rather than PDF painting order. A repeated central
//! gutter orders two text columns top-to-bottom; short White/Black move tables stay row-major.
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
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
    let pages: Vec<_> = layout.pages.into_iter().map(order).collect();
    let pictures = pictures(&doc, bytes, &fonts);
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
/// Pictures per page. A page with no picture, or whose picture is the whole page (a scan), is
/// rendered and searched for boards; board crops found there replace the page scan.
fn pictures(
    doc: &lopdf::Document,
    bytes: &[u8],
    fonts: &[Vec<super::fontboard::FontBoard>],
) -> Vec<Vec<super::picture::Placed>> {
    let renderer = super::raster::Renderer::new(bytes);
    doc.get_pages()
        .values()
        .enumerate()
        .map(|(i, &page)| {
            let mut placed = super::picture::pictures(doc, page);
            let Some((renderer, area)) = renderer.as_ref().and_then(|r| Some((r, r.area(i)?)))
            else {
                return placed;
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
            placed
        })
        .collect()
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
    // Parts split at wide gaps: (text, characters per glyph size).
    let mut parts = vec![(String::new(), Vec::<(f64, usize)>::new(), line[0].x)];
    let mut end = None;
    for glyph in line {
        if let Some(last) = end {
            let gap = glyph.x - last;
            if gap > 48.0 {
                parts.push((String::new(), Vec::new(), glyph.x));
            } else if gap > glyph.size * 0.15 {
                parts.last_mut().expect("text part").0.push(' ');
            }
        }
        let part = parts.last_mut().expect("text part");
        part.0.push_str(&glyph.text);
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
        .map(|(s, sizes, x)| {
            (
                s.split_whitespace().collect::<Vec<_>>().join(" "),
                dominant(&sizes),
                x,
            )
        })
        .filter(|(s, _, _)| !s.is_empty())
        .map(|(text, size, x)| Line {
            x,
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
                cells: row.len(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
