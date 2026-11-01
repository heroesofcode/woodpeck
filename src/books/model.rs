use super::document::Document;
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub type NodeId = usize;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub san: String,
    pub uci: String,
    pub fen: String,
    pub page: usize,
    pub span: Range<usize>,
    pub nags: Vec<String>,
    pub comment: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Resolved,
    Partial(String),
    Unresolved(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fragment {
    pub page: usize,
    pub span: Range<usize>,
    pub start_fen: String,
    pub nodes: Vec<NodeId>,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub page: usize,
    pub fragments: Vec<usize>,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub version: u32,
    pub document: Document,
    pub language: String,
    pub nodes: Vec<Node>,
    pub fragments: Vec<Fragment>,
    pub sections: Vec<Section>,
}

/// A reader can render these runs directly and make `node_id` runs clickable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun<'a> {
    pub text: &'a str,
    pub span: Range<usize>,
    pub node_id: Option<NodeId>,
}
impl Book {
    pub fn text_runs(&self, page_index: usize) -> Option<Vec<TextRun<'_>>> {
        let page = self.document.pages.get(page_index)?;
        let mut moves: Vec<_> = self.nodes.iter().filter(|n| n.page == page_index).collect();
        moves.sort_by_key(|n| n.span.start);
        let mut runs = Vec::new();
        let mut start = 0;
        for node in moves {
            if node.span.start < start || node.span.end > page.text.len() {
                return None;
            }
            if start < node.span.start {
                runs.push(TextRun {
                    text: page.text.get(start..node.span.start)?,
                    span: start..node.span.start,
                    node_id: None,
                });
            }
            runs.push(TextRun {
                text: page.text.get(node.span.clone())?,
                span: node.span.clone(),
                node_id: Some(node.id),
            });
            start = node.span.end;
        }
        if start < page.text.len() {
            runs.push(TextRun {
                text: page.text.get(start..)?,
                span: start..page.text.len(),
                node_id: None,
            });
        }
        Some(runs)
    }
}
