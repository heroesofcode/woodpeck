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
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub source: String,
    pub text: String,
    pub blocks: Vec<Block>,
}
impl Page {
    pub fn push(&mut self, text: &str, kind: BlockKind) {
        let start = self.text.len();
        self.text.push_str(text);
        self.blocks.push(Block {
            span: start..self.text.len(),
            kind,
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
}
