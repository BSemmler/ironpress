//! Overflow-related authored properties.

use crate::parser::css::CssValue;
use crate::style::computed::{ComputedStyle, Overflow};

use super::AuthoredCascade;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutOverflowKeyword {
    Visible,
    Clip,
    Hidden,
    Scroll,
    Auto,
}

impl LayoutOverflowKeyword {
    pub(crate) fn clips(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub(crate) fn establishes_bfc(self) -> bool {
        matches!(self, Self::Hidden | Self::Scroll | Self::Auto)
    }

    fn is_scrollable(self) -> bool {
        matches!(self, Self::Hidden | Self::Scroll | Self::Auto)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutOverflowAxes {
    pub(crate) x: LayoutOverflowKeyword,
    pub(crate) y: LayoutOverflowKeyword,
}

impl LayoutOverflowAxes {
    pub(crate) fn clips_any(self) -> bool {
        self.x.clips() || self.y.clips()
    }

    pub(crate) fn establishes_bfc(self) -> bool {
        self.x.establishes_bfc() || self.y.establishes_bfc()
    }
}

fn parse_layout_overflow_keyword(value: &str) -> LayoutOverflowKeyword {
    match value.trim().to_ascii_lowercase().as_str() {
        "clip" => LayoutOverflowKeyword::Clip,
        "hidden" => LayoutOverflowKeyword::Hidden,
        "scroll" => LayoutOverflowKeyword::Scroll,
        "auto" => LayoutOverflowKeyword::Auto,
        _ => LayoutOverflowKeyword::Visible,
    }
}

fn coerce_layout_overflow_axis(
    this: LayoutOverflowKeyword,
    other: LayoutOverflowKeyword,
) -> LayoutOverflowKeyword {
    match this {
        LayoutOverflowKeyword::Visible if other.is_scrollable() => LayoutOverflowKeyword::Auto,
        LayoutOverflowKeyword::Clip if other.is_scrollable() => LayoutOverflowKeyword::Hidden,
        value => value,
    }
}

impl AuthoredCascade<'_> {
    /// Used overflow per axis, with authored values (including logical
    /// longhands) overriding the computed style.
    pub(crate) fn overflow_axes(&self, style: &ComputedStyle) -> LayoutOverflowAxes {
        let from_computed = |value| match value {
            Overflow::Visible => LayoutOverflowKeyword::Visible,
            Overflow::Hidden => LayoutOverflowKeyword::Hidden,
            Overflow::Scroll => LayoutOverflowKeyword::Scroll,
            Overflow::Auto => LayoutOverflowKeyword::Auto,
        };
        let mut x = from_computed(style.overflow_x);
        let mut y = from_computed(style.overflow_y);
        let mut saw_authored = false;

        if let Some(value) = self.keyword("overflow") {
            let mut parts = value.split_whitespace();
            if let Some(first) = parts.next() {
                x = parse_layout_overflow_keyword(first);
                y = parts.next().map(parse_layout_overflow_keyword).unwrap_or(x);
                saw_authored = true;
            }
        }
        if let Some(value) = self.keyword("overflow-x") {
            x = parse_layout_overflow_keyword(&value);
            saw_authored = true;
        }
        if let Some(value) = self.keyword("overflow-y") {
            y = parse_layout_overflow_keyword(&value);
            saw_authored = true;
        }
        // Horizontal writing-mode is the only mode currently laid out by the block
        // engine, so logical inline/block overflow map to x/y here.
        if let Some(value) = self.keyword("overflow-inline") {
            x = parse_layout_overflow_keyword(&value);
            saw_authored = true;
        }
        if let Some(value) = self.keyword("overflow-block") {
            y = parse_layout_overflow_keyword(&value);
            saw_authored = true;
        }

        if saw_authored {
            LayoutOverflowAxes {
                x: coerce_layout_overflow_axis(x, y),
                y: coerce_layout_overflow_axis(y, x),
            }
        } else {
            LayoutOverflowAxes { x, y }
        }
    }

    pub(crate) fn line_clamp(&self) -> Option<usize> {
        for property in ["line-clamp", "-webkit-line-clamp"] {
            let Some(value) = self.value(property) else {
                continue;
            };
            let n = match value {
                CssValue::Number(n) => n,
                CssValue::Keyword(raw) => raw
                    .split_whitespace()
                    .find_map(|part| part.parse::<f32>().ok())?,
                _ => continue,
            };
            if n.is_finite() && n >= 1.0 {
                return Some(n.floor() as usize);
            }
        }
        None
    }

    pub(crate) fn overflow_clip_margin(&self) -> f32 {
        match self.value("overflow-clip-margin") {
            Some(CssValue::Length(v)) if v.is_finite() => v.max(0.0),
            Some(CssValue::Number(v)) if v.is_finite() => v.max(0.0),
            _ => 0.0,
        }
    }

    /// Reserved `scrollbar-gutter` space as (left, right) padding.
    pub(crate) fn scrollbar_gutter(&self) -> (f32, f32) {
        let Some(value) = self.keyword("scrollbar-gutter") else {
            return (0.0, 0.0);
        };
        if !value.split_whitespace().any(|part| part == "stable") {
            return (0.0, 0.0);
        }
        let gutter = 15.0 * 0.75;
        if value.split_whitespace().any(|part| part == "both-edges") {
            (gutter, gutter)
        } else {
            (0.0, gutter)
        }
    }
}
