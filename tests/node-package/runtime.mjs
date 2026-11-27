import { readFile } from 'node:fs/promises';

import browserInit from 'ironpress';
import init, { HtmlConverter, ImageCache } from 'ironpress/node';

if (typeof browserInit !== 'function') {
  throw new Error('the browser package entry point no longer exports its initializer');
}

const [firstRuntime, secondRuntime] = await Promise.all([init(), init()]);
if (firstRuntime !== secondRuntime || (await init()) !== firstRuntime) {
  throw new Error('Node initialization is not idempotent');
}

const converter = new HtmlConverter();
converter.headerHtml('<strong>Node HTML header</strong>');
converter.footerHtml('<em>Node HTML footer</em>');
converter.pageSizeCustom(320, 480);
converter.addFontPack('cjk-jp', await readFile(process.argv[2]));

const pdf = Buffer.from(converter.htmlToPdf("<p lang='ja'>第</p>"));
converter.free();

if (!pdf.subarray(0, 4).equals(Buffer.from('%PDF'))) {
  throw new Error('the installed Node package did not produce a PDF');
}
if (!pdf.includes(Buffer.from('/MediaBox [0 0 320 480]'))) {
  throw new Error('the Node converter configuration did not reach the PDF');
}
if (!pdf.includes(Buffer.from('DroidSansFallback'))) {
  throw new Error('the caller-provided font pack did not reach the PDF');
}

const imageHtml =
  '<img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlE' +
  'QVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==" width="50" height="50">';
const cachingConverter = new HtmlConverter();
cachingConverter.compress(false);
const images = new ImageCache();
if (images.size !== 0) {
  throw new Error('a new image cache is not empty');
}
const uncached = Buffer.from(cachingConverter.htmlToPdf(imageHtml));
for (let pass = 0; pass < 2; pass += 1) {
  const cached = Buffer.from(cachingConverter.htmlToPdfWithImageCache(imageHtml, images));
  if (!cached.equals(uncached)) {
    throw new Error('the image cache changed the PDF');
  }
}
if (images.size !== 1 || images.byteLength <= 0) {
  throw new Error('the conversion did not fill the image cache');
}
cachingConverter.markdownToPdfWithImageCache('# Cached', images);
let rejectedByteLimit = false;
try {
  new ImageCache(-1);
} catch {
  rejectedByteLimit = true;
}
if (!rejectedByteLimit) {
  throw new Error('a negative image cache byte limit was accepted');
}
images.free();
cachingConverter.free();
