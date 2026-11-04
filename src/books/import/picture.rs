//! Pictures painted on a PDF page, with where they sit, so diagram images can join the text
//! flow. JBIG2 (common in scanned and print-ready books), JPEG and plain 8-bit or 1-bit
//! streams are read; other encodings are skipped.
use lopdf::{Document, Object, ObjectId, Stream};

/// A picture on a page: the height of its top edge in page space (larger is higher) and its
/// bytes as PNG or JPEG.
pub(super) struct Placed {
    pub top: f64,
    /// Area covered on the page, in square points.
    pub area: f64,
    pub bytes: Vec<u8>,
    /// Cropped from a rendered page rather than embedded as a picture.
    pub rendered: bool,
}
/// Smallest side, in pixels, of a picture worth keeping (icons and rules are skipped).
const MIN_SIDE: i64 = 120;
/// Largest side, in pixels, that is decoded.
const MAX_SIDE: i64 = 6000;

/// The pictures drawn by a page's content stream, top to bottom.
pub(super) fn pictures(doc: &Document, page: ObjectId) -> Vec<Placed> {
    let Ok(content) = doc.get_and_decode_page_content(page) else {
        return Vec::new();
    };
    let Ok(resources) = doc
        .get_dictionary(page)
        .and_then(|p| doc.get_dict_in_dict(p, b"Resources"))
    else {
        return Vec::new();
    };
    let Ok(xobjects) = doc.get_dict_in_dict(resources, b"XObject") else {
        return Vec::new();
    };
    let mut ctm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut saved = Vec::new();
    let mut out = Vec::new();
    for op in &content.operations {
        match op.operator.as_str() {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().unwrap_or(ctm),
            "cm" => {
                let m: Vec<f64> = op
                    .operands
                    .iter()
                    .filter_map(|o| o.as_float().ok().map(f64::from))
                    .collect();
                if let [a, b, c, d, e, f] = m[..] {
                    ctm = multiply([a, b, c, d, e, f], ctm);
                }
            }
            "Do" => {
                let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let Some(stream) = xobjects
                    .get(name)
                    .ok()
                    .and_then(|o| doc.dereference(o).ok())
                    .and_then(|(_, o)| o.as_stream().ok())
                else {
                    continue;
                };
                if let Some(bytes) = picture(doc, stream) {
                    // The unit square maps to the picture; its top is the highest corner.
                    let top = [0.0, 1.0]
                        .iter()
                        .flat_map(|&x| [0.0, 1.0].map(move |y: f64| (x, y)))
                        .map(|(x, y): (f64, f64)| ctm[1] * x + ctm[3] * y + ctm[5])
                        .fold(f64::MIN, f64::max);
                    let area = (ctm[0] * ctm[3] - ctm[1] * ctm[2]).abs();
                    out.push(Placed {
                        top,
                        area,
                        bytes,
                        rendered: false,
                    });
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| b.top.total_cmp(&a.top));
    out
}
/// `m × ctm`: the matrix of a `cm` operator applied on top of the current one.
fn multiply(m: [f64; 6], t: [f64; 6]) -> [f64; 6] {
    [
        m[0] * t[0] + m[1] * t[2],
        m[0] * t[1] + m[1] * t[3],
        m[2] * t[0] + m[3] * t[2],
        m[2] * t[1] + m[3] * t[3],
        m[4] * t[0] + m[5] * t[2] + t[4],
        m[4] * t[1] + m[5] * t[3] + t[5],
    ]
}
fn int(stream: &Stream, key: &[u8]) -> Option<i64> {
    stream.dict.get(key).ok()?.as_i64().ok()
}
fn filters(stream: &Stream) -> Vec<Vec<u8>> {
    match stream.dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(names)) => names
            .iter()
            .filter_map(|n| n.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    }
}
/// An image `XObject` as PNG or JPEG bytes, if it is big enough to be a figure and readable.
fn picture(doc: &Document, stream: &Stream) -> Option<Vec<u8>> {
    if stream.dict.get(b"Subtype").ok()?.as_name().ok()? != b"Image" {
        return None;
    }
    let (width, height) = (int(stream, b"Width")?, int(stream, b"Height")?);
    if width.min(height) < MIN_SIDE || width.max(height) > MAX_SIDE {
        return None;
    }
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    // A Decode array of [1 0] swaps black and white.
    let inverted = stream
        .dict
        .get(b"Decode")
        .and_then(Object::as_array)
        .ok()
        .and_then(|d| d.first())
        .and_then(|v| v.as_float().ok())
        .is_some_and(|v| v > 0.5);
    let filters = filters(stream);
    let image = match filters.last().map(Vec::as_slice) {
        Some(b"DCTDecode") if filters.len() == 1 => return Some(stream.content.clone()),
        Some(b"JBIG2Decode") => {
            let data = if filters.len() == 1 {
                stream.content.clone()
            } else {
                return None;
            };
            let globals = stream
                .dict
                .get(b"DecodeParms")
                .and_then(Object::as_dict)
                .and_then(|p| p.get(b"JBIG2Globals"))
                .ok()
                .and_then(|g| doc.dereference(g).ok())
                .and_then(|(_, g)| g.as_stream().ok())
                .and_then(|g| g.decompressed_content().ok());
            jbig2(&data, globals.as_deref(), w, h, inverted)?
        }
        None | Some(b"FlateDecode") => {
            let pixels = if filters.is_empty() {
                stream.content.clone()
            } else {
                stream.decompressed_content().ok()?
            };
            raw(doc, stream, &pixels, w, h, inverted)?
        }
        _ => return None,
    };
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}
/// Colour components of an image colour space: gray, RGB, or an ICC profile of either.
fn components(doc: &Document, space: &Object) -> Option<u32> {
    let space = doc.dereference(space).ok()?.1;
    match space {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            _ => None,
        },
        Object::Array(parts) => match parts.first().and_then(|p| p.as_name().ok())? {
            b"ICCBased" => {
                let profile = doc.dereference(parts.get(1)?).ok()?.1.as_stream().ok()?;
                match int(profile, b"N")? {
                    1 => Some(1),
                    3 => Some(3),
                    _ => None,
                }
            }
            b"CalGray" => Some(1),
            b"CalRGB" => Some(3),
            _ => None,
        },
        _ => None,
    }
}
/// Unfiltered samples: 8-bit gray or RGB, or 1-bit gray and stencil masks (0 is black).
fn raw(
    doc: &Document,
    stream: &Stream,
    pixels: &[u8],
    w: u32,
    h: u32,
    inverted: bool,
) -> Option<image::DynamicImage> {
    let mask = stream
        .dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    let space = if mask {
        Some(1)
    } else {
        components(doc, stream.dict.get(b"ColorSpace").ok()?)
    };
    let bits = if mask {
        1
    } else {
        int(stream, b"BitsPerComponent")?
    };
    let (w_, h_) = (w as usize, h as usize);
    match (bits, space?) {
        (8, 1) => {
            let mut gray = pixels.get(..w_ * h_)?.to_vec();
            if inverted {
                for v in &mut gray {
                    *v = 255 - *v;
                }
            }
            image::GrayImage::from_raw(w, h, gray).map(image::DynamicImage::ImageLuma8)
        }
        (8, 3) => image::RgbImage::from_raw(w, h, pixels.get(..w_ * h_ * 3)?.to_vec())
            .map(image::DynamicImage::ImageRgb8),
        (1, 1) => {
            // Rows are padded to whole bytes; a 0 sample is black, and a stencil mask paints it.
            let row = w_.div_ceil(8);
            if pixels.len() < row * h_ {
                return None;
            }
            Some(image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(
                w,
                h,
                |x, y| {
                    let byte = pixels[y as usize * row + x as usize / 8];
                    let black = ((byte >> (7 - x % 8)) & 1 == 0) != inverted;
                    image::Luma([if black { 0 } else { 255 }])
                },
            )))
        }
        _ => None,
    }
}
/// A JBIG2 stream (black is 1) as a gray image.
fn jbig2(
    data: &[u8],
    globals: Option<&[u8]>,
    w: u32,
    h: u32,
    inverted: bool,
) -> Option<image::DynamicImage> {
    struct Canvas {
        image: image::GrayImage,
        x: u32,
        y: u32,
        inverted: bool,
    }
    impl Canvas {
        fn put(&mut self, black: bool, count: u32) {
            // JBIG2 ink bits are the inverse of PDF DeviceGray samples. Convert that
            // polarity first, then apply the PDF Decode array as for other gray images.
            let value = if black == self.inverted { 255 } else { 0 };
            let (width, height) = self.image.dimensions();
            if self.y < height {
                for x in self.x..(self.x + count).min(width) {
                    self.image.put_pixel(x, self.y, image::Luma([value]));
                }
            }
            self.x += count;
        }
    }
    impl hayro_jbig2::Decoder for Canvas {
        fn push_pixel(&mut self, black: bool) {
            self.put(black, 1);
        }
        fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
            self.put(black, chunk_count * 8);
        }
        fn next_line(&mut self) {
            self.x = 0;
            self.y += 1;
        }
    }
    let image = hayro_jbig2::Image::new_embedded(data, globals).ok()?;
    if image.width() != w || image.height() > h.max(1) * 2 {
        return None;
    }
    let mut canvas = Canvas {
        image: image::GrayImage::from_pixel(w, image.height().min(h), image::Luma([255])),
        x: 0,
        y: 0,
        inverted,
    };
    image.decode(&mut canvas).ok()?;
    Some(image::DynamicImage::ImageLuma8(canvas.image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, dictionary};

    fn gray_image(side: i64, value: u8) -> Stream {
        Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => side,
                "Height" => side,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![value; usize::try_from(side * side).unwrap_or(0)],
        )
    }
    fn page_with(images: Vec<(&str, Stream)>, content: &str) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        let mut xobjects = Dictionary::new();
        for (name, image) in images {
            let id = doc.add_object(image);
            xobjects.set(name, id);
        }
        let content = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Contents" => content,
            "Resources" => dictionary! { "XObject" => xobjects },
        });
        (doc, page)
    }
    fn decoded(placed: &Placed) -> image::GrayImage {
        image::load_from_memory(&placed.bytes)
            .expect("png")
            .to_luma8()
    }
    #[test]
    fn pictures_come_top_down_with_their_placement() {
        let (doc, page) = page_with(
            vec![("Low", gray_image(150, 40)), ("High", gray_image(200, 200))],
            "q 150 0 0 150 50 100 cm /Low Do Q q 1 0 0 1 0 300 cm 200 0 0 200 50 100 cm /High Do Q",
        );
        let found = pictures(&doc, page);
        assert_eq!(found.len(), 2);
        assert!((found[0].top - 600.0).abs() < 1e-6, "{}", found[0].top);
        assert!((found[1].top - 250.0).abs() < 1e-6, "{}", found[1].top);
        assert_eq!(decoded(&found[0]).dimensions(), (200, 200));
        assert_eq!(decoded(&found[1]).get_pixel(3, 3).0, [40]);
    }
    #[test]
    fn small_pictures_and_forms_are_skipped() {
        let mut form = gray_image(200, 0);
        form.dict.set("Subtype", "Form");
        let (doc, page) = page_with(
            vec![("Icon", gray_image(40, 0)), ("Form", form)],
            "q 40 0 0 40 0 0 cm /Icon Do Q /Form Do",
        );
        assert!(pictures(&doc, page).is_empty());
    }
    #[test]
    fn one_bit_rows_are_padded_and_zero_is_black() {
        let side = 121u32;
        let row = side.div_ceil(8) as usize;
        let mut bits = vec![0xffu8; row * side as usize];
        bits[0] = 0x7f; // only the top-left pixel is black
        let mut image = gray_image(i64::from(side), 0);
        image.dict.set("BitsPerComponent", 1);
        image.content = bits;
        let (doc, page) = page_with(vec![("Bits", image)], "/Bits Do");
        let found = pictures(&doc, page);
        let png = decoded(&found[0]);
        assert_eq!(png.get_pixel(0, 0).0, [0]);
        assert_eq!(png.get_pixel(1, 0).0, [255]);
        assert_eq!(png.get_pixel(120, 120).0, [255]);
    }
    #[test]
    fn a_decode_array_inverts_gray_samples() {
        let mut image = gray_image(130, 10);
        image
            .dict
            .set("Decode", vec![Object::Integer(1), Object::Integer(0)]);
        let (doc, page) = page_with(vec![("Neg", image)], "/Neg Do");
        assert_eq!(decoded(&pictures(&doc, page)[0]).get_pixel(5, 5).0, [245]);
    }
    #[test]
    fn jbig2_ink_obeys_pdf_decode_polarity() {
        // Synthetic embedded JBIG2: page information with a uniform default pixel,
        // followed by end-of-page. No book or external image data is used.
        for black in [false, true] {
            let mut data = vec![0, 0, 0, 1, 48, 0, 1, 0, 0, 0, 19];
            for n in [128u32, 128, 0, 0] {
                data.extend_from_slice(&n.to_be_bytes());
            }
            data.extend_from_slice(&[u8::from(black) << 2, 0, 0]);
            data.extend_from_slice(&[0, 0, 0, 2, 49, 0, 1, 0, 0, 0, 0]);
            for inverted in [false, true] {
                let mut image = gray_image(128, 0);
                image.dict.set("Filter", "JBIG2Decode");
                image.dict.set("BitsPerComponent", 1);
                if inverted {
                    image.dict.set("Decode", vec![1.into(), 0.into()]);
                }
                image.content = data.clone();
                let (doc, page) = page_with(vec![("Board", image)], "/Board Do");
                let found = pictures(&doc, page);
                assert_eq!(found.len(), 1);
                let value = if black == inverted { 255 } else { 0 };
                assert!(decoded(&found[0]).pixels().all(|p| p.0 == [value]));
            }
        }
    }
    #[test]
    fn icc_gray_counts_as_gray() {
        let mut doc = Document::with_version("1.5");
        let profile = doc.add_object(Stream::new(dictionary! { "N" => 1 }, vec![]));
        let space = Object::Array(vec![Object::Name(b"ICCBased".to_vec()), profile.into()]);
        assert_eq!(components(&doc, &space), Some(1));
        assert_eq!(
            components(&doc, &Object::Name(b"DeviceCMYK".to_vec())),
            None
        );
    }
}
