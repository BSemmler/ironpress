//! Page-based counters inside running elements.
//!
//! A running element is laid out once, before pagination, but CSS GCPM 3 §2
//! displays it in a page-margin box where `counter(page)` and
//! `counter(pages)` take the values of the page that shows it. Layout
//! therefore records each page-based counter as a [`DeferredPageCounter`]
//! marker, and the painter substitutes [`PageCounters`] for every page.

use super::elements::{
    FlexRow, GridRow, LayoutElement, LayoutNode, LayoutVisitorMut, TableRow, TextBlock,
    visit_layout_tree_mut,
};
use super::engine::{TextLine, TextRun};
use super::list_markers::format_counter_value;
use crate::style::computed::{ListStyleType, TARGET_PLACEHOLDER_END, TARGET_PLACEHOLDER_START};
use std::borrow::Cow;

/// Marker payload prefix; line measurement sizes these markers like page numbers.
pub(crate) const PAGE_COUNTER_PAYLOAD_PREFIX: &str = "page-counter|";

/// The two page-based counters CSS Paged Media 3 §4.3 defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageCounterKind {
    /// `counter(page)`: the page that displays the content.
    Page,
    /// `counter(pages)`: the document page count.
    Pages,
}

impl PageCounterKind {
    /// The page-based counter a `counter()` name refers to, if any.
    pub(crate) fn from_counter_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("page") {
            Some(Self::Page)
        } else if name.eq_ignore_ascii_case("pages") {
            Some(Self::Pages)
        } else {
            None
        }
    }

    const fn code(self) -> char {
        match self {
            Self::Page => '0',
            Self::Pages => '1',
        }
    }

    const fn from_code(code: char) -> Option<Self> {
        match code {
            '0' => Some(Self::Page),
            '1' => Some(Self::Pages),
            _ => None,
        }
    }
}

/// Counter styles a deferred page counter can carry through its text marker.
///
/// Custom `@counter-style` rules render as decimal, matching the counter
/// styles `@page` margin boxes support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageCounterFormat {
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    CjkDecimal,
}

impl PageCounterFormat {
    const ALL: [Self; 7] = [
        Self::Decimal,
        Self::DecimalLeadingZero,
        Self::LowerAlpha,
        Self::UpperAlpha,
        Self::LowerRoman,
        Self::UpperRoman,
        Self::CjkDecimal,
    ];

    fn from_style(style: &ListStyleType) -> Self {
        match style {
            ListStyleType::DecimalLeadingZero => Self::DecimalLeadingZero,
            ListStyleType::LowerAlpha => Self::LowerAlpha,
            ListStyleType::UpperAlpha => Self::UpperAlpha,
            ListStyleType::LowerRoman => Self::LowerRoman,
            ListStyleType::UpperRoman => Self::UpperRoman,
            ListStyleType::CjkDecimal => Self::CjkDecimal,
            _ => Self::Decimal,
        }
    }

    const fn style(self) -> ListStyleType {
        match self {
            Self::Decimal => ListStyleType::Decimal,
            Self::DecimalLeadingZero => ListStyleType::DecimalLeadingZero,
            Self::LowerAlpha => ListStyleType::LowerAlpha,
            Self::UpperAlpha => ListStyleType::UpperAlpha,
            Self::LowerRoman => ListStyleType::LowerRoman,
            Self::UpperRoman => ListStyleType::UpperRoman,
            Self::CjkDecimal => ListStyleType::CjkDecimal,
        }
    }

    fn code(self) -> char {
        let index = Self::ALL.iter().position(|format| *format == self);
        index
            .and_then(|index| char::from_digit(index as u32, 10))
            .unwrap_or('0')
    }

    fn from_code(code: char) -> Option<Self> {
        let index = code.to_digit(10)?;
        Self::ALL.get(index as usize).copied()
    }
}

/// A page-based counter whose value is known only once a page displays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeferredPageCounter {
    kind: PageCounterKind,
    format: PageCounterFormat,
}

impl DeferredPageCounter {
    pub(crate) fn new(kind: PageCounterKind, style: &ListStyleType) -> Self {
        Self {
            kind,
            format: PageCounterFormat::from_style(style),
        }
    }

    /// The text marker that stands in for the counter until paint.
    ///
    /// The marker shares the cross-reference delimiters, so line breaking keeps
    /// it atomic and measures it as a one-digit placeholder.
    pub(crate) fn encode(self) -> String {
        format!(
            "{TARGET_PLACEHOLDER_START}{PAGE_COUNTER_PAYLOAD_PREFIX}{}{}{TARGET_PLACEHOLDER_END}",
            self.kind.code(),
            self.format.code()
        )
    }

    /// `text` with its deferred page-counter markers removed, for glyph
    /// collection that runs before a displaying page is known.
    pub(crate) fn strip_markers(text: &str) -> Cow<'_, str> {
        DeferredMarkers::rewrite(text, |_| String::new())
    }

    /// Parse a marker payload (the text between the delimiters).
    fn from_payload(payload: &str) -> Option<Self> {
        let mut codes = payload.strip_prefix(PAGE_COUNTER_PAYLOAD_PREFIX)?.chars();
        let kind = PageCounterKind::from_code(codes.next()?)?;
        let format = PageCounterFormat::from_code(codes.next()?)?;
        codes.next().is_none().then_some(Self { kind, format })
    }
}

/// Page-based counter values for one displayed page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageCounters {
    pub(crate) page: usize,
    pub(crate) pages: usize,
}

impl PageCounters {
    fn format(self, counter: DeferredPageCounter) -> String {
        let value = match counter.kind {
            PageCounterKind::Page => self.page,
            PageCounterKind::Pages => self.pages,
        };
        let value = i32::try_from(value).unwrap_or(i32::MAX);
        format_counter_value(&counter.format.style(), value)
    }

    /// Replace every deferred page-counter marker in `text`.
    pub(crate) fn resolve_text(self, text: &str) -> Cow<'_, str> {
        DeferredMarkers::rewrite(text, |counter| self.format(counter))
    }

    /// A copy of a running element with its page-based counters resolved.
    pub(crate) fn resolve_element(self, element: &dyn LayoutElement) -> LayoutNode {
        self.resolve(element).element
    }

    /// The text runs whose page-based counters this page resolves, so font
    /// subsetting can register exactly the glyphs the page paints.
    pub(crate) fn resolved_runs(self, element: &dyn LayoutElement) -> Vec<TextRun> {
        self.resolve(element).resolved_runs
    }

    fn resolve(self, element: &dyn LayoutElement) -> ResolvedRunningElement {
        let mut element = element.clone_box();
        let mut resolver = PageCounterResolver {
            counters: self,
            resolved_runs: Vec::new(),
        };
        visit_layout_tree_mut(element.as_mut(), &mut resolver);
        ResolvedRunningElement {
            element,
            resolved_runs: resolver.resolved_runs,
        }
    }
}

struct ResolvedRunningElement {
    element: LayoutNode,
    resolved_runs: Vec<TextRun>,
}

/// Marker scanning shared by paint-time resolution and glyph collection.
struct DeferredMarkers;

impl DeferredMarkers {
    /// `text` with each page-counter marker replaced, borrowed when it holds
    /// none; other cross-reference markers are left for their own resolver.
    fn rewrite(text: &str, mut replace: impl FnMut(DeferredPageCounter) -> String) -> Cow<'_, str> {
        let mut out = String::new();
        let mut copied = 0;
        let mut rest = text;
        while let Some(start) = rest.find(TARGET_PLACEHOLDER_START) {
            let payload_start = start + TARGET_PLACEHOLDER_START.len();
            let Some(end_rel) = rest[payload_start..].find(TARGET_PLACEHOLDER_END) else {
                break;
            };
            let marker_end = payload_start + end_rel + TARGET_PLACEHOLDER_END.len();
            let consumed = text.len() - rest.len();
            if let Some(counter) =
                DeferredPageCounter::from_payload(&rest[payload_start..payload_start + end_rel])
            {
                out.push_str(&text[copied..consumed + start]);
                out.push_str(&replace(counter));
                copied = consumed + marker_end;
            }
            rest = &rest[marker_end..];
        }
        if copied == 0 {
            return Cow::Borrowed(text);
        }
        out.push_str(&text[copied..]);
        Cow::Owned(out)
    }
}

struct PageCounterResolver {
    counters: PageCounters,
    resolved_runs: Vec<TextRun>,
}

impl PageCounterResolver {
    fn resolve_lines(&mut self, lines: &mut [TextLine]) {
        for line in lines {
            for run in &mut line.runs {
                self.resolve_run(run);
            }
        }
    }

    fn resolve_run(&mut self, run: &mut TextRun) {
        if let Some(inline) = run.inline_box.as_deref_mut() {
            self.resolve_lines(&mut inline.lines);
        }
        if let Cow::Owned(text) = self.counters.resolve_text(&run.text) {
            run.text = text;
            self.resolved_runs.push(run.clone());
        }
    }
}

impl LayoutVisitorMut for PageCounterResolver {
    fn visit_text_block(&mut self, element: &mut TextBlock) {
        self.resolve_lines(&mut element.lines);
    }

    fn visit_table_row(&mut self, element: &mut TableRow) {
        for cell in &mut element.content.cells {
            self.resolve_lines(&mut cell.layout.content.lines);
        }
    }

    fn visit_grid_row(&mut self, element: &mut GridRow) {
        for cell in &mut element.content.cells {
            self.resolve_lines(&mut cell.layout.content.lines);
        }
    }

    fn visit_flex_row(&mut self, element: &mut FlexRow) {
        for cell in &mut element.content.cells {
            self.resolve_lines(&mut cell.lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THIRD_OF_TWELVE: PageCounters = PageCounters { page: 3, pages: 12 };

    fn marker(name: &str, style: ListStyleType) -> String {
        let kind = PageCounterKind::from_counter_name(name).expect("page-based counter");
        DeferredPageCounter::new(kind, &style).encode()
    }

    #[test]
    fn only_page_and_pages_are_page_based_counters() {
        assert_eq!(
            PageCounterKind::from_counter_name("PAGE"),
            Some(PageCounterKind::Page)
        );
        assert_eq!(
            PageCounterKind::from_counter_name("pages"),
            Some(PageCounterKind::Pages)
        );
        assert_eq!(PageCounterKind::from_counter_name("section"), None);
    }

    #[test]
    fn markers_resolve_to_the_displaying_page_amid_literal_text() {
        let text = format!(
            "Page {} of {}",
            marker("page", ListStyleType::Decimal),
            marker("pages", ListStyleType::Decimal)
        );
        assert_eq!(THIRD_OF_TWELVE.resolve_text(&text), "Page 3 of 12");
    }

    #[test]
    fn markers_keep_their_counter_style() {
        let text = marker("page", ListStyleType::UpperRoman);
        assert_eq!(THIRD_OF_TWELVE.resolve_text(&text), "III");
    }

    #[test]
    fn custom_counter_styles_render_as_decimal_like_margin_boxes() {
        let text = marker("pages", ListStyleType::Custom("thumbs".into()));
        assert_eq!(THIRD_OF_TWELVE.resolve_text(&text), "12");
    }

    #[test]
    fn foreign_and_unterminated_markers_are_left_intact() {
        let foreign =
            format!("{TARGET_PLACEHOLDER_START}counter|#intro|page{TARGET_PLACEHOLDER_END}");
        assert_eq!(THIRD_OF_TWELVE.resolve_text(&foreign), foreign);

        let unterminated = format!("x {TARGET_PLACEHOLDER_START}page-counter|00");
        assert_eq!(THIRD_OF_TWELVE.resolve_text(&unterminated), unterminated);
    }

    #[test]
    fn glyph_collection_text_drops_markers() {
        let text = format!("p. {}", marker("page", ListStyleType::Decimal));
        assert_eq!(DeferredPageCounter::strip_markers(&text), "p. ");
    }
}
