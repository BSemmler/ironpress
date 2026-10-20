//! Attribute selectors (Selectors-4 §6): presence and the six value
//! operators, with the trailing `i`/`s` case-sensitivity flag.

use super::element::SelectorElement;

#[derive(Debug, Clone)]
pub(crate) struct AttributeSelector {
    name: String,
    value: Option<AttributeValue>,
}

#[derive(Debug, Clone)]
struct AttributeValue {
    operator: AttributeOperator,
    value: String,
    case_insensitive: bool,
}

#[derive(Debug, Clone, Copy)]
enum AttributeOperator {
    /// `=`
    Equals,
    /// `~=`
    Includes,
    /// `|=`
    DashMatch,
    /// `^=`
    Prefix,
    /// `$=`
    Suffix,
    /// `*=`
    Substring,
}

impl AttributeSelector {
    /// Parse every `[...]` group in `selector`. Text outside a group that ends
    /// in `]` is ignored. Returns `None` when a group has an operator this
    /// grammar does not define, because such a group never matches.
    pub(super) fn parse_all(selector: &str) -> Option<Vec<Self>> {
        selector
            .split('[')
            .filter_map(|part| part.strip_suffix(']'))
            .map(Self::parse)
            .collect()
    }

    /// Parse the text between `[` and `]`.
    fn parse(expr: &str) -> Option<Self> {
        let expr = expr.trim();
        let (expr, case_insensitive) = if let Some(stripped) = strip_case_flag(expr, 'i') {
            (stripped, true)
        } else if let Some(stripped) = strip_case_flag(expr, 's') {
            (stripped, false)
        } else {
            (expr, false)
        };

        let Some(op_index) = expr.find(['=', '~', '|', '^', '$', '*']) else {
            return Some(Self {
                name: expr.trim().to_string(),
                value: None,
            });
        };

        let (name_part, op_and_value) = expr.split_at(op_index);
        let (operator, value_part) = [
            ("~=", AttributeOperator::Includes),
            ("|=", AttributeOperator::DashMatch),
            ("^=", AttributeOperator::Prefix),
            ("$=", AttributeOperator::Suffix),
            ("*=", AttributeOperator::Substring),
            ("=", AttributeOperator::Equals),
        ]
        .into_iter()
        .find_map(|(token, operator)| Some((operator, op_and_value.strip_prefix(token)?)))?;

        let value = unquote(value_part.trim());
        Some(Self {
            name: name_part.trim().to_string(),
            value: Some(AttributeValue {
                operator,
                // Case-insensitive comparisons fold both sides; fold this side once.
                value: if case_insensitive {
                    value.to_ascii_lowercase()
                } else {
                    value.to_string()
                },
                case_insensitive,
            }),
        })
    }

    pub(super) fn matches(&self, element: &impl SelectorElement) -> bool {
        // Attribute names are ASCII case-insensitive in HTML.
        let mut attributes = element.attributes().into_iter().flatten();
        let Some(value) = &self.value else {
            return attributes.any(|(name, _)| name.eq_ignore_ascii_case(&self.name));
        };
        attributes
            .find(|(name, _)| name.eq_ignore_ascii_case(&self.name))
            .is_some_and(|(_, have)| value.matches(have))
    }
}

impl AttributeValue {
    fn matches(&self, have: &str) -> bool {
        let folded;
        let have = if self.case_insensitive {
            folded = have.to_ascii_lowercase();
            folded.as_str()
        } else {
            have
        };
        let want = self.value.as_str();
        // An empty value never matches the word/prefix/suffix/substring operators.
        match self.operator {
            AttributeOperator::Equals => have == want,
            AttributeOperator::Includes => {
                !want.is_empty() && have.split_ascii_whitespace().any(|word| word == want)
            }
            AttributeOperator::DashMatch => {
                have == want
                    || have
                        .strip_prefix(want)
                        .is_some_and(|rest| rest.starts_with('-'))
            }
            AttributeOperator::Prefix => !want.is_empty() && have.starts_with(want),
            AttributeOperator::Suffix => !want.is_empty() && have.ends_with(want),
            AttributeOperator::Substring => !want.is_empty() && have.contains(want),
        }
    }
}

/// Strip a trailing ` i` / ` s` case flag (Selectors-4 §6.3) from an attribute
/// expression, returning the expression without the flag if present.
fn strip_case_flag(expr: &str, flag: char) -> Option<&str> {
    let trimmed = expr.trim_end();
    let without = trimmed.strip_suffix(flag)?;
    // The flag must be a standalone token: preceded by whitespace and the value
    // must contain an operator (so a bare `[attr]` ending in `i` isn't stripped).
    if without.ends_with(char::is_whitespace) && without.contains('=') {
        Some(without.trim_end())
    } else {
        None
    }
}

/// Remove surrounding single or double quotes from a value token.
pub(super) fn unquote(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}
