using Ironpress.Interop;

namespace Ironpress;

/// <summary>Images encoded by earlier conversions, kept for later ones.</summary>
/// <remarks>
/// Pass one cache to every conversion of a batch that repeats the same images,
/// such as a certificate frame or a watermark. A later conversion writes an image
/// that an earlier one already encoded at the same size instead of encoding it
/// again. The cache owns one native handle: it may move between threads while
/// idle, but two conversions that run at the same time need two caches.
/// </remarks>
public sealed class ImageCache : IDisposable
{
    /// <summary>The byte limit of a default cache: 64 MiB of encoded image data.</summary>
    public const long DefaultByteLimit = 64L * 1024 * 1024;

    private readonly ImageCacheHandle imageCache;

    /// <summary>
    /// Create an empty cache holding up to <paramref name="byteLimit"/> bytes of
    /// encoded image data, dropping the least recently used image first.
    /// </summary>
    public ImageCache(long byteLimit = DefaultByteLimit)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(byteLimit);
        SupportedPlatform.EnsureCurrent();
        IronpressInfo.EnsureCompatibleAbi();
        var status = NativeMethods.ironpress_image_cache_new(
            (nuint)Math.Min((ulong)byteLimit, (ulong)nuint.MaxValue),
            out var owner,
            out var error);
        imageCache = NativeResult.TakeImageCache(status, owner, error);
    }

    /// <summary>Number of encoded images held.</summary>
    public long Count
    {
        get
        {
            EnsureOpen();
            return (long)NativeMethods.ironpress_image_cache_len(imageCache);
        }
    }

    /// <summary>Bytes of encoded image data held.</summary>
    public long ByteCount
    {
        get
        {
            EnsureOpen();
            return (long)NativeMethods.ironpress_image_cache_byte_len(imageCache);
        }
    }

    internal ImageCacheHandle Handle
    {
        get
        {
            EnsureOpen();
            return imageCache;
        }
    }

    /// <summary>Release the native image cache deterministically.</summary>
    public void Dispose() => imageCache.Dispose();

    private void EnsureOpen() =>
        ObjectDisposedException.ThrowIf(imageCache.IsClosed, this);
}
