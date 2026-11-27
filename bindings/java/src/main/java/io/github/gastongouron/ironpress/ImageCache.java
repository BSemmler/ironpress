package io.github.gastongouron.ironpress;

import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.PointerByReference;
import io.github.gastongouron.ironpress.internal.SizeT;

/**
 * Images encoded by earlier conversions, kept for later ones.
 *
 * <p>Pass one cache to every conversion of a batch that repeats the same images, such as a
 * certificate frame or a watermark. A later conversion writes an image that an earlier one already
 * encoded at the same size instead of encoding it again. The cache owns one native handle: it may
 * move between threads while idle, but two conversions that run at the same time need two caches.
 */
public final class ImageCache implements AutoCloseable {
  /** The byte limit of a default cache: 64 MiB of encoded image data. */
  public static final long DEFAULT_BYTE_LIMIT = 64L * 1024 * 1024;

  private final NativeApi api;
  private Pointer imageCache;

  /** Create an empty cache holding up to {@link #DEFAULT_BYTE_LIMIT} bytes of encoded images. */
  public ImageCache() {
    this(DEFAULT_BYTE_LIMIT);
  }

  /**
   * Create an empty cache holding up to {@code byteLimit} bytes of encoded image data, dropping the
   * least recently used image first.
   */
  public ImageCache(long byteLimit) {
    if (byteLimit < 0) {
      throw new IllegalArgumentException("Image cache byte limit must not be negative.");
    }
    api = NativeLibraryLoader.api();
    var owner = new PointerByReference();
    var error = new PointerByReference();
    var status = api.ironpress_image_cache_new(nativeByteLimit(byteLimit), owner, error);
    imageCache = NativeResult.takeImageCache(api, status, owner, error);
  }

  /** Number of encoded images held. */
  public long size() {
    return api.ironpress_image_cache_len(pointer()).longValue();
  }

  /** Bytes of encoded image data held. */
  public long byteSize() {
    return api.ironpress_image_cache_byte_len(pointer()).longValue();
  }

  /** Release the native image cache deterministically. */
  @Override
  public void close() {
    if (imageCache != null) {
      NativeResult.closeImageCache(api, imageCache);
      imageCache = null;
    }
  }

  Pointer pointer() {
    if (imageCache == null) {
      throw new IllegalStateException("This Ironpress image cache is already closed.");
    }
    return imageCache;
  }

  /** A 32-bit {@code size_t} cannot hold larger limits; such a limit means no practical limit. */
  private static SizeT nativeByteLimit(long byteLimit) {
    var largest = Native.SIZE_T_SIZE >= Long.BYTES ? Long.MAX_VALUE : 0xFFFF_FFFFL;
    return new SizeT(Math.min(byteLimit, largest));
  }
}
