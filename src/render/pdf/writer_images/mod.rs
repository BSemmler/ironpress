use super::*;

mod cache;
mod encoded;
mod encoding;
mod image_cache;
mod resample;
mod scale;
mod streams;
#[cfg(test)]
mod tests;

pub(super) use cache::LayoutImageCacheKey;
pub(super) use encoded::ImageStream;
pub(super) use encoding::ImageEncoding;
pub use image_cache::ImageCache;

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
    /// One document embeds each image once per display size and
    /// `image-rendering`; the conversion's [`ImageCache`] supplies encodings
    /// that an earlier conversion already made.
    pub(crate) fn add_layout_image_object(
        &mut self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> usize {
        let placement =
            LayoutImageCacheKey::new(image, display_w_pt, display_h_pt, image_rendering);
        self.add_placed_image_object(
            placement,
            image,
            display_w_pt,
            display_h_pt,
            image_rendering,
        )
    }

    /// Like [`Self::add_layout_image_object`], for images that must not be
    /// painted unless they decode: `None` for a PNG that does not.
    pub(crate) fn add_decodable_layout_image_object(
        &mut self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> Option<usize> {
        let placement =
            LayoutImageCacheKey::new(image, display_w_pt, display_h_pt, image_rendering);
        if matches!(image.format, ImageFormat::Png | ImageFormat::PngAlpha)
            && !self.images.is_known_decodable(placement.source())
        {
            decode_png_for_pdf(&image.data)?;
            self.images.remember_decodable(placement.source());
        }
        Some(self.add_placed_image_object(
            placement,
            image,
            display_w_pt,
            display_h_pt,
            image_rendering,
        ))
    }

    fn add_placed_image_object(
        &mut self,
        placement: LayoutImageCacheKey,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> usize {
        if let Some(object_id) = self.layout_image_objects.get(&placement) {
            return *object_id;
        }
        let encoding = ImageEncoding::for_render(&self.opts);
        let encoded = self.images.encoded(placement, encoding, || {
            encoding.encode_layout_image(image, display_w_pt, display_h_pt, image_rendering)
        });
        let object_id = self.add_encoded_image(
            encoded,
            PdfImageInterpolation::for_css_image_rendering(image_rendering),
        );
        self.layout_image_objects.insert(placement, object_id);
        object_id
    }
}
