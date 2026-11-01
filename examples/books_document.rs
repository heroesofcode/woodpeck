//! Inspect imported text for authoring independent evaluation annotations.
use woodpeck::books::import::{PageSource, TextImporter};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("Expected book path")?;
    let mut doc = TextImporter.document(std::path::Path::new(&path))?;
    doc.assets.clear();
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}
