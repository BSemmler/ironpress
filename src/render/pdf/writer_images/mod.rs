use super::*;
use std::sync::Arc;

mod cache;
mod scale;
mod streams;

pub(super) use cache::LayoutImageCacheKey;
use scale::CssRasterScale;

#[derive(Clone, Copy)]
pub(super) enum PdfImageInterpolation {
    Default,
    Smooth,
    Crisp,
}

impl PdfImageInterpolation {
    pub(super) fn for_css_image_rendering(
        image_rendering: crate::style::computed::ImageRendering,
    ) -> Self {
        if image_rendering.preserves_source_edges() {
            Self::Crisp
        } else if image_rendering.requests_smooth_pdf_interpolation() {
            Self::Smooth
        } else {
            Self::Default
        }
    }

    const fn pdf_dictionary_entry(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Smooth => " /Interpolate true",
            Self::Crisp => " /Interpolate false",
        }
    }

    const fn preserves_source_edges(self) -> bool {
        matches!(self, Self::Crisp)
    }
}

impl PdfWriter {
    /// Embed a layout image at its display size.
    ///
    /// Document source images participate in the configurable source-image
    /// downscaling policy. Renderer-owned rasters keep their native dimensions:
    /// their pixels were deliberately generated at a filter or background DPI.
    pub(crate) fn add_layout_image_object(
        &mut self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> usize {
        let cache_key =
            LayoutImageCacheKey::new(image, display_w_pt, display_h_pt, image_rendering);
        if let Some(object_id) = self.layout_image_objects.get(&cache_key) {
            return *object_id;
        }
        let interpolation = PdfImageInterpolation::for_css_image_rendering(image_rendering);
        let object_id = if image.origin.preserves_native_resolution() {
            self.add_image_object_with_interpolation(
                &image.data,
                image.source_width,
                image.source_height,
                image.format,
                image.png_metadata.as_ref(),
                interpolation,
            )
        } else if image_rendering.is_pixelated() {
            self.add_pixelated_source_image_object(
                &image.data,
                image.source_width,
                image.source_height,
                display_w_pt,
                display_h_pt,
            )
            .unwrap_or_else(|| {
                self.add_image_object_with_interpolation(
                    &image.data,
                    image.source_width,
                    image.source_height,
                    image.format,
                    image.png_metadata.as_ref(),
                    interpolation,
                )
            })
        } else {
            self.add_source_image_object_with_interpolation(
                &image.data,
                image.source_width,
                image.source_height,
                image.format,
                image.png_metadata.as_ref(),
                display_w_pt,
                display_h_pt,
                interpolation,
            )
        };
        self.layout_image_objects.insert(cache_key, object_id);
        object_id
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> usize {
        self.add_source_image_object_with_interpolation(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
            PdfImageInterpolation::Default,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_source_image_object_with_interpolation(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        if let Some(resized) = self.maybe_resize_image(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
        ) {
            self.add_image_object_with_interpolation(
                &resized.data,
                resized.width,
                resized.height,
                resized.format,
                resized.png_metadata.as_ref(),
                interpolation,
            )
        } else {
            self.add_image_object_with_interpolation(
                data,
                width,
                height,
                format,
                png_metadata,
                interpolation,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_pixelated_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<usize> {
        if width == 0 || height == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
            return None;
        }
        let scale = CssRasterScale::parse(width, height, display_w_pt, display_h_pt)?;
        if scale.is_integral_multiple() {
            return None;
        }
        let raster = crate::render::blur::pixelated_image_at_css_size(
            &image::load_from_memory(data).ok()?.to_rgba8(),
            display_w_pt,
            display_h_pt,
        )?;
        if (raster.width(), raster.height()) == (width, height) {
            return None;
        }
        let width = raster.width();
        let height = raster.height();
        let opaque = raster.pixels().all(|pixel| pixel[3] == 255);
        let mut data = Vec::new();
        let (format, png_metadata) = if opaque {
            image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(raster).to_rgb8())
                .write_to(
                    &mut std::io::Cursor::new(&mut data),
                    image::ImageFormat::Png,
                )
                .ok()?;
            let png_metadata = crate::parser::png::parse_png(&data).map(|info| PngMetadata {
                channels: info.channels,
                bit_depth: info.bit_depth,
            });
            (ImageFormat::Png, png_metadata)
        } else {
            image::DynamicImage::ImageRgba8(raster)
                .write_to(
                    &mut std::io::Cursor::new(&mut data),
                    image::ImageFormat::Png,
                )
                .ok()?;
            let png_metadata = crate::parser::png::parse_png(&data).map(|info| PngMetadata {
                channels: info.channels,
                bit_depth: info.bit_depth,
            });
            (ImageFormat::PngAlpha, png_metadata)
        };
        Some(self.add_image_object_with_interpolation(
            &data,
            width,
            height,
            format,
            png_metadata.as_ref(),
            PdfImageInterpolation::Crisp,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_decodable_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<usize> {
        if matches!(format, ImageFormat::Png | ImageFormat::PngAlpha)
            && decode_png_for_pdf(data).is_none()
        {
            return None;
        }
        Some(self.add_source_image_object(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn maybe_resize_image(
        &self,
        data: &[u8],
        source_width: u32,
        source_height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<ResizedImage> {
        if !self.opts.auto_resize_images
            || source_width == 0
            || source_height == 0
            || display_w_pt <= 0.0
            || display_h_pt <= 0.0
        {
            return None;
        }

        let scale = crate::render::raster_scale::RasterScale::at_dpi(
            self.opts.raster_quality.source_image_dpi,
        );
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

        // The same image (a letterhead logo, say) is usually resized identically
        // in every document of a batch, and the resize and re-encode is costly
        let key = ResizeKey {
            source_hash: {
                let mut hasher = std::hash::DefaultHasher::new();
                std::hash::Hash::hash(data, &mut hasher);
                std::hash::Hasher::finish(&hasher)
            },
            new_w,
            new_h,
            format,
            png_layout: png_metadata.map(|m| (m.channels, m.bit_depth)),
            jpeg_quality: self.opts.jpeg_quality,
        };
        // The hash only finds the entry; the stored source bytes decide the hit
        if let Some(cached) = RESIZE_CACHE.get(&key)
            && cached.source.as_slice() == data
        {
            return Some(cached.resized.clone());
        }
        let resized = self.resize_image(
            data,
            source_width,
            source_height,
            format,
            png_metadata,
            new_w,
            new_h,
        )?;
        RESIZE_CACHE.insert(
            key,
            Arc::new(CachedResize {
                source: data.to_vec(),
                resized: resized.clone(),
            }),
        );
        Some(resized)
    }

    /// Resizes and re-encodes `data` to `new_w` x `new_h`; no caching
    #[allow(clippy::too_many_arguments)]
    fn resize_image(
        &self,
        data: &[u8],
        source_width: u32,
        source_height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        new_w: u32,
        new_h: u32,
    ) -> Option<ResizedImage> {
        match format {
            ImageFormat::Jpeg => {
                let decoded = image::load_from_memory(data).ok()?.to_rgb8();
                let resized = image::imageops::resize(
                    &decoded,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Lanczos3,
                );
                let encoded =
                    encode_rgb_as_jpeg(resized.as_raw(), new_w, new_h, self.opts.jpeg_quality)?;
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
                    data,
                    source_width,
                    source_height,
                    png_metadata,
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
                let mut encoded = Vec::new();
                let output_format = if has_alpha {
                    let rgba = decoded.to_rgba8();
                    let resized = image::imageops::resize(
                        &rgba,
                        new_w,
                        new_h,
                        image::imageops::FilterType::Lanczos3,
                    );
                    image::DynamicImage::ImageRgba8(resized)
                        .write_to(
                            &mut std::io::Cursor::new(&mut encoded),
                            image::ImageFormat::Png,
                        )
                        .ok()?;
                    ImageFormat::PngAlpha
                } else {
                    let rgb = decoded.to_rgb8();
                    let resized = image::imageops::resize(
                        &rgb,
                        new_w,
                        new_h,
                        image::imageops::FilterType::Lanczos3,
                    );
                    image::DynamicImage::ImageRgb8(resized)
                        .write_to(
                            &mut std::io::Cursor::new(&mut encoded),
                            image::ImageFormat::Png,
                        )
                        .ok()?;
                    ImageFormat::Png
                };
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

/// A batch rarely repeats more than a few distinct images (logos, signatures),
/// and each entry holds the source and resized bytes.
const RESIZE_CACHE_CAPACITY: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(16) {
    Some(capacity) => capacity,
    None => std::num::NonZeroUsize::MIN,
};

/// Resized source images, shared across conversions. Keyed by a hash of the
/// source bytes plus everything that shapes the output; each entry keeps the
/// source bytes so a hash collision is never mistaken for a hit.
static RESIZE_CACHE: std::sync::LazyLock<
    crate::bounded_cache::BoundedProcessCache<ResizeKey, Arc<CachedResize>>,
> = std::sync::LazyLock::new(|| {
    crate::bounded_cache::BoundedProcessCache::new(RESIZE_CACHE_CAPACITY)
});

#[derive(PartialEq, Eq, Hash)]
struct ResizeKey {
    source_hash: u64,
    new_w: u32,
    new_h: u32,
    format: ImageFormat,
    png_layout: Option<(u8, u8)>,
    jpeg_quality: u8,
}

struct CachedResize {
    source: Vec<u8>,
    resized: ResizedImage,
}

#[cfg(test)]
mod resize_cache_tests {
    use super::*;

    fn rgb_png(seed: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(96, 96, |x, y| {
            image::Rgb([
                ((x * 3 + y + seed) % 256) as u8,
                ((x + y * 5 + seed * 7) % 256) as u8,
                ((x * 7 + y * 11) % 256) as u8,
            ])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    /// `maybe_resize_image` for a 96px PNG shown at 12pt, which always downscales
    fn resize(writer: &PdfWriter, png: &[u8]) -> ResizedImage {
        writer
            .maybe_resize_image(png, 96, 96, ImageFormat::Png, None, 12.0, 12.0)
            .unwrap()
    }

    fn uncached(writer: &PdfWriter, png: &[u8], like: &ResizedImage) -> ResizedImage {
        writer
            .resize_image(png, 96, 96, ImageFormat::Png, None, like.width, like.height)
            .unwrap()
    }

    #[test]
    fn repeated_resizes_match_an_uncached_resize() {
        let writer = PdfWriter::new();
        let png = rgb_png(1);
        let first = resize(&writer, &png);
        let again = resize(&writer, &png);
        let fresh = uncached(&writer, &png, &first);
        assert!(
            first.width < 96 && first.height < 96,
            "test image must downscale"
        );
        for cached in [&first, &again] {
            assert_eq!(cached.data, fresh.data);
            assert_eq!((cached.width, cached.height), (fresh.width, fresh.height));
            assert_eq!(cached.format, fresh.format);
        }
    }

    #[test]
    fn different_sources_of_the_same_size_are_not_shared() {
        let writer = PdfWriter::new();
        let (a, b) = (rgb_png(2), rgb_png(3));
        let resized_a = resize(&writer, &a);
        let resized_b = resize(&writer, &b);
        assert_ne!(resized_a.data, resized_b.data);
        assert_eq!(resized_b.data, uncached(&writer, &b, &resized_b).data);
        assert_eq!(resize(&writer, &a).data, resized_a.data);
    }
}
