use ironpress_core::ImageCache;

use crate::handles::{
    IronpressBuffer, IronpressConverter, IronpressError, IronpressImageCache, OutputSlot, boundary,
    free_owned,
};
use crate::input::IronpressBytes;
use crate::status::IronpressStatus;

/// The byte limit of a default Rust `ImageCache`: 64 MiB of encoded image data.
pub const IRONPRESS_IMAGE_CACHE_DEFAULT_BYTE_LIMIT: usize = 64 * 1024 * 1024;

/// Allocate an empty image cache holding up to `byte_limit` bytes of encoded
/// image data, and transfer it into an empty output slot.
///
/// Pass the cache to the `_with_image_cache` conversions of every document
/// that repeats the same images. A later conversion writes an image an earlier
/// one already encoded at the same size instead of encoding it again. The
/// least recently used image is dropped first when the limit is reached.
///
/// # Safety
///
/// Output pointers must follow the ownership and slot rules in `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_image_cache_new(
    byte_limit: usize,
    out_image_cache: *mut *mut IronpressImageCache,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw output slots are validated before they are written.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_image_cache, "image cache")?;
            output.write(IronpressImageCache {
                images: ImageCache::with_byte_limit(byte_limit),
            });
            Ok(())
        })
    }
}

/// Release an image cache and clear its owning handle.
///
/// # Safety
///
/// `image_cache` must identify the unique handle returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_image_cache_free(
    image_cache: *mut *mut IronpressImageCache,
) -> IronpressStatus {
    // SAFETY: The caller accepts the ownership contract above.
    unsafe { free_owned(image_cache) }
}

/// Return the number of encoded images held, or zero for a null handle.
///
/// # Safety
///
/// A non-null `image_cache` must identify a live cache owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_image_cache_len(
    image_cache: *const IronpressImageCache,
) -> usize {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { image_cache.as_ref() }.map_or(0, |cache| cache.images.len())
}

/// Return the bytes of encoded image data held, or zero for a null handle.
///
/// # Safety
///
/// A non-null `image_cache` must identify a live cache owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_image_cache_byte_len(
    image_cache: *const IronpressImageCache,
) -> usize {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { image_cache.as_ref() }.map_or(0, |cache| cache.images.byte_len())
}

/// Convert UTF-8 HTML through a configured converter, reusing and filling
/// `image_cache`.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_convert_html_with_image_cache(
    converter: *const IronpressConverter,
    image_cache: *mut IronpressImageCache,
    html: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let converter = IronpressConverter::parse_ref(converter)?;
            let images = &mut IronpressImageCache::parse_mut(image_cache)?.images;
            let html = html.parse_text("HTML")?;
            let pdf = converter.converter.convert_with_image_cache(html, images)?;
            output.write(IronpressBuffer::new(pdf));
            Ok(())
        })
    }
}

/// Convert UTF-8 Markdown through a configured converter, reusing and filling
/// `image_cache`.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_convert_markdown_with_image_cache(
    converter: *const IronpressConverter,
    image_cache: *mut IronpressImageCache,
    markdown: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let converter = IronpressConverter::parse_ref(converter)?;
            let images = &mut IronpressImageCache::parse_mut(image_cache)?.images;
            let markdown = markdown.parse_text("Markdown")?;
            let pdf = converter
                .converter
                .convert_markdown_with_image_cache(markdown, images)?;
            output.write(IronpressBuffer::new(pdf));
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_byte_limit_matches_the_rust_default() {
        assert_eq!(
            IRONPRESS_IMAGE_CACHE_DEFAULT_BYTE_LIMIT,
            ImageCache::DEFAULT_BYTE_LIMIT
        );
    }
}
