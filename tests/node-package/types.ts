import browserInit from 'ironpress';
import nodeInit, { HtmlConverter, ImageCache, htmlToPdf } from 'ironpress/node';

async function render(): Promise<Uint8Array> {
  await nodeInit();
  const converter = new HtmlConverter();
  converter.pageSize('Letter');
  const images = new ImageCache(8 * 1024 * 1024);
  const cached: Uint8Array = converter.htmlToPdfWithImageCache('<h1>Cached</h1>', images);
  const count: number = images.size + images.byteLength;
  void cached;
  void count;
  images.free();
  converter.free();
  return htmlToPdf('<h1>Typed Node.js consumer</h1>');
}

void browserInit;
void render;
