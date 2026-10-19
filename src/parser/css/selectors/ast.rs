//! Compiled selector representation.
//!
//! A selector string is parsed once into these types when its rule is
//! created. Matching walks the tree without re-tokenizing. Every compound in
//! a compiled selector can match some element: components that can never
//! match (unknown or dynamic pseudo-classes, malformed attribute operators,
//! invalid `An+B` formulas) make their whole complex selector unmatchable, so
//! parsing drops it from the enclosing list.

use super::attributes::AttributeSelector;

/// Comma-separated alternatives; matches when any alternative matches.
#[derive(Debug, Clone, Default)]
pub(crate) struct SelectorList(pub(super) Vec<ComplexSelector>);

/// A complex selector stored subject-first: the rightmost compound plus the
/// relation to the rest of the selector toward the root.
#[derive(Debug, Clone)]
pub(crate) struct ComplexSelector {
    pub(super) subject: CompoundSelector,
    pub(super) relation: Option<Relation>,
}

/// How the subject relates to the remaining (left-hand) complex selector.
#[derive(Debug, Clone)]
pub(crate) struct Relation {
    pub(super) combinator: Combinator,
    pub(super) left: Box<ComplexSelector>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Combinator {
    GeneralSibling,
    AdjacentSibling,
    Child,
    Descendant,
}

/// One compound selector: every component must match the same element.
#[derive(Debug, Clone, Default)]
pub(crate) struct CompoundSelector {
    /// `None` for the universal selector or an omitted type.
    pub(super) local_name: Option<String>,
    pub(super) ids: Vec<String>,
    pub(super) classes: Vec<String>,
    pub(super) attributes: Vec<AttributeSelector>,
    pub(super) pseudo_classes: Vec<PseudoClass>,
}

/// A pseudo-class that can match in a static print context.
#[derive(Debug, Clone)]
pub(crate) enum PseudoClass {
    Not(SelectorList),
    /// `:is()` and `:where()`; they differ only in specificity.
    Is(SelectorList),
    Has(Vec<RelativeSelector>),
    Dir(TextDirection),
    /// Lowercased, unquoted language ranges.
    Lang(Vec<String>),
    Nth(NthSelector),
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    Scope,
    Defined,
    FirstOfType,
    LastOfType,
    OnlyOfType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextDirection {
    Ltr,
    Rtl,
}

/// One `:has()` argument.
#[derive(Debug, Clone)]
pub(crate) enum RelativeSelector {
    NextSibling(ComplexSelector),
    LaterSibling(ComplexSelector),
    /// Matched against the layout-supplied child class summary.
    ChildWithClasses(Vec<String>),
    /// Matched against the layout-supplied descendant class summary.
    DescendantWithClasses(Vec<String>),
}

/// `:nth-child()`, `:nth-last-child()`, `:nth-of-type()` and
/// `:nth-last-of-type()`.
#[derive(Debug, Clone)]
pub(crate) struct NthSelector {
    pub(super) formula: NthFormula,
    pub(super) from_end: bool,
    pub(super) scope: NthScope,
}

/// Which siblings an `An+B` index counts.
#[derive(Debug, Clone)]
pub(crate) enum NthScope {
    AllChildren,
    SameType,
    /// `:nth-child(An+B of S)`: only siblings matching `S`.
    Matching(SelectorList),
}

/// A parsed `An+B` formula (`odd` is `2n+1`, `even` is `2n`, `B` is `0n+B`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NthFormula {
    pub(super) a: i64,
    pub(super) b: i64,
}

impl NthFormula {
    /// Whether the element at zero-based `index` is selected.
    pub(super) fn matches(self, index: usize) -> bool {
        let n = index as i64 + 1;
        if self.a == 0 {
            return n == self.b;
        }
        let diff = n - self.b;
        if self.a > 0 {
            diff >= 0 && diff % self.a == 0
        } else {
            diff <= 0 && diff % self.a == 0
        }
    }
}
