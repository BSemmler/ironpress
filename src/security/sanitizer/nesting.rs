//! Element nesting depth of raw HTML, measured before the document is parsed.

/// Elements that never have content or an end tag
/// (HTML Living Standard, 13.1.2 "Elements", void elements).
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Elements whose content is text up to their end tag, so a `<` inside them never starts a tag
/// (HTML Living Standard, 13.1.2.5 raw text and escapable raw text elements).
const RAW_TEXT_ELEMENTS: [&str; 4] = ["script", "style", "textarea", "title"];

/// Roots of foreign content. HTML ignores the self-closing flag on its own non-void elements
/// (`<div/>` opens a `div`), but honors it on SVG and MathML elements
/// (HTML Living Standard, 13.2.6.5 rules for parsing tokens in foreign content).
const FOREIGN_ROOTS: [&str; 2] = ["svg", "math"];

/// How one tag changes the stack of open elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NestingChange {
    Open,
    Close,
}

/// A tag as written in the source: its name and how it was closed.
struct TagToken<'a> {
    name: &'a str,
    is_end_tag: bool,
    is_self_closing: bool,
}

impl TagToken<'_> {
    fn nesting_change(&self, in_foreign_content: bool) -> Option<NestingChange> {
        let honors_self_closing = in_foreign_content || self.is_one_of(&FOREIGN_ROOTS);
        if self.is_end_tag {
            Some(NestingChange::Close)
        } else if self.is_one_of(&VOID_ELEMENTS) || (self.is_self_closing && honors_self_closing) {
            None
        } else {
            Some(NestingChange::Open)
        }
    }

    fn is_one_of(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(self.name))
    }
}

/// Walks raw HTML and yields how each tag changes the element nesting.
///
/// Comments, declarations, quoted attribute values and raw text element content are skipped, so
/// a `/` or `>` inside them is never mistaken for tag syntax. Implied end tags (an unclosed `<p>`
/// or `<li>`) are not modeled, so the depth is an upper bound for documents that rely on them.
pub(super) struct NestingScanner<'a> {
    html: &'a str,
    position: usize,
    /// How many `<svg>` or `<math>` elements are currently open.
    open_foreign_roots: usize,
}

impl<'a> NestingScanner<'a> {
    pub(super) fn new(html: &'a str) -> Self {
        Self {
            html,
            position: 0,
            open_foreign_roots: 0,
        }
    }

    /// The deepest nesting of open elements reached anywhere in the document.
    pub(super) fn max_depth(self) -> usize {
        self.fold((0_usize, 0_usize), |(depth, max), change| match change {
            NestingChange::Open => (depth + 1, max.max(depth + 1)),
            NestingChange::Close => (depth.saturating_sub(1), max),
        })
        .1
    }

    fn rest(&self) -> &'a str {
        self.html.get(self.position..).unwrap_or_default()
    }

    /// Moves past the markup construct starting at the current `<` and returns the tag it
    /// spelled, if any.
    fn read_markup(&mut self) -> Option<TagToken<'a>> {
        let rest = self.rest();
        if let Some(comment) = rest.strip_prefix("<!--") {
            self.position += comment
                .find("-->")
                .map_or(rest.len(), |end| "<!--".len() + end + "-->".len());
            return None;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            self.position += rest.find('>').map_or(rest.len(), |end| end + 1);
            return None;
        }

        let is_end_tag = rest.starts_with("</");
        let after_open = &rest[if is_end_tag { 2 } else { 1 }..];
        let name_length = after_open
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':'))
            .count();
        if !after_open.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // A `<` not followed by a tag name is text, e.g. `1 < 2`.
            self.position += 1;
            return None;
        }

        let name = &after_open[..name_length];
        let attributes = AttributeSpan::read(&after_open[name_length..]);
        self.position += rest.len() - after_open.len() + name_length + attributes.consumed;
        Some(TagToken {
            name,
            is_end_tag,
            is_self_closing: attributes.is_self_closing,
        })
    }

    /// Moves to the end tag of a raw text element, so its content is never scanned for tags.
    fn skip_raw_text(&mut self, name: &str) {
        let rest = self.rest();
        let end_tag = rest
            .match_indices("</")
            .map(|(index, _)| index)
            .find(|&index| {
                let after = &rest.as_bytes()[index + 2..];
                after.len() >= name.len()
                    && after[..name.len()].eq_ignore_ascii_case(name.as_bytes())
                    && after
                        .get(name.len())
                        .is_none_or(|byte| !byte.is_ascii_alphanumeric())
            });
        self.position += end_tag.unwrap_or(rest.len());
    }
}

impl Iterator for NestingScanner<'_> {
    type Item = NestingChange;

    fn next(&mut self) -> Option<NestingChange> {
        loop {
            self.position += self.rest().find('<')?;
            let Some(tag) = self.read_markup() else {
                continue;
            };
            let Some(change) = tag.nesting_change(self.open_foreign_roots > 0) else {
                continue;
            };
            if tag.is_one_of(&FOREIGN_ROOTS) {
                self.open_foreign_roots = match change {
                    NestingChange::Open => self.open_foreign_roots + 1,
                    NestingChange::Close => self.open_foreign_roots.saturating_sub(1),
                };
            }
            if change == NestingChange::Open && tag.is_one_of(&RAW_TEXT_ELEMENTS) {
                self.skip_raw_text(tag.name);
            }
            return Some(change);
        }
    }
}

/// The attribute part of a tag, from after its name up to and including the closing `>`.
struct AttributeSpan {
    consumed: usize,
    is_self_closing: bool,
}

impl AttributeSpan {
    /// Quotes delimit a value only where a value starts, after `=`, matching the HTML
    /// tokenizer's attribute value states; a stray apostrophe elsewhere is ordinary text.
    fn read(text: &str) -> Self {
        let mut quote = None;
        let mut last_unquoted = None;
        for (index, byte) in text.bytes().enumerate() {
            match quote {
                Some(open) if byte == open => {
                    quote = None;
                    last_unquoted = Some(byte);
                }
                Some(_) => {}
                None if byte == b'>' => {
                    return Self {
                        consumed: index + 1,
                        is_self_closing: last_unquoted == Some(b'/'),
                    };
                }
                None if matches!(byte, b'"' | b'\'') && last_unquoted == Some(b'=') => {
                    quote = Some(byte);
                }
                None if !byte.is_ascii_whitespace() => last_unquoted = Some(byte),
                None => {}
            }
        }
        Self {
            consumed: text.len(),
            is_self_closing: false,
        }
    }
}
