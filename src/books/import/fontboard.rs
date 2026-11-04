//! Diagrams typeset with a chess diagram font (the widespread Marroquin layout used by Merida,
//! Alpha, Leipzig and similar fonts): each square is one glyph, so the position is read from
//! the text itself rather than from pixels. Light-square and dark-square glyphs differ, which
//! lets every square be checked against the board's colouring; any mismatch rejects the board.

/// One glyph: left edge and baseline in page space, size in points, and its character.
pub(super) struct Mark {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub ch: char,
}

/// A board read from glyphs: its extent in page space and FEN piece placement.
#[derive(Debug, PartialEq)]
pub(super) struct FontBoard {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub placement: String,
}

/// Font characters are often exposed in the symbol private-use range (U+F020 is the font's
/// space); fold them back to the font's ASCII codes.
fn code(ch: char) -> char {
    match u32::from(ch) {
        c @ 0xF020..=0xF0FF => char::from_u32(c - 0xF000).unwrap_or(ch),
        _ => ch,
    }
}

/// What a square glyph shows: the FEN letter (or `None` for empty) and whether it is drawn
/// on a dark square.
fn square(ch: char) -> Option<(Option<char>, bool)> {
    Some(match ch {
        ' ' | '*' => (None, false),
        '+' => (None, true),
        'p' | 'n' | 'b' | 'r' | 'q' | 'k' => (Some(ch.to_ascii_uppercase()), false),
        'P' | 'N' | 'B' | 'R' | 'Q' | 'K' => (Some(ch), true),
        'o' | 'm' | 'v' | 't' | 'w' | 'l' => (Some(black(ch)), false),
        'O' | 'M' | 'V' | 'T' | 'W' | 'L' => (Some(black(ch.to_ascii_lowercase())), true),
        _ => return None,
    })
}

fn black(ch: char) -> char {
    match ch {
        'o' => 'p',
        'm' => 'n',
        'v' => 'b',
        't' => 'r',
        'w' => 'q',
        _ => 'k',
    }
}

/// Left border glyphs: plain, or labelled with the rank (Merida-style fonts put ranks 1-8 at
/// 0xC0-0xC7).
fn left_border(ch: char) -> bool {
    ch == '$' || ch == '4' || ('\u{C0}'..='\u{C7}').contains(&ch)
}

fn right_border(ch: char) -> bool {
    ch == '%' || ch == '5'
}

/// One rank: left border x, cell width, baseline, size and eight squares (piece, dark).
struct Rank {
    left: f64,
    cell: f64,
    y: f64,
    size: f64,
    squares: [(Option<char>, bool); 8],
}

/// Ranks found on one text row: a left border, then squares up to a right border exactly
/// eight cells away. An empty light square may have no glyph at all.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ranks(row: &[(f64, char, f64, f64)]) -> Vec<Rank> {
    let mut out = Vec::new();
    for (i, &(x, ch, y, size)) in row.iter().enumerate() {
        if !left_border(ch) {
            continue;
        }
        let Some(&(rx, _, _, _)) = row[i + 1..]
            .iter()
            .take(17)
            .find(|(_, c, _, _)| right_border(*c))
        else {
            continue;
        };
        // Borders are one cell wide, like squares.
        let cell = (rx - x) / 9.0;
        if !(cell > 0.0 && (cell - size).abs() <= 0.25 * size) {
            continue;
        }
        let mut squares = [(None, false); 8];
        let mut filled = [false; 8];
        let mut ok = true;
        for &(gx, gc, _, _) in &row[i + 1..] {
            if gx >= rx - 0.3 * cell {
                break;
            }
            let k = ((gx - x) / cell - 1.0).round();
            if !(0.0..8.0).contains(&k) || ((gx - x) / cell - 1.0 - k).abs() > 0.25 {
                ok = false;
                break;
            }
            let k = k as usize;
            let Some(s) = square(gc) else {
                ok = false;
                break;
            };
            if filled[k] && squares[k] != s {
                ok = false;
                break;
            }
            squares[k] = s;
            filled[k] = true;
        }
        if !ok {
            continue;
        }
        // A square without a glyph reads as an empty light square; on a dark square the
        // colouring check rejects the board.
        if filled.iter().filter(|f| !**f).count() > 4 {
            continue;
        }
        out.push(Rank {
            left: x,
            cell,
            y,
            size,
            squares,
        });
    }
    out
}

/// Boards on a page, read from glyph rows; boards whose colouring is inconsistent are dropped.
pub(super) fn boards(marks: &[Mark]) -> Vec<FontBoard> {
    let mut glyphs: Vec<(f64, char, f64, f64)> = marks
        .iter()
        .map(|m| (m.x, code(m.ch), m.y, m.size))
        .collect();
    glyphs.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.total_cmp(&b.0)));
    // Rows of glyphs sharing a baseline.
    let mut rows: Vec<Vec<(f64, char, f64, f64)>> = Vec::new();
    for g in glyphs {
        match rows.last_mut() {
            Some(row) if (row[0].2 - g.2).abs() <= 0.2 * g.3.max(1.0) => row.push(g),
            _ => rows.push(vec![g]),
        }
    }
    let mut ranks: Vec<Rank> = Vec::new();
    for row in &mut rows {
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        ranks.extend(self::ranks(row));
    }
    let mut out = Vec::new();
    let mut used = vec![false; ranks.len()];
    for start in 0..ranks.len() {
        if used[start] {
            continue;
        }
        let first = &ranks[start];
        let mut chain = vec![start];
        for (j, r) in ranks.iter().enumerate().skip(start + 1) {
            let last = &ranks[*chain.last().expect("nonempty")];
            let step = last.y - r.y;
            if !used[j]
                && (r.left - first.left).abs() <= 0.2 * first.cell
                && (r.cell - first.cell).abs() <= 0.05 * first.cell
                && (step - first.cell).abs() <= 0.2 * first.cell
            {
                chain.push(j);
                if chain.len() == 8 {
                    break;
                }
            }
        }
        if chain.len() != 8 {
            continue;
        }
        if let Some(board) = board(&chain.iter().map(|&j| &ranks[j]).collect::<Vec<_>>()) {
            for j in chain {
                used[j] = true;
            }
            out.push(board);
        }
    }
    out
}

/// The board of eight ranks (top first), if every square's colour matches the checkering
/// (a8 light) and its glyph.
fn board(ranks: &[&Rank]) -> Option<FontBoard> {
    let mut placement = String::new();
    let mut glyph_squares = 0;
    for (r, rank) in ranks.iter().enumerate() {
        if r > 0 {
            placement.push('/');
        }
        let mut empty = 0u8;
        for (c, &(piece, dark)) in rank.squares.iter().enumerate() {
            if dark != ((r + c) % 2 == 1) {
                return None;
            }
            if dark || piece.is_some() {
                glyph_squares += 1;
            }
            match piece {
                Some(p) => {
                    if empty > 0 {
                        placement.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    placement.push(p);
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            placement.push(char::from(b'0' + empty));
        }
    }
    // Every dark square must have been drawn, so a board of light gaps cannot pass.
    let darks = ranks
        .iter()
        .enumerate()
        .flat_map(|(r, rank)| {
            rank.squares
                .iter()
                .enumerate()
                .filter(move |(c, _)| (r + c) % 2 == 1)
                .map(|(_, s)| s.1)
        })
        .filter(|d| *d)
        .count();
    if darks != 32 || glyph_squares < 32 {
        return None;
    }
    let kings = |k: char| placement.chars().filter(|c| *c == k).count() == 1;
    if !(kings('K') && kings('k')) {
        return None;
    }
    let (first, last) = (ranks[0], ranks[7]);
    Some(FontBoard {
        left: first.left,
        right: first.left + 10.0 * first.cell,
        top: first.y + first.size,
        bottom: last.y - 0.25 * last.size,
        placement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank_marks(y: f64, left: char, squares: &str) -> Vec<Mark> {
        let mut out = vec![Mark {
            x: 0.0,
            y,
            size: 10.0,
            ch: left,
        }];
        for (i, ch) in squares.chars().enumerate() {
            out.push(Mark {
                x: 10.0 * (i as f64 + 1.0),
                y,
                size: 10.0,
                ch,
            });
        }
        out.push(Mark {
            x: 90.0,
            y,
            size: 10.0,
            ch: '5',
        });
        out
    }

    fn page(ranks: &[&str]) -> Vec<Mark> {
        ranks
            .iter()
            .enumerate()
            .flat_map(|(i, r)| rank_marks(200.0 - 10.0 * i as f64, '4', r))
            .collect()
    }

    #[test]
    fn a_diagram_font_board_is_read_from_its_glyphs() {
        let marks = page(&[
            " +w+ Tl+", "+ + + +o", " + + + +", "+ + + + ", " + + + +", "+ + + + ", "p+ + + +",
            "+ + +k+ ",
        ]);
        let boards = boards(&marks);
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].placement, "2q2rk1/7p/8/8/8/8/P7/5K2");
    }

    #[test]
    fn private_use_codes_and_missing_light_squares_are_accepted() {
        let mut marks = page(&[
            " +w+ Tl+", "+ + + +o", " + + + +", "+ + + + ", " + + + +", "+ + + + ", "p+ + + +",
            "+ + +k+ ",
        ]);
        marks.retain(|m| m.ch != ' ');
        for m in &mut marks {
            m.ch = char::from_u32(0xF000 + u32::from(m.ch)).expect("char");
        }
        assert_eq!(boards(&marks)[0].placement, "2q2rk1/7p/8/8/8/8/P7/5K2");
    }

    #[test]
    fn a_square_against_the_colouring_rejects_the_board() {
        // The white pawn glyph for a dark square stands on a light square.
        let marks = page(&[
            "P+w+ Tl+", "+ + + +o", " + + + +", "+ + + + ", " + + + +", "+ + + + ", " + + + +",
            "+ + +k+ ",
        ]);
        assert_eq!(boards(&marks), Vec::new());
    }

    #[test]
    fn seven_ranks_are_not_a_board() {
        let marks = page(&[
            " +w+ Tl+", "+ + + +o", " + + + +", "+ + + + ", " + + + +", "+ + + + ", "p+ + + +",
        ]);
        assert_eq!(boards(&marks), Vec::new());
    }
}
