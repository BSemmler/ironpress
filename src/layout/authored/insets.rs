//! Authored `inset` and inset longhands.

use crate::parser::css::CssValue;
use crate::style::computed::ComputedStyle;

use super::AuthoredCascade;

impl AuthoredCascade<'_> {
    /// Apply authored `inset`, physical and logical inset longhands to
    /// `style`, later properties overriding earlier ones.
    pub(crate) fn apply_insets(&self, style: &mut ComputedStyle) {
        if let Some(value) = self.value("inset") {
            let values = match value {
                CssValue::Keyword(raw) => raw
                    .split_whitespace()
                    .filter_map(parse_inset_component)
                    .collect::<Vec<_>>(),
                CssValue::Length(_) | CssValue::Percentage(_) => vec![value],
                _ => Vec::new(),
            };
            if !values.is_empty() {
                let top = values[0].clone();
                let right = values.get(1).cloned().unwrap_or_else(|| top.clone());
                let bottom = values.get(2).cloned().unwrap_or_else(|| top.clone());
                let left = values.get(3).cloned().unwrap_or_else(|| right.clone());
                set_inset_side(style, "top", &top);
                set_inset_side(style, "right", &right);
                set_inset_side(style, "bottom", &bottom);
                set_inset_side(style, "left", &left);
            }
        }

        for (property, side) in [
            ("top", "top"),
            ("right", "right"),
            ("bottom", "bottom"),
            ("left", "left"),
            ("inset-block-start", "top"),
            ("inset-inline-end", "right"),
            ("inset-block-end", "bottom"),
            ("inset-inline-start", "left"),
        ] {
            if let Some(value) = self.value(property) {
                set_inset_side(style, side, &value);
            }
        }
    }
}

fn parse_inset_component(raw: &str) -> Option<CssValue> {
    crate::parser::css::parse_length(raw)
}

fn set_inset_side(style: &mut ComputedStyle, side: &str, value: &CssValue) {
    match (side, value) {
        ("top", CssValue::Length(v)) => {
            style.top = Some(*v);
            style.percentage_insets.top = None;
        }
        ("right", CssValue::Length(v)) => {
            style.right = Some(*v);
            style.percentage_insets.right = None;
        }
        ("bottom", CssValue::Length(v)) => {
            style.bottom = Some(*v);
            style.percentage_insets.bottom = None;
        }
        ("left", CssValue::Length(v)) => {
            style.left = Some(*v);
            style.percentage_insets.left = None;
        }
        ("top", CssValue::Percentage(v)) => {
            style.top = None;
            style.percentage_insets.top = Some(*v);
        }
        ("right", CssValue::Percentage(v)) => {
            style.right = None;
            style.percentage_insets.right = Some(*v);
        }
        ("bottom", CssValue::Percentage(v)) => {
            style.bottom = None;
            style.percentage_insets.bottom = Some(*v);
        }
        ("left", CssValue::Percentage(v)) => {
            style.left = None;
            style.percentage_insets.left = Some(*v);
        }
        _ => {}
    }
}
