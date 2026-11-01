//! Importer seam: future OCR implements `PageSource` and returns the same text/block document.
use super::document::{Asset, BlockKind, Document, Page};
use scraper::{Html, Selector};
use std::{
    collections::HashMap,
    io::Read,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;
mod pdf;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("This PDF has no text layer (scanned). Scanned books are not supported yet.")]
    NoTextLayer,
    #[error("Unsupported book format; expected EPUB or PDF.")]
    Unsupported,
    #[error("Invalid book: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub trait PageSource {
    fn document(&self, path: &Path) -> Result<Document, ImportError>;
}
pub struct TextImporter;
impl PageSource for TextImporter {
    fn document(&self, path: &Path) -> Result<Document, ImportError> {
        let bytes = std::fs::read(path)?;
        if bytes.starts_with(b"%PDF-") {
            let texts = pdf::extract(&bytes).map_err(|e| ImportError::Invalid(e.to_string()))?;
            if texts.is_empty()
                || texts
                    .iter()
                    .filter(|t| t.chars().filter(|c| c.is_alphanumeric()).count() < 8)
                    .count()
                    * 2
                    > texts.len()
            {
                return Err(ImportError::NoTextLayer);
            }
            let pages = texts
                .into_iter()
                .enumerate()
                .map(|(i, text)| {
                    let mut page = Page {
                        source: format!("page:{}", i + 1),
                        ..Page::default()
                    };
                    let lines: Vec<_> = text.lines().filter(|s| !s.trim().is_empty()).collect();
                    let mut i = 0;
                    while i < lines.len() {
                        if let Some(group) = lines.get(i..i + 9)
                            && let Some(pos) = super::resolve::diagram(&group.join("\n"))
                        {
                            page.push(
                                &group.join("\n"),
                                BlockKind::Diagram {
                                    fen: super::resolve::fen(&pos),
                                },
                            );
                            i += 9;
                        } else {
                            page.push(lines[i], BlockKind::Paragraph);
                            i += 1;
                        }
                    }
                    page
                })
                .collect();
            return Ok(Document {
                title: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                author: String::new(),
                format: "pdf".into(),
                pages,
                assets: vec![],
            });
        }
        if !bytes.starts_with(b"PK") {
            return Err(ImportError::Unsupported);
        }
        epub(&bytes)
    }
}
pub fn load(path: &Path) -> Result<super::model::Book, ImportError> {
    Ok(super::resolve::resolve(TextImporter.document(path)?))
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("constant CSS selector")
}
fn joined(base: &Path, href: &str) -> String {
    let mut result = PathBuf::new();
    for component in base
        .join(href.split('#').next().unwrap_or(href))
        .components()
    {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(v) => result.push(v),
            _ => {}
        }
    }
    result.to_string_lossy().replace('\\', "/")
}
#[allow(clippy::too_many_lines)] // Keep spine traversal and pagination in source order.
fn epub(bytes: &[u8]) -> Result<Document, ImportError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ImportError::Invalid(e.to_string()))?;
    let mut files = HashMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ImportError::Invalid(e.to_string()))?;
        // Bound decompressed input, including images, before allocating model storage.
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        total += data.len();
        if data.len() > 32 * 1024 * 1024 || total > 128 * 1024 * 1024 {
            return Err(ImportError::Invalid("EPUB exceeds import limits".into()));
        }
        files.insert(
            entry
                .name()
                .map_err(|e| ImportError::Invalid(e.to_string()))?
                .into_owned(),
            data,
        );
    }
    let html = |name: &str| -> Result<Html, ImportError> {
        Ok(Html::parse_document(&String::from_utf8_lossy(
            files
                .get(name)
                .ok_or_else(|| ImportError::Invalid(format!("Missing {name}")))?,
        )))
    };
    let container = html("META-INF/container.xml")?;
    let opf_path = container
        .select(&selector("rootfile"))
        .next()
        .and_then(|e| e.attr("full-path"))
        .ok_or_else(|| ImportError::Invalid("Missing OPF rootfile".into()))?;
    let base = Path::new(opf_path).parent().unwrap_or(Path::new(""));
    let opf = html(opf_path)?;
    let metadata = |name: &str| {
        opf.select(&selector(name))
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default()
    };
    let manifest: HashMap<_, _> = opf
        .select(&selector("item"))
        .filter_map(|e| Some((e.attr("id")?.to_owned(), e.attr("href")?.to_owned())))
        .collect();
    let mut doc = Document {
        title: metadata("dc\\:title"),
        author: metadata("dc\\:creator"),
        format: "epub".into(),
        pages: vec![],
        assets: vec![],
    };
    for item in opf.select(&selector("itemref")) {
        let href = item
            .attr("idref")
            .and_then(|id| manifest.get(id))
            .ok_or_else(|| ImportError::Invalid("Invalid spine reference".into()))?;
        let source = joined(base, href);
        let content = html(&source)?;
        let mut page = Page {
            source: source.clone(),
            ..Page::default()
        };
        for el in content.select(&selector("h1,h2,h3,h4,h5,h6,p,table,pre,img")) {
            if el.value().name() != "img"
                && el
                    .ancestors()
                    .skip(1)
                    .filter_map(scraper::ElementRef::wrap)
                    .any(|p| matches!(p.value().name(), "table" | "p" | "pre"))
            {
                continue;
            }
            let name = el.value().name();
            let text = if name == "pre" {
                el.text().collect::<String>()
            } else {
                el.text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            if !page.blocks.is_empty()
                && (page.text.len() + text.len() > 6000 || name == "h1" || name == "h2")
            {
                doc.pages.push(page);
                page = Page {
                    source: source.clone(),
                    ..Page::default()
                };
            }
            let kind = match name {
                "img" => {
                    let path = joined(
                        Path::new(&source).parent().unwrap_or(base),
                        el.attr("src").unwrap_or(""),
                    );
                    if let Some(bytes) = files.get(&path)
                        && !doc.assets.iter().any(|a| a.path == path)
                    {
                        doc.assets.push(Asset {
                            path: path.clone(),
                            bytes: bytes.clone(),
                        });
                    }
                    BlockKind::Image {
                        path,
                        alt: el.attr("alt").unwrap_or("").into(),
                    }
                }
                "table" => BlockKind::Table,
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => BlockKind::Heading,
                _ => BlockKind::Paragraph,
            };
            page.push(&text, kind);
        }
        if !page.blocks.is_empty() {
            doc.pages.push(page);
        }
    }
    Ok(doc)
}
