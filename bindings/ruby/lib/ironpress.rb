# frozen_string_literal: true

require "rbconfig"
require_relative "ironpress/version"

ruby_api_version = RUBY_VERSION.split(".").first(2).join(".")
versioned_extension = File.join(__dir__, "ironpress", ruby_api_version, "ironpress_ruby")
source_extension = File.join(__dir__, "ironpress", "ironpress_ruby")
extension_suffix = RbConfig::CONFIG.fetch("DLEXT")
extension = if File.file?("#{versioned_extension}.#{extension_suffix}")
              versioned_extension
            else
              source_extension
            end
require extension

module Ironpress
  # Images encoded by earlier conversions, kept for later ones.
  #
  # Pass one cache to every conversion of a batch that repeats the same
  # images, such as a certificate frame or a watermark. A later conversion
  # writes an image that an earlier one already encoded at the same size
  # instead of encoding it again. The least recently used image is dropped
  # first once the cache holds +byte_limit+ bytes of encoded image data.
  class ImageCache
    # The byte limit of a default cache: 64 MiB of encoded image data.
    DEFAULT_BYTE_LIMIT = 64 * 1024 * 1024

    def initialize(byte_limit = DEFAULT_BYTE_LIMIT)
      unless byte_limit.is_a?(Integer) && byte_limit >= 0
        raise ArgumentError, "image cache byte limit must be a non-negative Integer"
      end

      @native = NativeImageCache.new(byte_limit)
    end

    # Number of encoded images held.
    def size
      @native.size
    end

    # Bytes of encoded image data held.
    def byte_size
      @native.byte_size
    end

    # @api private
    attr_reader :native
  end

  # Mutable Ruby facade over immutable native converter values.
  class HtmlConverter
    CONFIGURATION_METHODS = %i[
      page_size
      page_size_custom
      margin
      margin_sides
      compress
      jpeg_quality
      auto_resize_images
      image_dpi
      filter_dpi
      mask_dpi
      background_raster_dpi
      occlusion_cull
      sanitize
      add_font
      add_font_pack
      base_path
      resource_root
      header
      header_html
      footer
      footer_html
    ].freeze

    def initialize
      @native = NativeConverter.new
    end

    CONFIGURATION_METHODS.each do |name|
      define_method(name) do |*arguments|
        @native = @native.public_send(name, *arguments)
        self
      end
    end

    def convert(html, image_cache: nil)
      return @native.convert(html) if image_cache.nil?

      @native.convert_with_image_cache(html, native_image_cache(image_cache))
    end

    def convert_markdown(markdown, image_cache: nil)
      return @native.convert_markdown(markdown) if image_cache.nil?

      @native.convert_markdown_with_image_cache(markdown, native_image_cache(image_cache))
    end

    def convert_to_file(html, path, image_cache: nil)
      if image_cache.nil?
        @native.convert_to_file(html, path)
      else
        @native.convert_to_file_with_image_cache(html, path, native_image_cache(image_cache))
      end
      self
    end

    def convert_markdown_to_file(markdown, path, image_cache: nil)
      if image_cache.nil?
        @native.convert_markdown_to_file(markdown, path)
      else
        @native.convert_markdown_to_file_with_image_cache(
          markdown, path, native_image_cache(image_cache)
        )
      end
      self
    end

    private

    def native_image_cache(image_cache)
      return image_cache.native if image_cache.is_a?(ImageCache)

      raise TypeError, "image_cache must be an Ironpress::ImageCache"
    end
  end

  private_constant :NativeConverter, :NativeImageCache
end
