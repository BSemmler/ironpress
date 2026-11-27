//! Encoding document images for the size they are displayed at.

use super::encoded::EncodedImage;
use super::encoding::ImageEncoding;
use super::scale::CssRasterScale;
use super::*;

impl ImageEncoding {
    /// Encode a layout image for its display size and CSS `image-rendering`.
    ///
    /// Document source images participate in the configurable source-image
    /// downscaling policy. Renderer-owned rasters keep their native dimensions:
    /// their pixels were deliberately generated at a filter or background DPI.
    pub(super) fn encode_layout_image(
        self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> EncodedImage {
        let interpolation = PdfImageInterpolation::for_css_image_rendering(image_rendering);
        let native = || {
            self.encode(
                &image.data,
                image.source_width,
                image.source_height,
                image.format,
                image.png_metadata.as_ref(),
                interpolation,
            )
        };
        if image.origin.preserves_native_resolution() {
            return native();
        }
        if image_rendering.is_pixelated() {
            return self
                .pixelated(image, display_w_pt, display_h_pt)
                .unwrap_or_else(native);
        }
        match self.downsample(image, display_w_pt, display_h_pt) {
            Some(resized) => self.encode(
                &resized.data,
                resized.width,
                resized.height,
                resized.format,
                resized.png_metadata.as_ref(),
                interpolation,
            ),
            None => native(),
        }
    }

    /// CSS `image-rendering: pixelated` at a non-integral scale: replicate
    /// source pixels to the nearest integral size, leaving only the remainder
    /// to PDF sampling.
    fn pixelated(
        self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<EncodedImage> {
        let (width, height) = (image.source_width, image.source_height);
        if width == 0 || height == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
            return None;
        }
        let scale = CssRasterScale::parse(width, height, display_w_pt, display_h_pt)?;
        if scale.is_integral_multiple() {
            return None;
        }
        let raster = crate::render::blur::pixelated_image_at_css_size(
            &image::load_from_memory(&image.data).ok()?.to_rgba8(),
            display_w_pt,
            display_h_pt,
        )?;
        if (raster.width(), raster.height()) == (width, height) {
            return None;
        }
        let (width, height) = (raster.width(), raster.height());
        let opaque = raster.pixels().all(|pixel| pixel[3] == 255);
        let (raster, format) = if opaque {
            (
                image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(raster).to_rgb8()),
                ImageFormat::Png,
            )
        } else {
            (
                image::DynamicImage::ImageRgba8(raster),
                ImageFormat::PngAlpha,
            )
        };
        let mut data = Vec::new();
        raster
            .write_to(
                &mut std::io::Cursor::new(&mut data),
                image::ImageFormat::Png,
            )
            .ok()?;
        let png_metadata = crate::parser::png::parse_png(&data).map(|info| PngMetadata {
            channels: info.channels,
            bit_depth: info.bit_depth,
        });
        Some(self.encode(
            &data,
            width,
            height,
            format,
            png_metadata.as_ref(),
            PdfImageInterpolation::Crisp,
        ))
    }

    /// The source image resampled to the configured density at its display
    /// size, or `None` when it is not denser than that.
    pub(super) fn downsample(
        self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<ResizedImage> {
        let (source_width, source_height) = (image.source_width, image.source_height);
        if source_width == 0 || source_height == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
            return None;
        }
        let scale = self.downsample_to?.scale();
        let target_w = scale.sample_count(display_w_pt)?;
        let target_h = scale.sample_count(display_h_pt)?;
        let scale = ((target_w as f32 / source_width as f32)
            .min(target_h as f32 / source_height as f32))
        .min(1.0);
        if scale >= 1.0 {
            return None;
        }
        let new_w = ((source_width as f32 * scale).round().max(1.0) as u32).min(source_width);
        let new_h = ((source_height as f32 * scale).round().max(1.0) as u32).min(source_height);
        if new_w >= source_width && new_h >= source_height {
            return None;
        }
        self.resize(image, new_w, new_h)
    }

    /// Resizes and re-encodes `image` to `new_w` x `new_h`.
    fn resize(
        self,
        image: &crate::layout::engine::RasterImageAsset,
        new_w: u32,
        new_h: u32,
    ) -> Option<ResizedImage> {
        let lanczos = image::imageops::FilterType::Lanczos3;
        match image.format {
            ImageFormat::Jpeg => {
                let decoded = image::load_from_memory(&image.data).ok()?.to_rgb8();
                let resized = image::imageops::resize(&decoded, new_w, new_h, lanczos);
                let encoded =
                    encode_rgb_as_jpeg(resized.as_raw(), new_w, new_h, self.jpeg_quality)?;
                Some(ResizedImage {
                    data: encoded,
                    width: new_w,
                    height: new_h,
                    format: ImageFormat::Jpeg,
                    png_metadata: None,
                })
            }
            ImageFormat::Png | ImageFormat::PngAlpha => {
                let png = crate::layout::images::png_bytes_for_decoding(
                    &image.data,
                    image.source_width,
                    image.source_height,
                    image.png_metadata.as_ref(),
                )?;
                let decoded = image::load_from_memory(&png).ok()?;
                let has_alpha = matches!(
                    decoded.color(),
                    image::ColorType::La8
                        | image::ColorType::La16
                        | image::ColorType::Rgba8
                        | image::ColorType::Rgba16
                        | image::ColorType::Rgba32F
                );
                let (resized, output_format) = if has_alpha {
                    let resized =
                        image::imageops::resize(&decoded.to_rgba8(), new_w, new_h, lanczos);
                    (
                        image::DynamicImage::ImageRgba8(resized),
                        ImageFormat::PngAlpha,
                    )
                } else {
                    let resized =
                        image::imageops::resize(&decoded.to_rgb8(), new_w, new_h, lanczos);
                    (image::DynamicImage::ImageRgb8(resized), ImageFormat::Png)
                };
                let mut encoded = Vec::new();
                resized
                    .write_to(
                        &mut std::io::Cursor::new(&mut encoded),
                        image::ImageFormat::Png,
                    )
                    .ok()?;
                let png_metadata =
                    crate::parser::png::parse_png(&encoded).map(|info| PngMetadata {
                        channels: info.channels,
                        bit_depth: info.bit_depth,
                    });
                Some(ResizedImage {
                    data: encoded,
                    width: new_w,
                    height: new_h,
                    format: output_format,
                    png_metadata,
                })
            }
        }
    }
}
