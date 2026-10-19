//! Authored declarations that layout reads directly from the cascade.
//!
//! Some properties (overflow, insets, `display: contents`, `string-set`, ...)
//! are consulted by layout as authored values rather than through
//! [`ComputedStyle`](crate::style::computed::ComputedStyle). An
//! [`AuthoredCascade`] selects one element's matching rules once, so each
//! property lookup only orders that element's own declarations instead of
//! re-matching the whole stylesheet.

mod insets;
mod overflow;

pub(crate) use overflow::{LayoutOverflowAxes, LayoutOverflowKeyword};

use crate::parser::css::{CssRule, CssValue, PseudoElement, SelectorContext, StyleMap};
use crate::parser::dom::ElementNode;
use crate::style::computed::IntrinsicWidthKeyword;

use super::helpers::selector_attributes_with_has;

/// One element's (or pseudo-element's) matching rules and inline style.
pub(crate) struct AuthoredCascade<'r> {
    /// Matching rules in source order, with their source index.
    matched: Vec<(usize, &'r CssRule)>,
    inline: Option<StyleMap>,
}

/// Cascade precedence of one candidate declaration, compared field by field:
/// importance, then origin (inline style above rules), specificity and source
/// order. Later equal candidates win.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Precedence {
    important: bool,
    inline: bool,
    specificity: u32,
    source_order: usize,
}

impl<'r> AuthoredCascade<'r> {
    /// Rules targeting `el` itself, plus its `style` attribute.
    pub(crate) fn new(
        el: &ElementNode,
        rules: &'r [CssRule],
        selector_ctx: &SelectorContext,
    ) -> Self {
        Self {
            matched: Self::matching(el, rules, selector_ctx, None),
            inline: el.style_attr().map(crate::parser::css::parse_inline_style),
        }
    }

    /// Rules targeting `pseudo` on `el`.
    pub(crate) fn pseudo(
        el: &ElementNode,
        rules: &'r [CssRule],
        selector_ctx: &SelectorContext,
        pseudo: PseudoElement,
    ) -> Self {
        Self {
            matched: Self::matching(el, rules, selector_ctx, Some(pseudo)),
            inline: None,
        }
    }

    fn matching(
        el: &ElementNode,
        rules: &'r [CssRule],
        selector_ctx: &SelectorContext,
        pseudo: Option<PseudoElement>,
    ) -> Vec<(usize, &'r CssRule)> {
        let classes = el.class_list();
        let attributes = selector_attributes_with_has(el);
        let (tag, id) = (el.tag_name(), el.id());
        rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| rule.pseudo_element == pseudo)
            .filter(|(_, rule)| {
                rule.selector
                    .matches(tag, &classes, id, &attributes, selector_ctx)
            })
            .collect()
    }

    /// The winning authored value of `property`.
    pub(crate) fn value(&self, property: &str) -> Option<CssValue> {
        let from_rules = self.matched.iter().filter_map(|(source_order, rule)| {
            let value = rule.declarations.get(property)?;
            Some((
                Precedence {
                    important: rule.declarations.is_important(property),
                    inline: false,
                    specificity: rule.selector.specificity(),
                    source_order: *source_order,
                },
                value,
            ))
        });
        let from_inline = self.inline.iter().filter_map(|inline| {
            let value = inline.get(property)?;
            Some((
                Precedence {
                    important: inline.is_important(property),
                    inline: true,
                    specificity: u32::MAX,
                    source_order: usize::MAX,
                },
                value,
            ))
        });
        from_rules
            .chain(from_inline)
            .max_by_key(|(precedence, _)| *precedence)
            .map(|(_, value)| value.clone())
    }

    /// The winning authored value of `property` when it is a keyword,
    /// lowercased.
    pub(crate) fn keyword(&self, property: &str) -> Option<String> {
        match self.value(property)? {
            CssValue::Keyword(value) => Some(value.to_ascii_lowercase()),
            _ => None,
        }
    }

    pub(crate) fn intrinsic_width_keyword(&self, property: &str) -> Option<IntrinsicWidthKeyword> {
        match self.keyword(property)?.as_str() {
            "min-content" => Some(IntrinsicWidthKeyword::MinContent),
            "max-content" => Some(IntrinsicWidthKeyword::MaxContent),
            "fit-content" => Some(IntrinsicWidthKeyword::FitContent),
            _ => None,
        }
    }

    pub(crate) fn display_contents(&self) -> bool {
        self.keyword("display")
            .is_some_and(|value| value == "contents")
    }

    /// Whether the generated box is a list item (`display: ... list-item`).
    pub(crate) fn display_list_item(&self) -> bool {
        self.keyword("display")
            .is_some_and(|value| value.split_whitespace().any(|part| part == "list-item"))
    }
}
