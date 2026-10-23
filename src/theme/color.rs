use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);
impl Rgb {
    pub const BLACK: Self = Self(0, 0, 0);
    pub const WHITE: Self = Self(255, 255, 255);
    pub fn parse(hex: &str) -> Option<Self> {
        let s = hex.strip_prefix('#')?;
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        Some(Self(
            u8::from_str_radix(&s[..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..], 16).ok()?,
        ))
    }
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
    #[must_use]
    pub fn mix(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let channel =
            |a: u8, b: u8| (f64::from(a) * (1.0 - amount) + f64::from(b) * amount).round() as u8;
        Self(
            channel(self.0, other.0),
            channel(self.1, other.1),
            channel(self.2, other.2),
        )
    }
    pub fn is_light(self) -> bool {
        u16::from(self.0) + u16::from(self.1) + u16::from(self.2) > 382
    }
}
/// CSS OKLCH (L in 0..1, hue in degrees) converted through linear sRGB and clamped.
pub fn oklch_to_srgb(lightness: f64, chroma: f64, hue: f64) -> Rgb {
    let radians = hue.to_radians();
    let a = chroma * radians.cos();
    let b = chroma * radians.sin();
    let ll = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let mm = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let ss = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    let r = 4.076_741_662_1 * ll - 3.307_711_591_3 * mm + 0.230_969_929_2 * ss;
    let g = -1.268_438_004_6 * ll + 2.609_757_401_1 * mm - 0.341_319_396_5 * ss;
    let b = -0.004_196_086_3 * ll - 0.703_418_614_7 * mm + 1.707_614_701 * ss;
    Rgb(encode(r), encode(g), encode(b))
}
fn encode(v: f64) -> u8 {
    let v = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_and_mix_rounding() {
        assert_eq!(Rgb::parse("#aAbBcC"), Some(Rgb(170, 187, 204)));
        for s in ["#fff", "123456", "#gg0000", "#ééé"] {
            assert_eq!(Rgb::parse(s), None);
        }
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 0.5), Rgb(128, 128, 128));
        assert_eq!(Rgb::WHITE.mix(Rgb::BLACK, 0.24), Rgb(194, 194, 194));
    }
    #[test]
    fn known_oklch_vectors() {
        assert_eq!(oklch_to_srgb(0.0, 0.0, 0.0), Rgb::BLACK);
        assert_eq!(oklch_to_srgb(1.0, 0.0, 0.0), Rgb::WHITE);
        assert_eq!(
            oklch_to_srgb(0.627_955, 0.257_683, 29.233_9),
            Rgb(255, 0, 0)
        );
        assert_eq!(oklch_to_srgb(0.5, 0.0, 0.0), Rgb(99, 99, 99));
        assert_eq!(oklch_to_srgb(0.60, 0.10, 145.0), Rgb(88, 144, 90));
    }
}
