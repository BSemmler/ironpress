//! CSS selectors: parsed once into a compiled form, then matched many times.
//!
//! Stylesheet rules hold a [`Selector`], so the cascade never re-tokenizes
//! selector text per element. Specificity is computed once at the same time.

mod ast;
mod attributes;
mod element;
mod matching;
mod parse;
mod specificity;

use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;

use ast::SelectorList;
use element::SubjectElement;

#[cfg(test)]
use super::AncestorInfo;
use super::SelectorContext;
#[cfg(test)]
use crate::parser::dom::ElementNode;

#[cfg(test)]
pub(crate) use parse::rfind_descendant_space;
pub use specificity::specificity;

/// A selector together with its compiled form and specificity.
///
/// Dereferences to the selector text, which other rule kinds (`@counter-style`,
/// generated font-face requests) use as a plain key.
#[derive(Debug, Clone)]
pub struct Selector {
    text: String,
    compiled: SelectorList,
    specificity: u32,
}

impl Selector {
    pub fn parse(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            compiled: SelectorList::parse(&text),
            specificity: specificity(&text),
            text,
        }
    }

    /// CSS Selectors-4 §17 specificity, packed as by [`specificity`].
    pub fn specificity(&self) -> u32 {
        self.specificity
    }

    /// Whether this selector matches the described element.
    pub(crate) fn matches(
        &self,
        tag: &str,
        classes: &[&str],
        id: Option<&str>,
        attributes: &HashMap<String, String>,
        ctx: &SelectorContext,
    ) -> bool {
        let element = SubjectElement {
            local_name: tag,
            classes,
            id,
            attributes,
        };
        self.compiled.matches(&element, ctx)
    }
}

impl Deref for Selector {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl From<String> for Selector {
    fn from(text: String) -> Self {
        Self::parse(text)
    }
}

impl From<&str> for Selector {
    fn from(text: &str) -> Self {
        Self::parse(text)
    }
}

impl PartialEq<str> for Selector {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

impl PartialEq<&str> for Selector {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Check if a CSS selector matches a given element (backward-compatible, no context).
#[cfg(test)]
pub(crate) fn selector_matches(
    selector: &str,
    tag: &str,
    classes: &[&str],
    id: Option<&str>,
) -> bool {
    selector_matches_with_context(
        selector,
        tag,
        classes,
        id,
        &HashMap::new(),
        &SelectorContext::default(),
    )
}

/// Check if selector text matches a given element with full context.
///
/// Compiles `selector` on every call; rule matching uses [`Selector::matches`].
#[cfg(test)]
pub(crate) fn selector_matches_with_context(
    selector: &str,
    tag: &str,
    classes: &[&str],
    id: Option<&str>,
    attributes: &HashMap<String, String>,
    ctx: &SelectorContext,
) -> bool {
    Selector::parse(selector).matches(tag, classes, id, attributes, ctx)
}

#[cfg(test)]
pub(crate) fn ancestor_info(element: &ElementNode) -> AncestorInfo<'_> {
    AncestorInfo {
        element,
        child_index: 0,
        sibling_count: 1,
        preceding_siblings: &[],
        following_siblings: &[],
        is_empty: false,
    }
}
