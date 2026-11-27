# ironpress for Ruby

Generate PDF output from HTML or Markdown with a native Rust renderer. No
browser process or system PDF dependency is required.

[Read the complete Ruby getting-started guide](https://gastongouron.github.io/ironpress/get-started/ruby/).

## Requirements

- Ruby 3.0 or later
- Linux, macOS, or Windows

Releases include supported native gems plus a source gem.

## Install

```bash
bundle add ironpress
```

Or install the gem globally:

```bash
gem install ironpress
```

## Create your first PDF

```ruby
require "ironpress"

pdf = Ironpress.html_to_pdf("<h1>Hello from Ruby</h1>")
File.binwrite("output.pdf", pdf)
```

The result is a binary Ruby `String`. Write it with `File.binwrite`, return it
from a web response, or store it in your application.

## Render Markdown

```ruby
pdf = Ironpress.markdown_to_pdf("# Release notes\n\nEverything shipped.")
```

## Configure and reuse a converter

```ruby
converter = Ironpress::HtmlConverter.new
  .page_size("Letter")
  .margin_sides(36, 48, 36, 48)
  .header("Quarterly report")
  .footer("Page {page} of {pages}")

# Use header_html/footer_html for sanitized images, tables, and styled markup.
converter.header_html("<strong>Quarterly report</strong>")

pdf = converter.convert("<h1>Results</h1>")
markdown_pdf = converter.convert_markdown("# Results")
```

Configuration methods return the converter, so policies compose in an idiomatic
chain. Use `convert_to_file` or `convert_markdown_to_file` for direct output.

## Reuse images across documents

Pass one `Ironpress::ImageCache` to every conversion of a batch that repeats
the same images, such as a certificate frame or a watermark. Later conversions
write those images from the cache instead of encoding them again:

```ruby
images = Ironpress::ImageCache.new # 64 MiB of encoded images; ImageCache.new(byte_limit)
%w[Ada Grace].each do |name|
  pdf = converter.convert("<h1>Certificate for #{name}</h1>", image_cache: images)
end
```

Every conversion method accepts `image_cache:`. Use one cache per thread.

## Local resources and fonts

Local URLs are denied until a canonical boundary is configured:

```ruby
converter = Ironpress::HtmlConverter.new
  .base_path("templates")
  .resource_root(".")
  .add_font("Inter", File.binread("assets/Inter.ttf"))
```

Optional fallback packs enter through `add_font_pack`. Valid names are
`cjk-jp`, `cjk-kr`, `cjk-sc`, `cjk-tc`, and `emoji`. Rendering never downloads
a font pack.

## Limits and errors

- Invalid named settings raise `ArgumentError`.
- Conversion failures raise `RuntimeError`.
- HTML sanitization is enabled by default.
- The binding has no async or streaming conversion API.
- Remote HTTP document resources are not available.
- JavaScript and browser DOM execution are not supported.

See the [binding capability matrix](../README.md) for the contract shared with
Rust, Python, browser WebAssembly, and Node.js.
