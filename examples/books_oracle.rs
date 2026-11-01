//! Independent fixture source: shakmaty SAN replay and seeded legal games, never books parsing.
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus};
use std::io::{self, Read};
fn fen(p: &Chess) -> String {
    Fen::from_position(p, EnPassantMode::Legal).to_string()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let sources: Vec<Value> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for source in sources {
        let mut pos: Chess = if let Some(s) = source["fen"].as_str() {
            s.parse::<Fen>()?.into_position(CastlingMode::Standard)?
        } else {
            Chess::default()
        };
        let mut seed = source["seed"].as_u64().unwrap_or(1);
        let tokens: Vec<&str> = source["pgn"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let count = source["plies"]
            .as_u64()
            .map_or(tokens.len(), |n| usize::try_from(n).unwrap());
        let mut moves = Vec::new();
        let mut sans = Vec::new();
        let mut fens = vec![fen(&pos)];
        let mut alternatives = Vec::new();
        for i in 0..count {
            let mut legal = pos.legal_moves().to_vec();
            legal.sort_by_key(|m| m.to_uci(CastlingMode::Standard).to_string());
            if legal.is_empty() {
                break;
            }
            let m = if tokens.is_empty() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                legal[usize::try_from(seed % u64::try_from(legal.len())?)?]
            } else {
                tokens[i].parse::<SanPlus>()?.san.to_move(&pos)?
            };
            if let Some(alt) = legal.iter().find(|candidate| **candidate != m) {
                let mut branch = pos.clone();
                let san = SanPlus::from_move(branch.clone(), *alt).to_string();
                branch.play_unchecked(*alt);
                alternatives.push(json!({"san":san,"uci":alt.to_uci(CastlingMode::Standard).to_string(),"fen":fen(&branch)}));
            } else {
                alternatives.push(Value::Null);
            }
            sans.push(SanPlus::from_move(pos.clone(), m).to_string());
            moves.push(m.to_uci(CastlingMode::Standard).to_string());
            pos.play_unchecked(m);
            fens.push(fen(&pos));
        }
        output.push(json!({"source": source,"san":sans,"uci":moves,"fens":fens,"alternatives":alternatives}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
