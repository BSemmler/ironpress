//! The conversion options that decide how a raster is encoded for PDF.

use super::encoded::{EncodedImage, ImageStream};
use super::*;

/// Resolution that source images shown below their native resolution are
/// resampled to.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SourceImageDensity {
    dpi_bits: u32,
}

impl SourceImageDensity {
    fn new(dpi: f32) -> Self {
        Self {
            dpi_bits: dpi.to_bits(),
        }
    }

    pub(super) fn scale(self) -> crate::render::raster_scale::RasterScale {
        crate::render::raster_scale::RasterScale::at_dpi(f32::from_bits(self.dpi_bits))
    }
}

/// How rasters become PDF image streams.
///
/// Two conversions that agree on this encode the same image identically,
/// which is what lets an [`ImageCache`] share encodings between them.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::render::pdf) struct ImageEncoding {
    pub(super) jpeg_quality: u8,
    /// `None` embeds source images at their native resolution.
    pub(super) downsample_to: Option<SourceImageDensity>,
}

impl ImageEncoding {
    pub(in crate::render::pdf) fn for_render(opts: &RenderOpts) -> Self {
        Self {
            jpeg_quality: opts.jpeg_quality,
            downsample_to: opts
                .auto_resize_images
                .then(|| SourceImageDensity::new(opts.raster_quality.source_image_dpi)),
        }
    }

    /// Encode image bytes for PDF at their own pixel dimensions.
    ///
    /// A large opaque PNG becomes a JPEG when that is smaller, unless the
    /// image must keep its source edges. Translucent PNGs are split into
    /// colour and soft-mask streams; other PNGs embed their IDAT unchanged.
    pub(in crate::render::pdf) fn encode(
        self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        interpolation: PdfImageInterpolation,
    ) -> EncodedImage {
        if matches!(format, ImageFormat::Png | ImageFormat::PngAlpha)
            && !interpolation.preserves_source_edges()
            && should_try_lossy_png_reencode(width, height, data.len())
            && let Some(decoded) = try_decode_png_as_opaque_rgb(data)
            && let Some(jpeg) = encode_rgb_as_jpeg(
                &decoded.color_data,
                decoded.width,
                decoded.height,
                self.jpeg_quality,
            )
            && jpeg.len() < data.len()
        {
            return EncodedImage::opaque(ImageStream::jpeg(decoded.width, decoded.height, jpeg));
        }
        if format == ImageFormat::PngAlpha
            && let Some(image) = self.encode_png_pixels(data, interpolation)
        {
            return image;
        }
        match format {
            ImageFormat::Jpeg => {
                EncodedImage::opaque(ImageStream::jpeg(width, height, data.to_vec()))
            }
            ImageFormat::Png | ImageFormat::PngAlpha => {
                let parsed = crate::parser::png::parse_png(data);
                let recovered = parsed.as_ref().map(|info| PngMetadata {
                    channels: info.channels,
                    bit_depth: info.bit_depth,
                });
                let Some(layout) = png_metadata.or(recovered.as_ref()) else {
                    return EncodedImage::opaque(ImageStream::blank());
                };
                let idat = parsed.map_or_else(|| data.to_vec(), |info| info.idat_data);
                EncodedImage::opaque(ImageStream::png_scanlines(width, height, layout, idat))
            }
        }
    }

    /// Encode a PNG from its decoded pixels, moving any alpha into a soft
    /// mask. `None` when the PNG cannot be decoded.
    pub(in crate::render::pdf) fn encode_png_pixels(
        self,
        raw_png: &[u8],
        interpolation: PdfImageInterpolation,
    ) -> Option<EncodedImage> {
        let decoded = decode_png_for_pdf(raw_png)?;
        let (width, height) = (decoded.width, decoded.height);
        let alpha = match decoded.alpha_data.as_deref() {
            Some(alpha) => Some(ImageStream::flate(
                width,
                height,
                "/DeviceGray",
                flate_compress(alpha)?,
            )),
            None => None,
        };
        let jpeg_color = (alpha.is_none()
            && decoded.color_space == "/DeviceRGB"
            && !interpolation.preserves_source_edges()
            && should_try_lossy_png_reencode(width, height, raw_png.len()))
        .then(|| encode_rgb_as_jpeg(&decoded.color_data, width, height, self.jpeg_quality))
        .flatten();
        if let Some(jpeg) = jpeg_color {
            return Some(EncodedImage::opaque(ImageStream::dct(
                width,
                height,
                "/DeviceRGB",
                jpeg,
            )));
        }
        let color = ImageStream::flate(
            width,
            height,
            decoded.color_space,
            flate_compress(&decoded.color_data)?,
        );
        Some(EncodedImage::with_alpha(color, alpha))
    }
}
