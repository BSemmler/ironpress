//! Selector text to compiled [`SelectorList`].
//!
//! Tokenizing reuses the splitting rules the string matcher applied on every
//! match, so a compiled selector keeps exactly the meaning its text had.

use super::ast::{
    Combinator, ComplexSelector, CompoundSelector, NthFormula, NthScope, NthSelector, PseudoClass,
    Relation, RelativeSelector, SelectorList, TextDirection,
};
use super::attributes::{AttributeSelector, unquote};

impl SelectorList {
    pub(crate) fn parse(list: &str) -> Self {
        Self(
            split_selector_list(list)
                .into_iter()
                .filter_map(ComplexSelector::parse)
                .collect(),
        )
    }

    fn parse_nonempty(list: &str) -> Option<Self> {
        let list = Self::parse(list);
        (!list.0.is_empty()).then_some(list)
    }
}

impl ComplexSelector {
    /// Parse one complex selector, or `None` when it can never match.
    fn parse(selector: &str) -> Option<Self> {
        let Some((combinator, left, right)) = split_rightmost_combinator(selector) else {
            return Some(Self {
                subject: CompoundSelector::parse(selector)?,
                relation: None,
            });
        };
        Some(Self {
            subject: CompoundSelector::parse(right)?,
            relation: Some(Relation {
                combinator,
                left: Box::new(Self::parse(left)?),
            }),
        })
    }
}

impl CompoundSelector {
    fn parse(selector: &str) -> Option<Self> {
        let selector = selector.trim();
        if selector.is_empty() {
            return None;
        }
        let (base, pseudos) = split_compound(selector);
        let pseudo_classes = pseudos
            .into_iter()
            .map(PseudoClass::parse)
            .collect::<Option<Vec<_>>>()?;
        // Type, class and id components precede the first attribute group.
        let (head, attributes) = match base.find('[') {
            Some(bracket_index) => {
                let (head, attributes) = base.split_at(bracket_index);
                (head, AttributeSelector::parse_all(attributes)?)
            }
            None => (base, Vec::new()),
        };
        Some(Self {
            attributes,
            pseudo_classes,
            ..Self::parse_head(head)
        })
    }

    /// An optional type/universal part followed by `.class` and `#id`
    /// segments, e.g. `.card.alt` or `div.box#hero`.
    fn parse_head(head: &str) -> Self {
        let (local_name, segments) = head.split_at(head.find(['.', '#']).unwrap_or(head.len()));
        let mut compound = Self {
            local_name: (!local_name.is_empty() && local_name != "*")
                .then(|| local_name.to_string()),
            ..Self::default()
        };
        let mut chars = segments.char_indices().peekable();
        while let Some((start, marker)) = chars.next() {
            let mut end = segments.len();
            while let Some(&(index, ch)) = chars.peek() {
                if ch == '.' || ch == '#' {
                    end = index;
                    break;
                }
                chars.next();
            }
            let name = segments[start + 1..end].to_string();
            if marker == '#' {
                compound.ids.push(name);
            } else {
                compound.classes.push(name);
            }
        }
        compound
    }
}

impl PseudoClass {
    /// Parse one pseudo-class token (with its `:`), or `None` when it never
    /// matches in a static print context.
    fn parse(pseudo: &str) -> Option<Self> {
        if let Some(arg) = functional_arg(pseudo, ":not(") {
            return Some(Self::Not(SelectorList::parse(arg)));
        }
        if let Some(arg) =
            functional_arg(pseudo, ":is(").or_else(|| functional_arg(pseudo, ":where("))
        {
            return SelectorList::parse_nonempty(arg).map(Self::Is);
        }
        if let Some(arg) = functional_arg(pseudo, ":has(") {
            let relatives: Vec<_> = split_selector_list(arg)
                .into_iter()
                .filter_map(RelativeSelector::parse)
                .collect();
            return (!relatives.is_empty()).then_some(Self::Has(relatives));
        }
        if let Some(arg) = functional_arg(pseudo, ":dir(") {
            return match unquote(arg.trim()).to_ascii_lowercase().as_str() {
                "ltr" => Some(Self::Dir(TextDirection::Ltr)),
                "rtl" => Some(Self::Dir(TextDirection::Rtl)),
                _ => None,
            };
        }
        if let Some(arg) = functional_arg(pseudo, ":lang(") {
            let ranges: Vec<_> = split_selector_list(arg)
                .into_iter()
                .map(|range| unquote(range.trim()).to_ascii_lowercase())
                .collect();
            return (!ranges.is_empty()).then_some(Self::Lang(ranges));
        }
        if let Some(nth) = NthSelector::parse(pseudo) {
            return nth.map(Self::Nth);
        }
        Some(match pseudo {
            ":first-child" => Self::FirstChild,
            ":last-child" => Self::LastChild,
            ":only-child" => Self::OnlyChild,
            ":empty" => Self::Empty,
            ":root" => Self::Root,
            // :scope without an explicit scoping root matches the document root in
            // print contexts (no :scope attribute is set), mirroring :root.
            ":scope" => Self::Scope,
            // HTML built-in elements are always defined. Unknown/custom elements
            // would require a custom-element registry, which this static renderer
            // does not model.
            ":defined" => Self::Defined,
            ":first-of-type" => Self::FirstOfType,
            ":last-of-type" => Self::LastOfType,
            ":only-of-type" => Self::OnlyOfType,
            // Dynamic / UI pseudo-classes never match in a static print context.
            _ => return None,
        })
    }
}

impl NthSelector {
    /// `None` when `pseudo` is not an `:nth-*` pseudo-class; `Some(None)` when
    /// it is one that can never match.
    fn parse(pseudo: &str) -> Option<Option<Self>> {
        let child_of = |arg: &str, from_end| match split_nth_of(arg) {
            Some((formula, selector_list)) => Some(Self {
                formula: NthFormula::parse(formula)?,
                from_end,
                scope: NthScope::Matching(SelectorList::parse_nonempty(selector_list)?),
            }),
            None => Self::new(arg, from_end, NthScope::AllChildren),
        };
        if let Some(arg) = functional_arg(pseudo, ":nth-child(") {
            return Some(child_of(arg, false));
        }
        if let Some(arg) = functional_arg(pseudo, ":nth-last-child(") {
            return Some(child_of(arg, true));
        }
        if let Some(arg) = functional_arg(pseudo, ":nth-of-type(") {
            return Some(Self::new(arg, false, NthScope::SameType));
        }
        if let Some(arg) = functional_arg(pseudo, ":nth-last-of-type(") {
            return Some(Self::new(arg, true, NthScope::SameType));
        }
        None
    }

    fn new(formula: &str, from_end: bool, scope: NthScope) -> Option<Self> {
        Some(Self {
            formula: NthFormula::parse(formula)?,
            from_end,
            scope,
        })
    }
}

impl NthFormula {
    fn parse(arg: &str) -> Option<Self> {
        let normalized = arg
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        match normalized.as_str() {
            "odd" => return Some(Self { a: 2, b: 1 }),
            "even" => return Some(Self { a: 2, b: 0 }),
            _ => {}
        }
        if let Ok(b) = normalized.parse::<i64>() {
            return Some(Self { a: 0, b });
        }
        let (a, b) = parse_an_plus_b(&normalized)?;
        Some(Self { a, b })
    }
}

impl RelativeSelector {
    /// Evaluate `:has()` arguments against the element's known context.
    /// Sibling forms use the sibling lists in `SelectorContext`; child and
    /// descendant forms support class selectors against synthetic class
    /// summaries supplied by layout.
    fn parse(relative: &str) -> Option<Self> {
        let arg = relative.trim();
        if let Some(rest) = arg.strip_prefix('+') {
            return ComplexSelector::parse(rest.trim()).map(Self::NextSibling);
        }
        if let Some(rest) = arg.strip_prefix('~') {
            return ComplexSelector::parse(rest.trim()).map(Self::LaterSibling);
        }
        if let Some(rest) = arg.strip_prefix('>') {
            return required_classes(rest.trim()).map(Self::ChildWithClasses);
        }
        required_classes(arg).map(Self::DescendantWithClasses)
    }
}

pub(crate) fn rfind_descendant_space(selector: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = selector.char_indices().collect();
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;

    for index in (0..chars.len()).rev() {
        let (byte_index, ch) = chars[index];
        match ch {
            ']' => bracket_depth += 1,
            '[' => bracket_depth = bracket_depth.saturating_sub(1),
            ')' => paren_depth += 1,
            '(' => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }

        if ch != ' ' || bracket_depth != 0 || paren_depth != 0 {
            continue;
        }

        let prev = index
            .checked_sub(1)
            .and_then(|prev_index| chars.get(prev_index))
            .map(|(_, ch)| *ch);
        let next = chars.get(index + 1).map(|(_, ch)| *ch);
        if matches!(prev, Some('>' | '+' | '~')) || matches!(next, Some('>' | '+' | '~')) {
            continue;
        }

        return Some(byte_index);
    }

    None
}

fn split_rightmost_combinator(selector: &str) -> Option<(Combinator, &str, &str)> {
    let mut candidate = rfind_descendant_space(selector).and_then(|byte_index| {
        let left = selector.get(..byte_index)?.trim();
        let right = selector.get(byte_index + ' '.len_utf8()..)?.trim();
        Some((byte_index, Combinator::Descendant, left, right))
    });

    for (combinator, combinator_char) in [
        (Combinator::GeneralSibling, '~'),
        (Combinator::AdjacentSibling, '+'),
        (Combinator::Child, '>'),
    ] {
        if let Some((byte_index, left, right)) = split_on_combinator(selector, combinator_char) {
            match candidate {
                Some((current_index, _, _, _)) if current_index > byte_index => {}
                _ => candidate = Some((byte_index, combinator, left, right)),
            }
        }
    }

    candidate.map(|(_, combinator, left, right)| (combinator, left, right))
}

fn split_on_combinator(selector: &str, combinator: char) -> Option<(usize, &str, &str)> {
    let chars: Vec<(usize, char)> = selector.char_indices().collect();
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;

    for index in (0..chars.len()).rev() {
        let (_, ch) = chars[index];
        match ch {
            ']' => bracket_depth += 1,
            '[' => bracket_depth = bracket_depth.saturating_sub(1),
            ')' => paren_depth += 1,
            '(' => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }

        if bracket_depth != 0 || paren_depth != 0 || ch != combinator {
            continue;
        }

        let byte_index = chars[index].0;
        let right_start = byte_index + combinator.len_utf8();
        let left = selector.get(..byte_index)?.trim();
        let right = selector.get(right_start..)?.trim();
        if !left.is_empty() && !right.is_empty() {
            return Some((byte_index, left, right));
        }
    }

    None
}

/// Split a compound selector into its non-pseudo head and the list of trailing
/// pseudo-class tokens. Each pseudo token retains its leading `:` and, for
/// functional pseudos, its parenthesised argument (e.g. `:nth-child(2n+1)`,
/// `:not(.a, .b)`). Brackets and parentheses are tracked so a `:` inside an
/// attribute value (`[x=":"]`) or a functional argument (`:not(:first-child)`)
/// does not terminate the head.
pub(super) fn split_compound(selector: &str) -> (&str, Vec<&str>) {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut head_end = selector.len();
    let mut pseudo_starts: Vec<usize> = Vec::new();

    for (byte_index, ch) in selector.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ':' if bracket_depth == 0 && paren_depth == 0 => {
                // A `::` introduces a pseudo-element, which is stripped earlier;
                // treat any `:` at top level as a pseudo-class boundary.
                if pseudo_starts.is_empty() {
                    head_end = byte_index;
                }
                pseudo_starts.push(byte_index);
            }
            _ => {}
        }
    }

    let head = &selector[..head_end];
    let mut pseudos = Vec::with_capacity(pseudo_starts.len());
    for (i, &start) in pseudo_starts.iter().enumerate() {
        let end = pseudo_starts.get(i + 1).copied().unwrap_or(selector.len());
        // Skip a leading `:` belonging to a `::pseudo-element` written with a
        // single colon split — only collect real pseudo-class tokens.
        let token = &selector[start..end];
        if token.starts_with("::") {
            continue;
        }
        pseudos.push(token);
    }
    (head, pseudos)
}

/// If `pseudo` is the functional pseudo named by `prefix` (e.g. `:not(`),
/// return its inner argument with the trailing `)` stripped.
pub(super) fn functional_arg<'a>(pseudo: &'a str, prefix: &str) -> Option<&'a str> {
    pseudo
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(')'))
}

/// Split a selector list on top-level commas (ignoring commas inside `[]`/`()`).
pub(super) fn split_selector_list(list: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in list.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if bracket_depth == 0 && paren_depth == 0 => {
                let piece = list[start..index].trim();
                if !piece.is_empty() {
                    parts.push(piece);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    let tail = list[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

pub(super) fn split_nth_of(arg: &str) -> Option<(&str, &str)> {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;

    for (index, ch) in arg.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            'o' | 'O' if bracket_depth == 0 && paren_depth == 0 => {
                let rest = &arg[index..];
                if !rest
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("of"))
                {
                    continue;
                }
                let before_is_space = arg[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
                let after_is_space = arg[index + 2..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace);
                if before_is_space && after_is_space {
                    let formula = arg[..index].trim();
                    let selector_list = arg[index + 2..].trim();
                    if !formula.is_empty() && !selector_list.is_empty() {
                        return Some((formula, selector_list));
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn required_classes(selector: &str) -> Option<Vec<String>> {
    let selector = selector.trim();
    if selector.is_empty() || selector.contains([' ', '>', '+', '~', '[', ':']) {
        return None;
    }
    let mut classes = Vec::new();
    for part in selector.split('.') {
        if part.is_empty() {
            continue;
        }
        if part.contains('#') {
            return None;
        }
        if classes.is_empty() && !selector.starts_with('.') {
            continue;
        }
        classes.push(part.to_string());
    }
    (!classes.is_empty()).then_some(classes)
}

fn parse_an_plus_b(s: &str) -> Option<(i64, i64)> {
    let n_index = s.find('n')?;
    let (a_part, b_part) = s.split_at(n_index);
    let a = match a_part.trim() {
        "" | "+" => 1,
        "-" => -1,
        value => value.parse::<i64>().ok()?,
    };
    let b = match b_part.strip_prefix('n')?.trim() {
        "" => 0,
        value => value.parse::<i64>().ok()?,
    };
    Some((a, b))
}
