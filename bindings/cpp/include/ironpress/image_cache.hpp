#pragma once

#include "ironpress/detail/native.hpp"

#include <cstddef>
#include <utility>

namespace ironpress {

class Converter;

/// Images encoded by earlier conversions, with unique native ownership.
///
/// Pass one cache to every conversion of a batch that repeats the same images,
/// such as a certificate frame or a watermark. A later conversion writes an
/// image that an earlier one already encoded at the same size instead of
/// encoding it again. Two conversions that run at the same time need two
/// caches.
class ImageCache final {
public:
    /// The byte limit of a default cache: 64 MiB of encoded image data.
    static constexpr std::size_t default_byte_limit =
        IRONPRESS_IMAGE_CACHE_DEFAULT_BYTE_LIMIT;

    /// Allocate an empty cache that holds up to `byte_limit` bytes of encoded
    /// image data, dropping the least recently used image first.
    explicit ImageCache(std::size_t byte_limit = default_byte_limit) {
        detail::NativeResult::require_compatible_abi();
        IronpressImageCache* image_cache = nullptr;
        IronpressError* error = nullptr;
        const auto status =
            ironpress_image_cache_new(byte_limit, &image_cache, &error);
        detail::ImageCacheOwner owner(image_cache);
        detail::NativeResult::require_success(status, error);
        if (!owner) {
            throw detail::NativeResult::contract_violation(
                "Native image cache allocation returned no owner.");
        }
        owner_ = std::move(owner);
    }

    ImageCache(const ImageCache&) = delete;
    ImageCache& operator=(const ImageCache&) = delete;
    ImageCache(ImageCache&&) noexcept = default;
    ImageCache& operator=(ImageCache&&) noexcept = default;
    ~ImageCache() noexcept = default;

    /// Report whether this object still owns a native image cache.
    [[nodiscard]] explicit operator bool() const noexcept {
        return static_cast<bool>(owner_);
    }

    /// Number of encoded images held.
    [[nodiscard]] std::size_t size() const noexcept {
        return ironpress_image_cache_len(owner_.get());
    }

    /// Bytes of encoded image data held.
    [[nodiscard]] std::size_t byte_size() const noexcept {
        return ironpress_image_cache_byte_len(owner_.get());
    }

private:
    friend class Converter;

    detail::ImageCacheOwner owner_;
};

}
