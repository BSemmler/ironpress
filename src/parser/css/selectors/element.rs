//! What selector matching needs to know about one element.
//!
//! The subject element, its DOM ancestors, its sibling summaries and the
//! virtual document ancestors all expose different amounts of information.
//! Matching is generic over this trait, so no representation is converted
//! (and no class list is allocated) while walking combinators.

use std::collections::HashMap;

use crate::parser::css::model::SiblingSummary;
use crate::parser::dom::ElementNode;

pub(crate) trait SelectorElement {
    /// The tag name type selectors compare against.
    fn local_name(&self) -> &str;
    fn has_class(&self, class: &str) -> bool;
    fn id(&self) -> Option<&str>;
    /// Attributes, when this representation carries them.
    fn attributes(&self) -> Option<&HashMap<String, String>>;
}

/// The element whose style is being computed, as callers describe it.
pub(crate) struct SubjectElement<'a> {
    pub(crate) local_name: &'a str,
    pub(crate) classes: &'a [&'a str],
    pub(crate) id: Option<&'a str>,
    pub(crate) attributes: &'a HashMap<String, String>,
}

impl SelectorElement for SubjectElement<'_> {
    fn local_name(&self) -> &str {
        self.local_name
    }

    fn has_class(&self, class: &str) -> bool {
        self.classes.contains(&class)
    }

    fn id(&self) -> Option<&str> {
        self.id
    }

    fn attributes(&self) -> Option<&HashMap<String, String>> {
        Some(self.attributes)
    }
}

/// A DOM ancestor reached through a child or descendant combinator.
impl SelectorElement for ElementNode {
    fn local_name(&self) -> &str {
        &self.raw_tag_name
    }

    fn has_class(&self, class: &str) -> bool {
        self.attributes
            .get("class")
            .is_some_and(|classes| classes.split_whitespace().any(|present| present == class))
    }

    fn id(&self) -> Option<&str> {
        ElementNode::id(self)
    }

    fn attributes(&self) -> Option<&HashMap<String, String>> {
        Some(&self.attributes)
    }
}

/// A sibling reached through a sibling combinator or counted by a structural
/// pseudo-class. Only its tag name and classes are tracked.
impl SelectorElement for SiblingSummary {
    fn local_name(&self) -> &str {
        &self.0
    }

    fn has_class(&self, class: &str) -> bool {
        self.1.iter().any(|present| present == class)
    }

    fn id(&self) -> Option<&str> {
        None
    }

    fn attributes(&self) -> Option<&HashMap<String, String>> {
        None
    }
}

/// The implied `html` element above a fragment without a document root.
pub(crate) struct VirtualElement(pub(crate) &'static str);

impl SelectorElement for VirtualElement {
    fn local_name(&self) -> &str {
        self.0
    }

    fn has_class(&self, _class: &str) -> bool {
        false
    }

    fn id(&self) -> Option<&str> {
        None
    }

    fn attributes(&self) -> Option<&HashMap<String, String>> {
        None
    }
}
