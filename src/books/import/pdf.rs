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
pub(super) fn extract(bytes: &[u8]) -> Result<Vec<String>, OutputError> {
    let doc = lopdf::Document::load_mem(bytes)?;
    let mut layout = Layout::default();
    pdf_extract::output_doc(&doc, &mut layout)?;
    Ok(layout.pages.into_iter().map(order).collect())
}
fn order(mut glyphs: Vec<Glyph>) -> String {
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
    let mut rows = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut parts = vec![String::new()];
        let mut end = None;
        for glyph in line {
            if let Some(last) = end {
                let gap = glyph.x - last;
                if gap > 48.0 {
                    parts.push(String::new());
                } else if gap > glyph.size * 0.15 {
                    parts.last_mut().expect("text part").push(' ');
                }
            }
            parts.last_mut().expect("text part").push_str(&glyph.text);
            end = Some(glyph.end);
        }
        let parts: Vec<_> = parts
            .into_iter()
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
            .collect();
        if !parts.is_empty() {
            rows.push(parts);
        }
    }
    let split = rows.iter().filter(|r| r.len() == 2).count();
    let table = rows
        .iter()
        .any(|r| r.join(" ").contains("White") && r.join(" ").contains("Black"))
        || rows
            .iter()
            .filter(|r| r.len() == 2)
            .all(|r| r[1].len() < 12 && !r[1].starts_with(|c: char| c.is_ascii_digit()));
    let columns = split >= 2 && split * 2 >= rows.len() && !table;
    let mut text = Vec::new();
    if columns {
        for row in &rows {
            text.push(row[0].clone());
        }
        for row in &rows {
            if let Some(right) = row.get(1) {
                text.push(right.clone());
            }
        }
    } else {
        for row in rows {
            text.push(row.join(" "));
        }
    }
    text.join("\n")
}
