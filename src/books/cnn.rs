//! Small convolutional square classifier for printed and screen diagram images.
//!
//! Each of the 64 located squares is classified independently by three 3x3 convolution,
//! ReLU and 2x2 max-pool stages followed by two dense layers. Besides the twelve pieces and
//! the empty square there is a "junk" class, trained on squares cut by a misplaced frame, so
//! a board whose grid was found in the wrong place abstains instead of being misread.
//! Two networks of different widths were trained only on synthetic boards drawn with freely
//! licensed piece sets (see `assets/LICENSES.md` and `tools/diagram_cnn`); the weights are
//! embedded and inference runs on the CPU.
//!
//! A board is accepted only when both networks read the same label on every square, every
//! square is confident, kings and queens (whose colours
//! are the usual confusion in heavily inked prints) pass a stricter threshold, and the
//! material checks of [`diagram::placement_of`] hold. Otherwise the whole board abstains.

use super::diagram::{self, CELL, DiagramRecognizer, LABELS};

const MAGIC: &[u8; 4] = b"WPCN";
/// Classes: the 13 FEN labels followed by the junk class.
const CLASSES: usize = 14;
const JUNK: usize = 13;

/// Minimum probability for every empty, pawn and minor or rook square.
pub const MIN_PROBABILITY: f32 = 0.9;
/// Minimum probability for every king and queen square.
pub const MIN_ROYAL_PROBABILITY: f32 = 0.99;

static BUILTIN: std::sync::OnceLock<Option<Cnn>> = std::sync::OnceLock::new();

/// The networks bundled with the application.
pub fn builtin() -> Option<&'static Cnn> {
    BUILTIN
        .get_or_init(|| Cnn::decode(include_bytes!("diagram_cnn.bin")))
        .as_ref()
}

struct Conv {
    cin: usize,
    cout: usize,
    /// `[cout][cin][3][3]`
    weight: Vec<f32>,
    bias: Vec<f32>,
}

struct Dense {
    input: usize,
    output: usize,
    /// `[output][input]`
    weight: Vec<f32>,
    bias: Vec<f32>,
}

struct Network {
    convs: [Conv; 3],
    hidden: Dense,
    out: Dense,
}

/// Independently trained networks that must agree on every square.
pub struct Cnn {
    networks: Vec<Network>,
}

/// Per-square result: the most likely class and its probability.
#[derive(Clone, Copy, Debug)]
pub struct Square {
    /// FEN letter, `b'.'` for empty, or `b'?'` for junk or a disagreement.
    pub label: u8,
    pub probability: f32,
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn floats(&mut self, n: usize) -> Option<Vec<f32>> {
        let len = n.checked_mul(4)?;
        if self.bytes.len() < len {
            return None;
        }
        let (head, tail) = self.bytes.split_at(len);
        self.bytes = tail;
        Some(
            head.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
        )
    }
}

/// Scratch buffers reused across the squares of one board.
struct Scratch {
    padded: Vec<f32>,
    acc: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
}

impl Conv {
    /// 3x3 convolution with zero padding, ReLU and 2x2 max pooling of a `side` x `side` input.
    fn forward(&self, input: &[f32], side: usize, out: &mut Vec<f32>, s: &mut Scratch) {
        let p = side + 2;
        s.padded.clear();
        s.padded.resize(self.cin * p * p, 0.0);
        for c in 0..self.cin {
            for y in 0..side {
                let src = &input[(c * side + y) * side..][..side];
                s.padded[(c * p + y + 1) * p + 1..][..side].copy_from_slice(src);
            }
        }
        let half = side / 2;
        out.clear();
        out.resize(self.cout * half * half, 0.0);
        // Output rows are computed `p` wide over the flattened padded planes so every tap is
        // one long contiguous multiply-add; the last two columns of each row are discarded.
        let span = (side - 1) * p + side;
        s.acc.resize(span, 0.0);
        for o in 0..self.cout {
            s.acc.fill(self.bias[o]);
            for c in 0..self.cin {
                let plane = &s.padded[c * p * p..][..p * p];
                let kernel = &self.weight[(o * self.cin + c) * 9..][..9];
                for (ky, k) in kernel.chunks_exact(3).enumerate() {
                    let row = &plane[ky * p..][..span + 2];
                    let taps = row.iter().zip(&row[1..]).zip(&row[2..]);
                    for (a, ((v0, v1), v2)) in s.acc.iter_mut().zip(taps) {
                        *a += k[0] * v0 + k[1] * v1 + k[2] * v2;
                    }
                }
            }
            let dst = &mut out[o * half * half..][..half * half];
            for y in 0..half {
                for x in 0..half {
                    let at = |dy: usize, dx: usize| s.acc[(2 * y + dy) * p + 2 * x + dx];
                    dst[y * half + x] = at(0, 0).max(at(0, 1)).max(at(1, 0)).max(at(1, 1)).max(0.0);
                }
            }
        }
    }
}

impl Dense {
    fn forward(&self, input: &[f32], out: &mut Vec<f32>, relu: bool) {
        out.clear();
        out.extend(
            self.weight
                .chunks_exact(self.input)
                .zip(&self.bias)
                .map(|(w, b)| {
                    let v = b + w.iter().zip(input).map(|(w, x)| w * x).sum::<f32>();
                    if relu { v.max(0.0) } else { v }
                }),
        );
    }
}

/// Linear-interpolated percentile of `values` (reordered in place), as in `numpy.percentile`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn percentile(values: &mut [f32], q: f32) -> f32 {
    let rank = q / 100.0 * (values.len() - 1) as f32;
    let lo = rank.floor() as usize;
    let (_, v, rest) = values.select_nth_unstable_by(lo, f32::total_cmp);
    let v = *v;
    let next = rest.iter().copied().fold(f32::INFINITY, f32::min);
    if next.is_finite() {
        v + (next - v) * (rank - lo as f32)
    } else {
        v
    }
}

/// Quantises like the 8-bit training crops and stretches the board's 2nd..98th percentile
/// to [-1, 1], so print density and paper tone do not matter.
fn normalise(cells: &[Vec<f32>]) -> Vec<f32> {
    let quantise = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) / 255.0;
    let mut all: Vec<f32> = cells.iter().flatten().map(|v| quantise(*v)).collect();
    let mut sorted = all.clone();
    let lo = percentile(&mut sorted, 2.0);
    let hi = percentile(&mut sorted, 98.0);
    let range = (hi - lo).max(0.2);
    for v in &mut all {
        *v = ((*v - lo) / range).clamp(0.0, 1.0) * 2.0 - 1.0;
    }
    all
}

impl Network {
    /// Class count and three channel widths, then little-endian `f32` weights and biases
    /// layer by layer in `PyTorch` order.
    fn decode(r: &mut Reader) -> Option<Self> {
        let header = r.bytes.get(..4)?;
        if usize::from(header[0]) != CLASSES || header[1..].contains(&0) {
            return None;
        }
        let widths = [
            1,
            usize::from(header[1]),
            usize::from(header[2]),
            usize::from(header[3]),
        ];
        r.bytes = &r.bytes[4..];
        let mut conv = |cin: usize, cout: usize| -> Option<Conv> {
            Some(Conv {
                cin,
                cout,
                weight: r.floats(cout * cin * 9)?,
                bias: r.floats(cout)?,
            })
        };
        let convs = [
            conv(widths[0], widths[1])?,
            conv(widths[1], widths[2])?,
            conv(widths[2], widths[3])?,
        ];
        let flat = widths[3] * (CELL / 8) * (CELL / 8);
        let mut dense = |input: usize, output: usize| -> Option<Dense> {
            Some(Dense {
                input,
                output,
                weight: r.floats(output * input)?,
                bias: r.floats(output)?,
            })
        };
        let hidden = dense(flat, 64)?;
        let out = dense(64, CLASSES)?;
        Some(Self { convs, hidden, out })
    }

    /// Class logits of one normalised `CELL` x `CELL` square.
    fn logits(&self, cell: &[f32], s: &mut Scratch, logits: &mut Vec<f32>) {
        let mut a = std::mem::take(&mut s.a);
        let mut b = std::mem::take(&mut s.b);
        self.convs[0].forward(cell, CELL, &mut a, s);
        self.convs[1].forward(&a, CELL / 2, &mut b, s);
        self.convs[2].forward(&b, CELL / 4, &mut a, s);
        self.hidden.forward(&a, &mut b, true);
        self.out.forward(&b, logits, false);
        debug_assert_eq!(logits.len(), self.out.output);
        s.a = a;
        s.b = b;
    }

    /// Most likely class and its softmax probability for each normalised square.
    fn classify(&self, input: &[f32], s: &mut Scratch) -> Vec<Square> {
        let mut logits = Vec::with_capacity(CLASSES);
        input
            .chunks_exact(CELL * CELL)
            .map(|cell| {
                self.logits(cell, s, &mut logits);
                let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let exp: Vec<f32> = logits.iter().map(|z| (z - max).exp()).collect();
                let total: f32 = exp.iter().sum();
                let (best, p) = exp
                    .iter()
                    .enumerate()
                    .fold((0, 0.0), |a, (i, v)| if *v > a.1 { (i, *v) } else { a });
                Square {
                    label: if best == JUNK {
                        b'?'
                    } else {
                        LABELS.as_bytes()[best]
                    },
                    probability: p / total,
                }
            })
            .collect()
    }
}

impl Cnn {
    /// `WPCN`, version 2, network count, then each network as read by `Network::decode`.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..6)?;
        if &header[..4] != MAGIC || header[4] != 2 || header[5] == 0 {
            return None;
        }
        let mut r = Reader { bytes: &bytes[6..] };
        let networks = (0..header[5])
            .map(|_| Network::decode(&mut r))
            .collect::<Option<Vec<_>>>()?;
        r.bytes.is_empty().then_some(Self { networks })
    }

    /// Classifies the 64 squares of a located board, row 0 = rank 8. A square where the
    /// networks disagree becomes junk; otherwise it keeps the lowest probability.
    pub fn classify(&self, cells: &[Vec<f32>]) -> Vec<Square> {
        let input = normalise(cells);
        let mut s = Scratch {
            padded: Vec::new(),
            acc: Vec::new(),
            a: Vec::new(),
            b: Vec::new(),
        };
        let mut networks = self.networks.iter();
        let first = networks
            .next()
            .map(|n| n.classify(&input, &mut s))
            .unwrap_or_default();
        networks.fold(first, |mut squares, network| {
            for (square, other) in squares.iter_mut().zip(network.classify(&input, &mut s)) {
                if square.label == other.label {
                    square.probability = square.probability.min(other.probability);
                } else {
                    square.label = b'?';
                }
            }
            squares
        })
    }

    /// Classified squares of an encoded image, or `None` when no board is located.
    pub fn squares(&self, image: &[u8]) -> Option<Vec<Square>> {
        Some(self.classify(&diagram::board_cells(image)?))
    }
}

/// FEN placement when every square passes its confidence gate and the material checks hold.
pub fn placement(squares: &[Square]) -> Option<String> {
    if squares.len() != 64 {
        return None;
    }
    let confident = squares.iter().all(|s| {
        let gate = if matches!(s.label, b'K' | b'Q' | b'k' | b'q') {
            MIN_ROYAL_PROBABILITY
        } else {
            MIN_PROBABILITY
        };
        s.label != b'?' && s.probability >= gate
    });
    if !confident {
        return None;
    }
    let labels: Vec<u8> = squares.iter().map(|s| s.label).collect();
    diagram::placement_of(&labels)
}

impl DiagramRecognizer for Cnn {
    fn placement(&self, image: &[u8]) -> Option<String> {
        placement(&self.squares(image)?)
    }
    fn reading(&self, image: &[u8]) -> Option<String> {
        let squares = self.squares(image)?;
        if squares.iter().any(|s| s.label == b'?') {
            return None;
        }
        Some(squares.iter().map(|s| char::from(s.label)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_network_decodes() {
        let cnn = builtin().expect("bundled networks");
        assert_eq!(cnn.networks.len(), 2);
        assert!(cnn.networks.iter().all(|n| n.out.output == CLASSES));
        assert!(Cnn::decode(b"WPCN").is_none());
        let mut truncated = include_bytes!("diagram_cnn.bin").to_vec();
        truncated.pop();
        assert!(Cnn::decode(&truncated).is_none());
    }

    #[test]
    fn percentile_matches_linear_interpolation() {
        let mut v: Vec<f32> = (0..=100).rev().map(|v| v as f32).collect();
        assert!((percentile(&mut v, 2.0) - 2.0).abs() < 1e-6);
        let mut v = vec![0.0, 1.0];
        assert!((percentile(&mut v, 50.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn uncertain_or_junk_squares_abstain() {
        let mut squares = vec![
            Square {
                label: b'.',
                probability: 1.0
            };
            64
        ];
        squares[4].label = b'k';
        squares[60].label = b'K';
        assert_eq!(placement(&squares).as_deref(), Some("4k3/8/8/8/8/8/8/4K3"));
        squares[60].probability = 0.95;
        assert_eq!(
            placement(&squares),
            None,
            "royal squares need more confidence"
        );
        squares[60].probability = 1.0;
        squares[20].label = b'?';
        assert_eq!(placement(&squares), None, "a misplaced frame abstains");
        squares[20] = Square {
            label: b'N',
            probability: 0.5,
        };
        assert_eq!(placement(&squares), None);
        squares[20].probability = 0.95;
        assert!(placement(&squares).is_some());
        squares[0].label = b'P';
        assert_eq!(placement(&squares), None, "back-rank pawn");
    }
}
