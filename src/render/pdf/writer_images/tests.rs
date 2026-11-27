use super::encoded::EncodedImage;
use super::*;
use crate::layout::engine::RasterImageAsset;
use crate::{HtmlConverter, ImageCache};

fn png(image: image::DynamicImage) -> Vec<u8> {
    let mut data = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut data),
            image::ImageFormat::Png,
        )
        .unwrap();
    data
}

/// Noisy enough to exceed the size at which PNGs are tried as JPEGs.
fn opaque_png(seed: u32) -> Vec<u8> {
    png(image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(
        200,
        200,
        |x, y| {
            let n = (x * 7919 + y * 104_729 + seed * 13) % 251;
            image::Rgb([n as u8, (n * 3 % 256) as u8, (x + y) as u8])
        },
    )))
}

fn translucent_png() -> Vec<u8> {
    png(image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(
        400,
        200,
        |x, y| image::Rgba([200, (x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]),
    )))
}

fn data_uri(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        crate::layout::images::base64_encode(png)
    )
}

fn image_objects(pdf: &[u8]) -> usize {
    pdf.windows(b"/Subtype /Image".len())
        .filter(|window| *window == b"/Subtype /Image")
        .count()
}

fn pages(html_pages: &[String]) -> String {
    let pages: String = html_pages
        .iter()
        .map(|page| format!("<div style=\"page-break-after: always\">{page}</div>"))
        .collect();
    format!("<html><body>{pages}</body></html>")
}

/// Opaque and translucent images, resampled and native, as `<img>` and as a
/// background, on two pages.
fn repeated_images_document() -> String {
    let opaque = data_uri(&opaque_png(1));
    let translucent = data_uri(&translucent_png());
    let page = format!(
        "<img src=\"{opaque}\" style=\"width: 150px\">\
         <img src=\"{translucent}\" style=\"width: 40px\">\
         <div style=\"width: 200px; height: 100px; background-image: url('{translucent}'); background-size: 100% 100%\"></div>"
    );
    pages(&[page.clone(), page])
}

#[test]
fn a_warm_image_cache_writes_the_same_pdf_as_an_uncached_conversion() {
    let converter = HtmlConverter::new();
    let html = repeated_images_document();
    let uncached = converter.convert(&html).unwrap();
    let mut images = ImageCache::new();
    let cold = converter
        .convert_with_image_cache(&html, &mut images)
        .unwrap();
    assert!(!images.is_empty(), "the conversion must fill the cache");
    let warm = converter
        .convert_with_image_cache(&html, &mut images)
        .unwrap();
    assert!(cold == uncached, "a cold cache must not change the PDF");
    assert!(warm == uncached, "a warm cache must not change the PDF");
}

#[test]
fn converters_with_different_image_options_do_not_share_encodings() {
    let html = repeated_images_document();
    let resizing = HtmlConverter::new();
    let native = HtmlConverter::new().auto_resize_images(false);
    assert!(
        resizing.convert(&html).unwrap() != native.convert(&html).unwrap(),
        "the options must change this document's images"
    );
    let mut images = ImageCache::new();
    resizing
        .convert_with_image_cache(&html, &mut images)
        .unwrap();
    let shared = native.convert_with_image_cache(&html, &mut images).unwrap();
    assert!(shared == native.convert(&html).unwrap());
}

#[test]
fn a_background_image_repeated_on_every_page_is_embedded_once() {
    let frame = data_uri(&opaque_png(2));
    let page = format!(
        "<div style=\"width: 300px; height: 200px; background-image: url('{frame}'); background-size: 100% 100%\"></div>"
    );
    let pdf = HtmlConverter::new()
        .convert(&pages(&[page.clone(), page.clone(), page]))
        .unwrap();
    assert_eq!(image_objects(&pdf), 1);
}

#[test]
fn an_undecodable_png_background_is_not_painted() {
    let background = |png: &[u8]| {
        pages(&[format!(
            "<div style=\"width: 300px; height: 200px; background-image: url('{}')\"></div>",
            data_uri(png)
        )])
    };
    let intact = HtmlConverter::new()
        .convert(&background(&opaque_png(3)))
        .unwrap();
    assert_eq!(
        image_objects(&intact),
        1,
        "the intact image must be painted"
    );
    let mut corrupt = opaque_png(3);
    let idat = corrupt
        .windows(4)
        .position(|window| window == b"IDAT")
        .unwrap()
        + 4;
    corrupt[idat..idat + 64].fill(0xFF);
    let mut images = ImageCache::new();
    for _ in 0..2 {
        let pdf = HtmlConverter::new()
            .convert_with_image_cache(&background(&corrupt), &mut images)
            .unwrap();
        assert_eq!(image_objects(&pdf), 0);
    }
}

fn placement(seed: u32) -> LayoutImageCacheKey {
    let asset =
        RasterImageAsset::source(seed.to_be_bytes().to_vec(), 1, 1, ImageFormat::Jpeg, None);
    LayoutImageCacheKey::new(
        &asset,
        10.0,
        10.0,
        crate::style::computed::ImageRendering::Auto,
    )
}

fn encoded(bytes: usize) -> EncodedImage {
    EncodedImage::opaque(ImageStream::flate(1, 1, "/DeviceGray", vec![0; bytes]))
}

/// Ask `cache` for `seed`'s image, reporting whether it had to be encoded.
fn encodes(cache: &mut ImageCache, seed: u32, bytes: usize) -> bool {
    let encoding = ImageEncoding::for_render(&RenderOpts::default());
    let mut encoded_now = false;
    cache.encoded(placement(seed), encoding, || {
        encoded_now = true;
        encoded(bytes)
    });
    encoded_now
}

#[test]
fn a_cached_image_is_not_encoded_again() {
    let mut cache = ImageCache::new();
    assert!(encodes(&mut cache, 1, 100));
    assert!(!encodes(&mut cache, 1, 100));
    assert!(encodes(&mut cache, 2, 100));
}

#[test]
fn the_least_recently_used_image_makes_way_when_the_byte_limit_is_reached() {
    let mut cache = ImageCache::with_byte_limit(250);
    encodes(&mut cache, 1, 100);
    encodes(&mut cache, 2, 100);
    encodes(&mut cache, 1, 100);
    encodes(&mut cache, 3, 100);
    assert_eq!((cache.len(), cache.byte_len()), (2, 200));
    assert!(
        !encodes(&mut cache, 1, 100),
        "recently used image was evicted"
    );
    assert!(
        encodes(&mut cache, 2, 100),
        "least recently used image was kept"
    );
}

#[test]
fn an_image_larger_than_the_byte_limit_is_not_kept() {
    let mut cache = ImageCache::with_byte_limit(250);
    encodes(&mut cache, 1, 100);
    assert!(encodes(&mut cache, 2, 300));
    assert!(encodes(&mut cache, 2, 300));
    assert_eq!((cache.len(), cache.byte_len()), (1, 100));
}
