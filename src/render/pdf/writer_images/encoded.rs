//! PDF image streams encoded independently of the document they land in.
//!
//! Encoding a raster (decoding, resampling, recompressing) is the expensive
//! part of embedding it. Keeping the result free of object numbers lets one
//! encoding be written into any number of documents.

use super::*;

/// The `/Filter` an image stream's samples are encoded with.
#[derive(Clone, Copy)]
enum ImageFilter {
    /// Uncompressed samples.
    None,
    /// JPEG, with the `/Decode` array that inverted Adobe CMYK needs.
    Dct { decode: Option<&'static str> },
    /// zlib-compressed samples.
    Flate,
    /// zlib-compressed PNG scanlines, each led by its PNG filter-type byte.
    PngScanlines { colors: u8 },
}

/// One image XObject: how its samples are laid out, and the encoded samples.
#[derive(Clone)]
pub(in crate::render::pdf) struct ImageStream {
    size: RasterDimensions,
    color_space: &'static str,
    bits_per_component: u8,
    filter: ImageFilter,
    data: Vec<u8>,
}

impl ImageStream {
    pub(in crate::render::pdf) fn flate(
        width: u32,
        height: u32,
        color_space: &'static str,
        data: Vec<u8>,
    ) -> Self {
        Self {
            size: RasterDimensions { width, height },
            color_space,
            bits_per_component: 8,
            filter: ImageFilter::Flate,
            data,
        }
    }

    /// A JPEG whose components are known to match `color_space`.
    pub(in crate::render::pdf) fn dct(
        width: u32,
        height: u32,
        color_space: &'static str,
        data: Vec<u8>,
    ) -> Self {
        Self {
            size: RasterDimensions { width, height },
            color_space,
            bits_per_component: 8,
            filter: ImageFilter::Dct { decode: None },
            data,
        }
    }

    /// A JPEG embedded unchanged, labelled with the colour space of its
    /// components.
    pub(super) fn jpeg(width: u32, height: u32, data: Vec<u8>) -> Self {
        let color = crate::parser::jpeg::jpeg_pdf_color(&data);
        Self {
            size: RasterDimensions { width, height },
            color_space: color.color_space,
            bits_per_component: 8,
            filter: ImageFilter::Dct {
                decode: color.decode,
            },
            data,
        }
    }

    /// PNG IDAT data embedded undecoded; the PNG predictor undoes its
    /// scanline filters.
    pub(super) fn png_scanlines(
        width: u32,
        height: u32,
        layout: &PngMetadata,
        idat: Vec<u8>,
    ) -> Self {
        Self {
            size: RasterDimensions { width, height },
            color_space: match layout.channels {
                1 | 2 => "/DeviceGray",
                _ => "/DeviceRGB",
            },
            bits_per_component: layout.bit_depth,
            filter: ImageFilter::PngScanlines {
                colors: layout.channels,
            },
            data: idat,
        }
    }

    /// A single white pixel, standing in for an image that cannot be read.
    pub(super) fn blank() -> Self {
        Self {
            size: RasterDimensions {
                width: 1,
                height: 1,
            },
            color_space: "/DeviceGray",
            bits_per_component: 8,
            filter: ImageFilter::None,
            data: vec![255],
        }
    }

    fn header(
        &self,
        id: usize,
        soft_mask: Option<usize>,
        interpolation: PdfImageInterpolation,
    ) -> String {
        let RasterDimensions { width, height } = self.size;
        let bpc = self.bits_per_component;
        let (decode, filter) = match self.filter {
            ImageFilter::None => (None, String::new()),
            ImageFilter::Dct { decode } => (decode, " /Filter /DCTDecode".to_string()),
            ImageFilter::Flate => (None, " /Filter /FlateDecode".to_string()),
            ImageFilter::PngScanlines { colors } => (
                None,
                format!(
                    " /Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns {width} /Colors {colors} /BitsPerComponent {bpc} >>"
                ),
            ),
        };
        let decode = decode.map(|d| format!(" /Decode {d}")).unwrap_or_default();
        let soft_mask = soft_mask.map_or_else(String::new, |id| format!(" /SMask {id} 0 R"));
        format!(
            "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {color_space}{decode} /BitsPerComponent {bpc}{filter}{soft_mask}{interpolation} /Length {len} >>\nstream\n",
            color_space = self.color_space,
            interpolation = interpolation.pdf_dictionary_entry(),
            len = self.data.len(),
        )
    }
}

/// A raster encoded for PDF: its colour samples and, when the source is
/// translucent, the soft mask that carries its alpha.
#[derive(Clone)]
pub(in crate::render::pdf) struct EncodedImage {
    color: ImageStream,
    alpha: Option<ImageStream>,
}

impl EncodedImage {
    pub(super) fn opaque(color: ImageStream) -> Self {
        Self { color, alpha: None }
    }

    pub(super) fn with_alpha(color: ImageStream, alpha: Option<ImageStream>) -> Self {
        Self { color, alpha }
    }

    /// Bytes of encoded sample data this image writes into a PDF.
    pub(super) fn byte_len(&self) -> usize {
        self.color.data.len() + self.alpha.as_ref().map_or(0, |alpha| alpha.data.len())
    }
}

impl PdfWriter {
    /// Write `image` as an image XObject and return its object ID.
    pub(in crate::render::pdf) fn add_encoded_image(
        &mut self,
        image: EncodedImage,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        let soft_mask = image
            .alpha
            .map(|alpha| self.add_image_stream(alpha, None, PdfImageInterpolation::Default));
        self.add_image_stream(image.color, soft_mask, interpolation)
    }

    pub(in crate::render::pdf) fn add_image_stream(
        &mut self,
        stream: ImageStream,
        soft_mask: Option<usize>,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        let id = self.next_id();
        self.objects
            .push(stream.header(id, soft_mask, interpolation));
        self.binary_objects.insert(id, stream.data);
        id
    }
}
