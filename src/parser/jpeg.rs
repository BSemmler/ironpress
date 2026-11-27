use image::ImageDecoder;

#[allow(dead_code)]
pub(crate) struct DecodedJpegImage {
    pub width: u32,
    pub height: u32,
    pub rgb_data: Vec<u8>,
    pub icc_profile: Option<Vec<u8>>,
}

pub(crate) fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    parse_jpeg_frame(data).map(|frame| (frame.width, frame.height))
}

/// What a JPEG's header says about its frame, read without decoding it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JpegFrame {
    pub width: u32,
    pub height: u32,
    /// Colour components per sample: 1 (grey), 3 (YCbCr/RGB) or 4 (CMYK/YCCK)
    pub components: u8,
    /// An Adobe APP14 segment came before the frame. Adobe writes four-component
    /// JPEGs with inverted samples, which PDF readers undo with a `/Decode` array.
    pub adobe: bool,
}

/// How a viewer reads a JPEG embedded as-is with `/DCTDecode`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JpegPdfColor {
    pub color_space: &'static str,
    /// A `/Decode` array, for inverted Adobe CMYK
    pub decode: Option<&'static str>,
}

impl JpegPdfColor {
    const RGB: Self = Self {
        color_space: "/DeviceRGB",
        decode: None,
    };
}

/// The PDF colour space for embedding `data` unchanged: the colour space has to
/// match the JPEG's component count, or a viewer reads its samples misaligned
/// (a greyscale JPEG labelled DeviceRGB is drawn garbled, a third of its width).
pub(crate) fn jpeg_pdf_color(data: &[u8]) -> JpegPdfColor {
    match parse_jpeg_frame(data) {
        Some(JpegFrame { components: 1, .. }) => JpegPdfColor {
            color_space: "/DeviceGray",
            decode: None,
        },
        Some(JpegFrame {
            components: 4,
            adobe,
            ..
        }) => JpegPdfColor {
            color_space: "/DeviceCMYK",
            decode: adobe.then_some("[1 0 1 0 1 0 1 0]"),
        },
        _ => JpegPdfColor::RGB,
    }
}

pub(crate) fn parse_jpeg_frame(data: &[u8]) -> Option<JpegFrame> {
    if data.len() < 4 || data.first().copied() != Some(0xFF) || data.get(1).copied() != Some(0xD8) {
        return None;
    }

    let mut adobe = false;
    let mut pos = 2usize;
    while pos + 3 < data.len() {
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        let marker = *data.get(pos)?;
        pos += 1;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }

        let length = u16::from_be_bytes([*data.get(pos)?, *data.get(pos + 1)?]) as usize;
        if length < 2 || pos + length > data.len() {
            return None;
        }

        // APP14, identified by "Adobe"
        if marker == 0xEE && data.get(pos + 2..pos + 7) == Some(b"Adobe".as_slice()) {
            adobe = true;
        }

        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) {
            if length < 7 {
                return None;
            }
            let height = u16::from_be_bytes([*data.get(pos + 3)?, *data.get(pos + 4)?]) as u32;
            let width = u16::from_be_bytes([*data.get(pos + 5)?, *data.get(pos + 6)?]) as u32;
            if width == 0 || height == 0 {
                return None;
            }
            // A truncated frame header still has its size; its components default to 3
            let components = if length >= 8 { *data.get(pos + 7)? } else { 3 };
            return Some(JpegFrame {
                width,
                height,
                components,
                adobe,
            });
        }

        pos += length;
    }

    None
}

#[allow(dead_code)]
pub(crate) fn decode_jpeg_for_pdf(data: &[u8]) -> Option<DecodedJpegImage> {
    let cursor = std::io::Cursor::new(data);
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(cursor).ok()?;
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    let icc_profile = decoder.icc_profile().ok().flatten();
    let total_bytes = usize::try_from(decoder.total_bytes()).ok()?;
    let mut pixels = vec![0; total_bytes];
    decoder.read_image(&mut pixels).ok()?;

    let rgb_data = match color_type {
        image::ColorType::Rgb8 => pixels,
        image::ColorType::L8 => pixels
            .into_iter()
            .flat_map(|value| [value, value, value])
            .collect(),
        _ => image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
            .ok()?
            .to_rgb8()
            .into_raw(),
    };

    Some(DecodedJpegImage {
        width,
        height,
        rgb_data,
        icc_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn decode_jpeg_for_pdf_preserves_icc_profile() {
        let pixels = [255u8, 128, 0];
        let mut encoded = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut encoded);
        let icc_profile = vec![1, 2, 3, 4];
        encoder
            .set_icc_profile(icc_profile.clone())
            .expect("jpeg encoder should accept ICC profile");
        encoder
            .write_image(&pixels, 1, 1, image::ExtendedColorType::Rgb8)
            .expect("jpeg encoding should succeed");

        let decoded = decode_jpeg_for_pdf(&encoded).expect("jpeg should decode");
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.icc_profile.as_deref(), Some(icc_profile.as_slice()));
        assert_eq!(decoded.rgb_data.len(), 3);
    }

    #[test]
    fn parse_jpeg_dimensions_valid() {
        // Encode a small RGB JPEG and confirm parse_jpeg_dimensions returns the right size.
        let pixels = [128u8, 64, 32, 200, 100, 50, 10, 20, 30, 40, 80, 160];
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut encoded)
            .write_image(&pixels, 2, 2, image::ExtendedColorType::Rgb8)
            .expect("jpeg encoding should succeed");
        let dims = parse_jpeg_dimensions(&encoded);
        assert_eq!(dims, Some((2, 2)));
    }

    #[test]
    fn parse_jpeg_dimensions_rejects_empty() {
        assert_eq!(parse_jpeg_dimensions(&[]), None);
    }

    #[test]
    fn parse_jpeg_dimensions_rejects_wrong_magic() {
        // Valid length but wrong SOI marker bytes.
        assert_eq!(parse_jpeg_dimensions(&[0x00, 0xD8, 0xFF, 0xE0]), None);
    }

    #[test]
    fn parse_jpeg_dimensions_rejects_too_short() {
        // Only the SOI bytes — not enough for any marker segment.
        assert_eq!(parse_jpeg_dimensions(&[0xFF, 0xD8, 0xFF]), None);
    }

    #[test]
    fn parse_jpeg_dimensions_stops_at_eoi_marker() {
        // A JPEG that contains an EOI (0xD9) marker before any SOF — should return None.
        let data: &[u8] = &[
            0xFF, 0xD8, // SOI
            0xFF, 0xD9, // EOI — parser must stop here
            0xFF, 0xC0, // SOF0 — should never be reached
            0x00, 0x0B, // length = 11
            0x08, // precision
            0x00, 0x01, // height = 1
            0x00, 0x01, // width  = 1
            0x01, // components
        ];
        assert_eq!(parse_jpeg_dimensions(data), None);
    }

    #[test]
    fn parse_jpeg_dimensions_rejects_invalid_segment_length() {
        // Segment length of 1 is invalid (must be >= 2).
        let data: &[u8] = &[
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, // APP0 marker
            0x00, 0x01, // length = 1 — invalid
        ];
        assert_eq!(parse_jpeg_dimensions(data), None);
    }

    #[test]
    fn parse_jpeg_dimensions_rejects_zero_width() {
        // SOF0 segment with zero width — must be rejected.
        let data: &[u8] = &[
            0xFF, 0xD8, // SOI
            0xFF, 0xC0, // SOF0
            0x00, 0x0B, // length = 11
            0x08, // precision
            0x00, 0x01, // height = 1
            0x00, 0x00, // width  = 0  ← invalid
            0x01, 0x01, 0x11, 0x00, // component data
        ];
        assert_eq!(parse_jpeg_dimensions(data), None);
    }

    #[test]
    fn decode_jpeg_grayscale_expands_to_rgb() {
        // Encode a 1-pixel grayscale JPEG and verify decode_jpeg_for_pdf expands it to 3 bytes.
        let pixels = [200u8];
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut encoded)
            .write_image(&pixels, 1, 1, image::ExtendedColorType::L8)
            .expect("jpeg encoding should succeed");

        let decoded = decode_jpeg_for_pdf(&encoded).expect("grayscale jpeg should decode");
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        // Grayscale expanded to RGB: all three channels equal the original luma value.
        assert_eq!(decoded.rgb_data.len(), 3);
        assert!(
            decoded
                .rgb_data
                .iter()
                .all(|&b| (b as i32 - 200i32).abs() <= 5),
            "grayscale pixel should expand to near-equal R/G/B values, got {:?}",
            decoded.rgb_data
        );
    }

    /// SOI, an optional Adobe APP14 segment, then a SOF0 frame header with
    /// `components` components
    fn frame_header(components: u8, adobe: bool) -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8];
        if adobe {
            data.extend_from_slice(&[
                0xFF, 0xEE, 0x00, 0x0E, b'A', b'd', b'o', b'b', b'e', 0x00, 0x64, 0x00, 0x00, 0x00,
                0x00, 0x02,
            ]);
        }
        let length = 8 + 3 * u16::from(components);
        data.extend_from_slice(&[0xFF, 0xC0]);
        data.extend_from_slice(&length.to_be_bytes());
        data.extend_from_slice(&[0x08, 0x00, 0x02, 0x00, 0x03, components]);
        for id in 1..=components {
            data.extend_from_slice(&[id, 0x11, 0x00]);
        }
        data
    }

    #[test]
    fn parse_jpeg_frame_reads_components_and_adobe_segment() {
        assert_eq!(
            parse_jpeg_frame(&frame_header(1, false)),
            Some(JpegFrame {
                width: 3,
                height: 2,
                components: 1,
                adobe: false
            })
        );
        let cmyk = parse_jpeg_frame(&frame_header(4, true)).unwrap();
        assert_eq!((cmyk.components, cmyk.adobe), (4, true));
    }

    #[test]
    fn jpeg_pdf_color_matches_the_component_count() {
        assert_eq!(
            jpeg_pdf_color(&frame_header(1, false)).color_space,
            "/DeviceGray"
        );
        assert_eq!(jpeg_pdf_color(&frame_header(3, false)), JpegPdfColor::RGB);
        assert_eq!(
            jpeg_pdf_color(&frame_header(4, false)),
            JpegPdfColor {
                color_space: "/DeviceCMYK",
                decode: None
            }
        );
        assert_eq!(
            jpeg_pdf_color(&frame_header(4, true)),
            JpegPdfColor {
                color_space: "/DeviceCMYK",
                decode: Some("[1 0 1 0 1 0 1 0]")
            }
        );
        // An Adobe segment only inverts CMYK
        assert_eq!(jpeg_pdf_color(&frame_header(3, true)), JpegPdfColor::RGB);
        // What can't be parsed keeps the old behaviour
        assert_eq!(jpeg_pdf_color(b"not a jpeg"), JpegPdfColor::RGB);
    }

    #[test]
    fn jpeg_pdf_color_of_an_encoded_grayscale_jpeg() {
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut encoded)
            .write_image(&[200u8], 1, 1, image::ExtendedColorType::L8)
            .expect("jpeg encoding should succeed");
        assert_eq!(jpeg_pdf_color(&encoded).color_space, "/DeviceGray");
    }

    #[test]
    fn decode_jpeg_for_pdf_rejects_non_jpeg() {
        // Random bytes are not a valid JPEG — decode should return None.
        let data = b"not a jpeg at all";
        assert!(decode_jpeg_for_pdf(data).is_none());
    }
}
