//! Matching compiled selectors against an element and its selector context.

use std::collections::HashMap;

use super::ast::{
    Combinator, ComplexSelector, CompoundSelector, NthScope, NthSelector, PseudoClass, Relation,
    RelativeSelector, SelectorList, TextDirection,
};
use super::element::{SelectorElement, VirtualElement};
use crate::parser::css::model::SiblingSummary;
use crate::parser::css::{AncestorInfo, SelectorContext};
use crate::parser::dom::{ElementNode, HtmlTag};

impl SelectorList {
    pub(crate) fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        self.0.iter().any(|selector| selector.matches(element, ctx))
    }
}

impl ComplexSelector {
    fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        self.subject.matches(element, ctx)
            && self
                .relation
                .as_ref()
                .is_none_or(|relation| relation.matches(ctx))
    }

    /// Match against the implied `body`, whose parent is the implied `html`.
    fn matches_virtual_body(&self) -> bool {
        let html = ElementNode {
            tag: HtmlTag::Html,
            raw_tag_name: "html".to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
        };
        let ancestors = [AncestorInfo {
            element: &html,
            child_index: 0,
            sibling_count: 1,
            preceding_siblings: &[],
            following_siblings: &[],
            is_empty: false,
        }];
        let ctx = SelectorContext {
            ancestors: &ancestors,
            sibling_count: 1,
            ..SelectorContext::default()
        };
        self.matches(&VirtualElement("body"), &ctx)
    }

    fn matches_virtual_html(&self) -> bool {
        self.matches(&VirtualElement("html"), &SelectorContext::default())
    }
}

impl Relation {
    /// Whether the left-hand selector matches an element in the required
    /// position relative to the subject described by `ctx`.
    fn matches(&self, ctx: &SelectorContext) -> bool {
        let left = &self.left;
        match self.combinator {
            Combinator::GeneralSibling => ctx
                .preceding_siblings
                .iter()
                .enumerate()
                .any(|(index, sibling)| left.matches(sibling, &ctx.sibling_context(index))),
            Combinator::AdjacentSibling => ctx
                .preceding_siblings
                .iter()
                .enumerate()
                .next_back()
                .is_some_and(|(index, sibling)| left.matches(sibling, &ctx.sibling_context(index))),
            Combinator::Child => match ctx.ancestors.len().checked_sub(1) {
                Some(parent) => {
                    left.matches(ctx.ancestors[parent].element, &ctx.ancestor_context(parent))
                }
                None => left.matches_virtual_body(),
            },
            Combinator::Descendant => {
                (0..ctx.ancestors.len()).any(|index| {
                    left.matches(ctx.ancestors[index].element, &ctx.ancestor_context(index))
                }) || left.matches_virtual_body()
                    || left.matches_virtual_html()
            }
        }
    }
}

impl CompoundSelector {
    fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        self.local_name
            .as_deref()
            .is_none_or(|name| name == element.local_name())
            && self.ids.iter().all(|id| element.id() == Some(id.as_str()))
            && self.classes.iter().all(|class| element.has_class(class))
            && self
                .attributes
                .iter()
                .all(|attribute| attribute.matches(element))
            && self
                .pseudo_classes
                .iter()
                .all(|pseudo| pseudo.matches(element, ctx))
    }
}

impl PseudoClass {
    fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        match self {
            Self::Not(list) => !list.matches(element, ctx),
            Self::Is(list) => list.matches(element, ctx),
            Self::Has(relatives) => relatives
                .iter()
                .any(|relative| relative.matches(element, ctx)),
            Self::Dir(direction) => direction.matches(element, ctx),
            Self::Lang(ranges) => {
                let lang = inherited_attribute_value(element, ctx, &["lang", "xml:lang"])
                    .unwrap_or("en")
                    .to_ascii_lowercase();
                ranges.iter().any(|range| {
                    range == "*"
                        || lang == *range
                        || lang
                            .strip_prefix(range.as_str())
                            .is_some_and(|suffix| suffix.starts_with('-'))
                })
            }
            Self::Nth(nth) => nth.matches(element, ctx),
            Self::FirstChild => ctx.child_index == 0,
            Self::LastChild => ctx.child_index + 1 == ctx.sibling_count,
            Self::OnlyChild => ctx.sibling_count == 1,
            Self::Empty => ctx.is_empty,
            Self::Root => {
                ctx.ancestors.is_empty() && element.local_name().eq_ignore_ascii_case("html")
            }
            Self::Scope => ctx.ancestors.is_empty(),
            Self::Defined => element.local_name() != "unknown",
            Self::FirstOfType => same_type_count(element, ctx.preceding_siblings) == 0,
            Self::LastOfType => same_type_count(element, ctx.following_siblings) == 0,
            Self::OnlyOfType => {
                same_type_count(element, ctx.preceding_siblings) == 0
                    && same_type_count(element, ctx.following_siblings) == 0
            }
        }
    }
}

impl TextDirection {
    fn matches(self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        let dir = inherited_attribute_value(element, ctx, &["dir"]).map(str::to_ascii_lowercase);
        match dir.as_deref() {
            Some("ltr") => self == Self::Ltr,
            Some("rtl") => self == Self::Rtl,
            // `auto` needs text-direction analysis, which is outside this matcher.
            Some("auto") => false,
            // HTML's default directionality is left-to-right.
            _ => self == Self::Ltr,
        }
    }
}

impl NthSelector {
    fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        let (before, siblings) = if self.from_end {
            (
                ctx.sibling_count.saturating_sub(ctx.child_index + 1),
                ctx.following_siblings,
            )
        } else {
            (ctx.child_index, ctx.preceding_siblings)
        };
        let index = match &self.scope {
            NthScope::AllChildren => before,
            NthScope::SameType => same_type_count(element, siblings),
            NthScope::Matching(list) => {
                if !list.matches(element, ctx) {
                    return false;
                }
                siblings
                    .iter()
                    .filter(|sibling| list.matches(*sibling, &SelectorContext::default()))
                    .count()
            }
        };
        self.formula.matches(index)
    }
}

impl RelativeSelector {
    fn matches(&self, element: &impl SelectorElement, ctx: &SelectorContext) -> bool {
        let detached = SelectorContext::default();
        match self {
            Self::NextSibling(target) => ctx
                .following_siblings
                .first()
                .is_some_and(|sibling| target.matches(sibling, &detached)),
            Self::LaterSibling(target) => ctx
                .following_siblings
                .iter()
                .any(|sibling| target.matches(sibling, &detached)),
            Self::ChildWithClasses(classes) => {
                has_summarized_classes(element, "__ironpress_has_child_classes", classes)
            }
            Self::DescendantWithClasses(classes) => {
                has_summarized_classes(element, "__ironpress_has_descendant_classes", classes)
            }
        }
    }
}

impl<'a> SelectorContext<'a> {
    /// The context of the preceding sibling at `sibling_index`. Its following
    /// siblings are approximated by the anchor's preceding siblings after it;
    /// forward siblings past the anchor are not needed for sibling-combinator
    /// left-hand matching.
    fn sibling_context(&self, sibling_index: usize) -> Self {
        Self {
            child_index: sibling_index,
            preceding_siblings: &self.preceding_siblings[..sibling_index],
            following_siblings: &self.preceding_siblings[sibling_index + 1..],
            is_empty: false,
            ..*self
        }
    }

    /// The context of the ancestor at `ancestor_index`.
    fn ancestor_context(&self, ancestor_index: usize) -> Self {
        let ancestor = &self.ancestors[ancestor_index];
        Self {
            ancestors: &self.ancestors[..ancestor_index],
            child_index: ancestor.child_index,
            sibling_count: ancestor.sibling_count,
            preceding_siblings: ancestor.preceding_siblings,
            following_siblings: ancestor.following_siblings,
            is_empty: ancestor.is_empty,
        }
    }
}

/// Siblings with the element's type, for the `*-of-type` pseudo-classes.
fn same_type_count(element: &impl SelectorElement, siblings: &[SiblingSummary]) -> usize {
    siblings
        .iter()
        .filter(|(tag, _)| tag.eq_ignore_ascii_case(element.local_name()))
        .count()
}

/// Whether the layout-supplied class summary under `key` holds every class.
fn has_summarized_classes(element: &impl SelectorElement, key: &str, classes: &[String]) -> bool {
    element
        .attributes()
        .and_then(|attributes| attributes.get(key))
        .is_some_and(|summary| {
            classes
                .iter()
                .all(|class| summary.split_whitespace().any(|present| present == class))
        })
}

/// The first of `names` set on the element or its nearest ancestor.
fn inherited_attribute_value<'a>(
    element: &'a impl SelectorElement,
    ctx: &SelectorContext<'a>,
    names: &[&str],
) -> Option<&'a str> {
    find_attribute_value(element.attributes(), names).or_else(|| {
        ctx.ancestors
            .iter()
            .rev()
            .find_map(|ancestor| find_attribute_value(Some(&ancestor.element.attributes), names))
    })
}

fn find_attribute_value<'a>(
    attributes: Option<&'a HashMap<String, String>>,
    names: &[&str],
) -> Option<&'a str> {
    attributes?.iter().find_map(|(name, value)| {
        names
            .iter()
            .any(|want| name.eq_ignore_ascii_case(want))
            .then_some(value.as_str())
    })
}
