//! CSS Selectors-4 §17 specificity, computed from selector text once when a
//! selector is compiled.

use super::parse::{functional_arg, split_compound, split_nth_of, split_selector_list};

/// CSS Selectors-4 §17 specificity, packed as `(a << 20) | (b << 10) | c`
/// where `a` = id count, `b` = class/attr/pseudo-class count, `c` =
/// type/pseudo-element count. The packed form gives a single sortable integer
/// for the cascade (css-cascade-4 §6.3). For a selector list (commas) the
/// highest specificity among the alternatives is returned.
pub fn specificity(selector: &str) -> u32 {
    split_selector_list(selector)
        .into_iter()
        .map(|part| {
            let (a, b, c) = complex_specificity(part);
            (a.min(1023) << 20) | (b.min(1023) << 10) | c.min(1023)
        })
        .max()
        .unwrap_or(0)
}

/// Compute the raw (a, b, c) triple for a single complex selector.
fn complex_specificity(selector: &str) -> (u32, u32, u32) {
    let mut total = (0u32, 0u32, 0u32);
    // Walk each compound separated by combinators; combinators add nothing.
    for compound in split_into_compounds(selector) {
        let (a, b, c) = compound_specificity(compound);
        total.0 += a;
        total.1 += b;
        total.2 += c;
    }
    total
}

/// Split a complex selector into its compound pieces, dropping the combinators
/// (which contribute zero specificity). Respects bracket/paren nesting so that
/// combinator characters inside `[...]`/`(...)` are not treated as separators.
fn split_into_compounds(selector: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut start = 0usize;
    let mut last_was_sep = false;
    for (byte_index, ch) in selector.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }
        let is_sep = bracket_depth == 0
            && paren_depth == 0
            && (ch.is_whitespace() || ch == '>' || ch == '+' || ch == '~');
        if is_sep {
            if !last_was_sep {
                let piece = selector[start..byte_index].trim();
                if !piece.is_empty() {
                    pieces.push(piece);
                }
            }
            // Advance start past this separator char.
            start = byte_index + ch.len_utf8();
            last_was_sep = true;
        } else {
            last_was_sep = false;
        }
    }
    let tail = selector[start..].trim();
    if !tail.is_empty() {
        pieces.push(tail);
    }
    pieces
}

/// Specificity (a, b, c) of a single compound selector (no combinators).
fn compound_specificity(compound: &str) -> (u32, u32, u32) {
    let (head, pseudos) = split_compound(compound);
    let (mut a, mut b, mut c) = head_specificity(head);

    for pseudo in pseudos {
        // Functional logical pseudos contribute the specificity of their
        // argument list (most specific item); :where() contributes zero.
        if let Some(arg) = functional_arg(pseudo, ":where(") {
            let _ = arg; // zero contribution
        } else if let Some(arg) = functional_arg(pseudo, ":is(")
            .or_else(|| functional_arg(pseudo, ":not("))
            .or_else(|| functional_arg(pseudo, ":has("))
        {
            let (na, nb, nc) = split_selector_list(arg)
                .into_iter()
                .map(complex_specificity)
                .max_by_key(|(a, b, c)| (*a, *b, *c))
                .unwrap_or((0, 0, 0));
            a += na;
            b += nb;
            c += nc;
        } else if let Some(arg) = functional_arg(pseudo, ":nth-child(")
            .or_else(|| functional_arg(pseudo, ":nth-last-child("))
        {
            b += 1;
            if let Some((_, selector_list)) = split_nth_of(arg) {
                let (na, nb, nc) = split_selector_list(selector_list)
                    .into_iter()
                    .map(complex_specificity)
                    .max_by_key(|(a, b, c)| (*a, *b, *c))
                    .unwrap_or((0, 0, 0));
                a += na;
                b += nb;
                c += nc;
            }
        } else {
            // Plain (or functional structural) pseudo-class: counts as a class.
            b += 1;
        }
    }

    (a, b, c)
}

/// Specificity of the non-pseudo head (tag/universal + ids + classes + attrs).
fn head_specificity(head: &str) -> (u32, u32, u32) {
    // Splice out every `[...]` attribute selector, counting each toward `b`,
    // then score the remaining tag/class/id part.
    let mut attr_count = 0u32;
    let mut simple = String::with_capacity(head.len());
    let mut chars = head.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '[' {
            attr_count += 1;
            // Skip until the matching `]`.
            for (_, c) in chars.by_ref() {
                if c == ']' {
                    break;
                }
            }
        } else {
            simple.push(ch);
            let _ = i;
        }
    }
    let (a, b, c) = simple_part_specificity(&simple);
    (a, b + attr_count, c)
}

/// Specificity of a simple compound with no attribute selectors: an optional
/// type/universal part plus `.class` / `#id` segments.
fn simple_part_specificity(part: &str) -> (u32, u32, u32) {
    let (mut a, mut b, mut c) = (0u32, 0u32, 0u32);
    if part.is_empty() {
        return (a, b, c);
    }
    let first_delim = part.find(['.', '#']);
    let (tag_part, segs) = match first_delim {
        Some(i) => part.split_at(i),
        None => (part, ""),
    };
    // A type selector adds to c; `*` and an empty type part add nothing.
    if !tag_part.is_empty() && tag_part != "*" {
        c += 1;
    }
    let mut chars = segs.char_indices().peekable();
    while let Some((start, marker)) = chars.next() {
        let mut end = segs.len();
        while let Some(&(idx, ch)) = chars.peek() {
            if ch == '.' || ch == '#' {
                end = idx;
                break;
            }
            chars.next();
        }
        let _name = &segs[start + 1..end];
        match marker {
            '#' => a += 1,
            '.' => b += 1,
            _ => {}
        }
    }
    (a, b, c)
}
