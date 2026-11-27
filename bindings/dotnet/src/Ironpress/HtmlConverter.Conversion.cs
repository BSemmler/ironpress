using System.Diagnostics;
using Ironpress.Interop;

namespace Ironpress;

public sealed partial class HtmlConverter
{
    /// <summary>Convert UTF-16 .NET HTML text to owned PDF bytes.</summary>
    public byte[] ConvertHtml(string html) => Convert(DocumentKind.Html, html, nameof(html), null);

    /// <summary>
    /// Convert UTF-16 .NET HTML text to owned PDF bytes, reusing and filling
    /// <paramref name="images"/>.
    /// </summary>
    public byte[] ConvertHtml(string html, ImageCache images) =>
        Convert(DocumentKind.Html, html, nameof(html), RequireCache(images));

    /// <summary>Convert UTF-16 .NET Markdown text to owned PDF bytes.</summary>
    public byte[] ConvertMarkdown(string markdown) =>
        Convert(DocumentKind.Markdown, markdown, nameof(markdown), null);

    /// <summary>
    /// Convert UTF-16 .NET Markdown text to owned PDF bytes, reusing and filling
    /// <paramref name="images"/>.
    /// </summary>
    public byte[] ConvertMarkdown(string markdown, ImageCache images) =>
        Convert(DocumentKind.Markdown, markdown, nameof(markdown), RequireCache(images));

    private static ImageCache RequireCache(ImageCache images)
    {
        ArgumentNullException.ThrowIfNull(images);
        return images;
    }

    private unsafe byte[] Convert(
        DocumentKind kind,
        string source,
        string parameterName,
        ImageCache? images)
    {
        EnsureOpen();
        var encoded = Utf8Input.Encode(source, parameterName);
        fixed (byte* data = encoded)
        {
            var input = new NativeBytes((nint)data, (nuint)encoded.Length);
            nint pdf;
            nint error;
            var status = (kind, images) switch
            {
                (DocumentKind.Html, null) => NativeMethods.ironpress_converter_convert_html(
                    converter, input, out pdf, out error),
                (DocumentKind.Markdown, null) => NativeMethods.ironpress_converter_convert_markdown(
                    converter, input, out pdf, out error),
                (DocumentKind.Html, { } cache) =>
                    NativeMethods.ironpress_converter_convert_html_with_image_cache(
                        converter, cache.Handle, input, out pdf, out error),
                (DocumentKind.Markdown, { } cache) =>
                    NativeMethods.ironpress_converter_convert_markdown_with_image_cache(
                        converter, cache.Handle, input, out pdf, out error),
                _ => throw new UnreachableException(),
            };
            return NativeResult.TakePdf(status, pdf, error);
        }
    }

    private enum DocumentKind
    {
        Html,
        Markdown,
    }
}
