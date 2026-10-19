//! Selector-matching cost as stylesheet and document shape grow.
//!
//! Each group isolates one scaling axis seen in generated reports: the number
//! of class rules, descendant selectors such as `.header .header-title`, and
//! wide sibling lists such as long tables.
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fmt::Write;
use std::hint::black_box;

/// `rules` single-class rules over a fixed 200-card document whose elements
/// each use a few of those classes.
fn many_classes_html(rules: usize) -> String {
    let mut html = String::from("<style>\n");
    for i in 0..rules {
        let _ = writeln!(
            html,
            ".c{i} {{ color: #{:06x}; padding: {}pt; }}",
            i * 97 % 0xffffff,
            i % 7
        );
    }
    html.push_str("</style>\n");
    for card in 0..200 {
        let a = card % rules;
        let b = (card * 7 + 3) % rules;
        let _ = writeln!(
            html,
            "<div class=\"c{a} c{b}\"><p class=\"c{b}\">Card {card} body text.</p></div>"
        );
    }
    html
}

/// `rules` two-compound descendant rules (`.section-N .title-N`) over a
/// document nested `depth` levels below each section.
fn descendant_html(rules: usize, depth: usize) -> String {
    let mut html = String::from("<style>\n");
    for i in 0..rules {
        let _ = writeln!(
            html,
            ".section-{i} .title-{i} {{ font-weight: bold; color: navy; }}"
        );
    }
    html.push_str("</style>\n");
    for section in 0..100 {
        let s = section % rules;
        let _ = write!(html, "<div class=\"section-{s}\">");
        for _ in 0..depth {
            html.push_str("<div class=\"wrap\">");
        }
        let _ = write!(html, "<p class=\"title-{s}\">Section {section}</p>");
        for _ in 0..depth {
            html.push_str("</div>");
        }
        html.push_str("</div>\n");
    }
    html
}

/// A table with `rows` rows styled by a handful of realistic report rules.
fn wide_table_html(rows: usize) -> String {
    let mut html = String::from(
        "<style>
  table { border-collapse: collapse; width: 100%; }
  .report .row td { border: 1pt solid #ddd; padding: 3pt; }
  .report .row:nth-child(even) td { background-color: #f5f5f5; }
  .report .row > .num { text-align: right; }
</style>
<table class=\"report\"><tbody>\n",
    );
    for row in 0..rows {
        let _ = writeln!(
            html,
            "<tr class=\"row\"><td>Item {row}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row * 3,
            row * 7
        );
    }
    html.push_str("</tbody></table>\n");
    html
}

fn bench_many_classes(c: &mut Criterion) {
    let mut group = c.benchmark_group("selectors/many_classes");
    group.sample_size(10);
    for rules in [100, 1_000, 5_000] {
        let html = many_classes_html(rules);
        group.bench_with_input(BenchmarkId::from_parameter(rules), &html, |b, html| {
            b.iter(|| ironpress::html_to_pdf(black_box(html)).unwrap())
        });
    }
    group.finish();
}

fn bench_descendant(c: &mut Criterion) {
    let mut group = c.benchmark_group("selectors/descendant");
    group.sample_size(10);
    for (rules, depth) in [(100, 2), (1_000, 2), (1_000, 8)] {
        let html = descendant_html(rules, depth);
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{rules}_rules_depth_{depth}")),
            &html,
            |b, html| b.iter(|| ironpress::html_to_pdf(black_box(html)).unwrap()),
        );
    }
    group.finish();
}

fn bench_wide_table(c: &mut Criterion) {
    let mut group = c.benchmark_group("selectors/wide_table");
    group.sample_size(10);
    for rows in [100, 500, 1_000] {
        let html = wide_table_html(rows);
        group.bench_with_input(BenchmarkId::from_parameter(rows), &html, |b, html| {
            b.iter(|| ironpress::html_to_pdf(black_box(html)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_many_classes,
    bench_descendant,
    bench_wide_table
);
criterion_main!(benches);
