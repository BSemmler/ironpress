use sha2::{Digest, Sha256};

use super::*;

/// SHA-256 identity of a raster asset: its bytes, layout and origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SourceDigest([u8; 32]);

/// One raster asset sampled for one display size and `image-rendering`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::render::pdf) struct LayoutImageCacheKey {
    source: SourceDigest,
    display_width: u32,
    display_height: u32,
    rendering: crate::style::computed::ImageRendering,
}

impl LayoutImageCacheKey {
    pub(super) fn new(
        image: &crate::layout::engine::RasterImageAsset,
        display_width: f32,
        display_height: f32,
        rendering: crate::style::computed::ImageRendering,
    ) -> Self {
        let mut source = Sha256::new();
        source.update(b"ironpress-layout-image-v1");
        source.update(image.source_width.to_be_bytes());
        source.update(image.source_height.to_be_bytes());
        source.update([match image.format {
            ImageFormat::Jpeg => 0,
            ImageFormat::Png => 1,
            ImageFormat::PngAlpha => 2,
        }]);
        if let Some(metadata) = &image.png_metadata {
            source.update([1, metadata.channels, metadata.bit_depth]);
        } else {
            source.update([0]);
        }
        match image.origin {
            crate::layout::engine::RasterImageOrigin::Source => source.update([0]),
            crate::layout::engine::RasterImageOrigin::Rendered(density) => {
                source.update([1]);
                source.update(density.dpi().to_bits().to_be_bytes());
            }
        }
        source.update(&image.data);
        Self {
            source: SourceDigest(source.finalize().into()),
            display_width: display_width.to_bits(),
            display_height: display_height.to_bits(),
            rendering,
        }
    }

    pub(super) fn source(&self) -> SourceDigest {
        self.source
    }
}
