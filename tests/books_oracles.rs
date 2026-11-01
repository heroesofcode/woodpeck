//! Verify independent source transcriptions without invoking the notation parser or importers.
use shakmaty::{CastlingMode, Chess, Position, fen::Fen, uci::UciMove};
use woodpeck::books::eval::Truth;

#[test]
fn source_uci_transcriptions_are_legal() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/books");
    let mut fragments = 0;
    for directory in ["fixtures", "gutenberg"] {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json")
                || path.file_name().is_some_and(|n| n == "metrics.json")
            {
                continue;
            }
            let truth: Truth = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            for fragment in truth.fragments {
                fragments += 1;
                let start: Chess = fragment
                    .start_fen
                    .parse::<Fen>()
                    .unwrap()
                    .into_position(CastlingMode::Standard)
                    .unwrap();
                let mut positions: Vec<Chess> = Vec::new();
                if let Some(parents) = &fragment.parents {
                    assert_eq!(parents.len(), fragment.moves.len());
                }
                for (index, uci) in fragment.moves.iter().enumerate() {
                    let parent = fragment
                        .parents
                        .as_ref()
                        .map_or_else(|| index.checked_sub(1), |p| p[index]);
                    let mut position =
                        parent.map_or_else(|| start.clone(), |i| positions[i].clone());
                    let m = uci
                        .parse::<UciMove>()
                        .unwrap()
                        .to_move(&position)
                        .unwrap_or_else(|e| panic!("{} {uci}: {e}", path.display()));
                    position.play_unchecked(m);
                    positions.push(position);
                }
            }
            for diagram in truth.diagrams {
                let _: Chess = diagram
                    .parse::<Fen>()
                    .unwrap()
                    .into_position(CastlingMode::Standard)
                    .unwrap();
            }
        }
    }
    assert!(fragments >= 55);
}
