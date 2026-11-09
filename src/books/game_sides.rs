//! Side to move from real games.
//!
//! `game_sides.bin` lists positions that occurred in at least `K` games of public game
//! databases. It is built by `tools/game_sides` from game records alone, never from a book. A
//! placement seen with only one side to move has one entry; one seen with both has two. A board
//! found with exactly one side gets that side; anything else abstains.
//!
//! Format: `WPGS`, version byte `1`, the minimum game count `K` (u16 LE), the entry count (u32 LE),
//! then sorted 6-byte big-endian entries. An entry is `(fnv1a64(placement) >> 17) << 1 | black`.

/// Fewer men than this and the same placement turns up with either side to move.
pub const MIN_MEN: usize = 13;

const MAGIC: &[u8; 4] = b"WPGS";
const HEADER: usize = 11;
const ENTRY: usize = 6;

#[cfg(not(target_arch = "wasm32"))]
static TABLE: &[u8] = include_bytes!("game_sides.bin");
#[cfg(target_arch = "wasm32")]
static TABLE: &[u8] = &[];

/// FNV-1a over the FEN placement field.
fn fnv1a64(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn entries(table: &[u8]) -> Option<&[u8]> {
    if table.len() < HEADER || &table[..4] != MAGIC || table[4] != 1 {
        return None;
    }
    let count = u32::from_le_bytes(table[7..11].try_into().ok()?) as usize;
    let body = &table[HEADER..];
    (body.len() == count.checked_mul(ENTRY)?).then_some(body)
}

fn contains(body: &[u8], key: u64) -> bool {
    let wanted = key.to_be_bytes();
    let wanted = &wanted[2..];
    let (mut low, mut high) = (0, body.len() / ENTRY);
    while low < high {
        let middle = low + (high - low) / 2;
        let entry = &body[middle * ENTRY..(middle + 1) * ENTRY];
        match entry.cmp(wanted) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

/// `Some(true)` when only White-to-move games reached `placement`, `Some(false)` for Black,
/// `None` when the table is silent, ambiguous, or the board has too few men.
pub fn side(placement: &str) -> Option<bool> {
    side_in(TABLE, placement)
}

/// `true` when `placement` occurred in real games with either side to move: an independent
/// check that a recognized board is a real position.
pub fn seen(placement: &str) -> bool {
    lookup(TABLE, placement).is_some_and(|(white, black)| white || black)
}

fn lookup(table: &[u8], placement: &str) -> Option<(bool, bool)> {
    let men = placement.chars().filter(char::is_ascii_alphabetic).count();
    if men < MIN_MEN {
        return None;
    }
    let body = entries(table)?;
    let base = (fnv1a64(placement) >> 17) << 1;
    Some((contains(body, base), contains(body, base | 1)))
}

fn side_in(table: &[u8], placement: &str) -> Option<bool> {
    match lookup(table, placement)? {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADER, MAGIC, fnv1a64, lookup, side_in};

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";
    const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR";

    fn table(entries: &[(&str, bool)]) -> Vec<u8> {
        let mut keys: Vec<u64> = entries
            .iter()
            .map(|(placement, black)| ((fnv1a64(placement) >> 17) << 1) | u64::from(*black))
            .collect();
        keys.sort_unstable();
        let mut out = MAGIC.to_vec();
        out.push(1);
        out.extend_from_slice(&5_u16.to_le_bytes());
        out.extend_from_slice(&u32::try_from(keys.len()).unwrap().to_le_bytes());
        for key in keys {
            out.extend_from_slice(&key.to_be_bytes()[2..]);
        }
        assert_eq!(out.len(), HEADER + 6 * entries.len());
        out
    }

    #[test]
    fn fnv_matches_the_generator() {
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn a_position_seen_with_one_side_names_it() {
        let t = table(&[(AFTER_E4, true), (START, false)]);
        assert_eq!(side_in(&t, AFTER_E4), Some(false));
        assert_eq!(side_in(&t, START), Some(true));
    }

    #[test]
    fn both_sides_or_absent_abstain() {
        let t = table(&[(AFTER_E4, false), (AFTER_E4, true)]);
        assert_eq!(side_in(&t, AFTER_E4), None);
        assert_eq!(side_in(&t, START), None);
        assert_eq!(lookup(&t, AFTER_E4), Some((true, true)));
        assert_eq!(lookup(&t, START), Some((false, false)));
    }

    #[test]
    fn few_men_or_a_broken_table_abstain() {
        let few = "8/8/4k3/8/8/4K3/4P3/8";
        let t = table(&[(few, false)]);
        assert_eq!(side_in(&t, few), None);
        assert_eq!(side_in(&[], START), None);
        assert_eq!(side_in(b"WPGS\x01\x05\x00\x09\x00\x00\x00", START), None);
    }
}
