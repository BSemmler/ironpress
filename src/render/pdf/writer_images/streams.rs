use super::encoded::EncodedImage;
use super::*;

impl PdfWriter {
    /// Add an image as a PDF XObject and return its object ID.
    pub(in crate::render::pdf) fn add_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
    ) -> usize {
        self.add_image_object_with_interpolation(
            data,
            width,
            height,
            format,
            png_metadata,
            PdfImageInterpolation::Default,
        )
    }

    /// Embed an image with the requested final PDF sampling behavior.
    pub(in crate::render::pdf) fn add_image_object_with_interpolation(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        let encoded = ImageEncoding::for_render(&self.opts).encode(
            data,
            width,
            height,
            format,
            png_metadata,
            interpolation,
        );
        self.add_encoded_image(encoded, interpolation)
    }

    pub(in crate::render::pdf) fn add_raw_rgba_image_object(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Option<usize> {
        let pixels = usize::try_from(width.checked_mul(height)?).ok()?;
        if rgba.len() != pixels.checked_mul(4)? {
            return None;
        }
        let has_alpha = rgba.chunks_exact(4).any(|pixel| pixel[3] != 255);
        let mut rgb = Vec::with_capacity(pixels.checked_mul(3)?);
        let mut alpha = has_alpha.then(|| Vec::with_capacity(pixels));
        for pixel in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
            if let Some(alpha) = &mut alpha {
                alpha.push(pixel[3]);
            }
        }

        let color = ImageStream::flate(width, height, "/DeviceRGB", flate_compress(&rgb)?);
        let alpha = match alpha {
            Some(alpha) => Some(ImageStream::flate(
                width,
                height,
                "/DeviceGray",
                flate_compress(&alpha)?,
            )),
            None => None,
        };
        Some(self.add_encoded_image(
            EncodedImage::with_alpha(color, alpha),
            PdfImageInterpolation::Default,
        ))
    }

    pub(in crate::render::pdf) fn add_flate_image_stream(
        &mut self,
        stream: Vec<u8>,
        width: u32,
        height: u32,
        color_space: &'static str,
        alpha_id: Option<usize>,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        self.add_image_stream(
            ImageStream::flate(width, height, color_space, stream),
            alpha_id,
            interpolation,
        )
    }

    pub(in crate::render::pdf) fn add_dct_image_stream(
        &mut self,
        stream: Vec<u8>,
        width: u32,
        height: u32,
        color_space: &'static str,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        self.add_image_stream(
            ImageStream::dct(width, height, color_space, stream),
            None,
            interpolation,
        )
    }

    pub(crate) fn add_raw_png_image_object(&mut self, raw_png: &[u8]) -> Option<usize> {
        let encoded = ImageEncoding::for_render(&self.opts)
            .encode_png_pixels(raw_png, PdfImageInterpolation::Default)?;
        Some(self.add_encoded_image(encoded, PdfImageInterpolation::Default))
    }
}
