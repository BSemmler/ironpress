//! Encoded images kept from one conversion for the next.

use std::num::NonZeroUsize;

use lru::LruCache;

use super::cache::{LayoutImageCacheKey, SourceDigest};
use super::encoded::EncodedImage;
use super::encoding::ImageEncoding;

/// Encoded images kept from one conversion for the next.
///
/// Documents rendered from one template usually repeat the same images: a
/// certificate frame, a watermark, a letterhead logo. Decoding, resampling and
/// recompressing those images can dominate the render. Pass one `ImageCache` to
/// every [`HtmlConverter::convert_with_image_cache`](crate::HtmlConverter::convert_with_image_cache)
/// call and later conversions write the finished PDF image streams instead of
/// encoding the images again.
///
/// An entry is reused only for the same image bytes, displayed at the same size
/// with the same `image-rendering`, under the same image options (JPEG quality,
/// automatic resizing and source image DPI). The cache holds at most its byte
/// limit of encoded image data and drops the least recently used image first.
///
/// A conversion borrows the cache mutably, so a multi-threaded caller keeps
/// one cache per thread.
///
/// ```
/// use ironpress::{HtmlConverter, ImageCache};
///
/// let converter = HtmlConverter::new();
/// let mut images = ImageCache::new();
/// for name in ["Ada", "Grace"] {
///     let html = format!("<h1>Certificate for {name}</h1>");
///     let pdf = converter.convert_with_image_cache(&html, &mut images).unwrap();
///     assert!(pdf.starts_with(b"%PDF"));
/// }
/// ```
pub struct ImageCache {
    encoded: LruCache<EncodedImageKey, EncodedImage>,
    encoded_bytes: usize,
    byte_limit: usize,
    /// PNG sources already proven to decode. A PNG that compresses well is
    /// embedded without being decoded, but a background or SVG image must
    /// decode before it may be painted.
    decodable: LruCache<SourceDigest, ()>,
}

#[derive(PartialEq, Eq, Hash)]
struct EncodedImageKey {
    placement: LayoutImageCacheKey,
    encoding: ImageEncoding,
}

/// Remembering a proven source costs one digest, so this bound is far above
/// the number of distinct images any batch repeats.
const DECODABLE_SOURCES: NonZeroUsize = match NonZeroUsize::new(1024) {
    Some(capacity) => capacity,
    None => NonZeroUsize::MIN,
};

impl ImageCache {
    /// The byte limit of [`ImageCache::new`]: 64 MiB of encoded image data.
    pub const DEFAULT_BYTE_LIMIT: usize = 64 * 1024 * 1024;

    /// An empty cache holding up to [`ImageCache::DEFAULT_BYTE_LIMIT`] bytes.
    pub fn new() -> Self {
        Self::with_byte_limit(Self::DEFAULT_BYTE_LIMIT)
    }

    /// An empty cache holding up to `byte_limit` bytes of encoded image data.
    /// An image larger than the limit is encoded every time it is used.
    pub fn with_byte_limit(byte_limit: usize) -> Self {
        Self {
            encoded: LruCache::unbounded(),
            encoded_bytes: 0,
            byte_limit,
            decodable: LruCache::new(DECODABLE_SOURCES),
        }
    }

    /// Number of encoded images held.
    pub fn len(&self) -> usize {
        self.encoded.len()
    }

    /// Whether no encoded image is held.
    pub fn is_empty(&self) -> bool {
        self.encoded.is_empty()
    }

    /// Bytes of encoded image data held.
    pub fn byte_len(&self) -> usize {
        self.encoded_bytes
    }

    /// The encoding stored for `placement` under `encoding`, or the result of
    /// `encode`, which is stored for next time.
    pub(super) fn encoded(
        &mut self,
        placement: LayoutImageCacheKey,
        encoding: ImageEncoding,
        encode: impl FnOnce() -> EncodedImage,
    ) -> EncodedImage {
        let key = EncodedImageKey {
            placement,
            encoding,
        };
        if let Some(image) = self.encoded.get(&key) {
            return image.clone();
        }
        let image = encode();
        self.insert(key, image.clone());
        image
    }

    fn insert(&mut self, key: EncodedImageKey, image: EncodedImage) {
        let size = image.byte_len();
        if size > self.byte_limit {
            return;
        }
        if let Some(replaced) = self.encoded.put(key, image) {
            self.encoded_bytes -= replaced.byte_len();
        }
        self.encoded_bytes += size;
        while self.encoded_bytes > self.byte_limit {
            let Some((_, evicted)) = self.encoded.pop_lru() else {
                break;
            };
            self.encoded_bytes -= evicted.byte_len();
        }
    }

    pub(super) fn is_known_decodable(&mut self, source: SourceDigest) -> bool {
        self.decodable.get(&source).is_some()
    }

    pub(super) fn remember_decodable(&mut self, source: SourceDigest) {
        self.decodable.put(source, ());
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCache")
            .field("images", &self.encoded.len())
            .field("byte_len", &self.encoded_bytes)
            .field("byte_limit", &self.byte_limit)
            .finish()
    }
}
