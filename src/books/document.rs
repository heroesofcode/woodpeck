use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockKind {
    Heading,
    Paragraph,
    Table,
    Image {
        path: String,
        alt: String,
    },
    Diagram {
        fen: String,
    },
    /// A diagram image whose position was read from its pixels and checked against the text.
    RecognizedDiagram {
        path: String,
        alt: String,
        fen: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    /// UTF-8 byte offsets into the containing page, never character indices.
    pub span: Range<usize>,
    pub kind: BlockKind,
    /// Heading level 1–6 for `Heading` blocks; 0 elsewhere and in books imported before levels.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level: u8,
}
#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes fields by reference.
fn is_zero(level: &u8) -> bool {
    *level == 0
}
impl Block {
    /// Heading level for display: unknown levels read as section headings (2).
    pub fn heading_level(&self) -> u8 {
        if self.level == 0 {
            2
        } else {
            self.level.min(6)
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub source: String,
    /// PDF paragraphs already reconstructed with geometric evidence. Older imports default
    /// to false so the reader can repair physical line blocks without changing source spans.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paragraphs: bool,
    pub text: String,
    pub blocks: Vec<Block>,
}
impl Page {
    pub fn push(&mut self, text: &str, kind: BlockKind) {
        self.push_level(text, kind, 0);
    }
    pub fn heading(&mut self, text: &str, level: u8) {
        self.push_level(text, BlockKind::Heading, level.clamp(1, 6));
    }
    fn push_level(&mut self, text: &str, kind: BlockKind, level: u8) {
        let start = self.text.len();
        self.text.push_str(text);
        self.blocks.push(Block {
            span: start..self.text.len(),
            kind,
            level,
        });
        self.text.push('\n');
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub path: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub author: String,
    pub format: String,
    pub pages: Vec<Page>,
    pub assets: Vec<Asset>,
    /// Path of the asset holding the book's own cover picture, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// ISBN from the book's metadata, used only to look up a cover when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<String>,
}
