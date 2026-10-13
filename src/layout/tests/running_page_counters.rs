//! CSS GCPM 3 §2: a running element is displayed in page-margin boxes, so
//! page-based counters in its content take the values of the displaying page.

use super::support::layout_pages;
use crate::layout::elements::{LayoutElement, LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::layout::page_counter::PageCounters;
use crate::parser::css::{PageContentPolicy, PageContentReference};

const FOOTER: &str = r#"<style>
    .footer { position: running(footer) }
    .page::after { content: counter(page) }
    .pages::after { content: counter(pages, upper-roman) }
</style>
<div class="footer">Page <span class="page"></span> of <span class="pages"></span></div>
<div>First</div>
<div style="break-before: page">Second</div>
<div style="break-before: page">Third</div>"#;

fn painted_text(element: &dyn LayoutElement) -> String {
    struct Text(String);

    impl LayoutVisitor for Text {
        fn visit_text_block(&mut self, block: &TextBlock) {
            for line in &block.lines {
                for run in &line.runs {
                    self.0.push_str(&run.text);
                }
            }
        }
    }

    let mut text = Text(String::new());
    visit_layout_tree(element, &mut text);
    text.0
}

fn footer_on(page_index: usize) -> String {
    let pages = layout_pages(FOOTER);
    let footer = pages[page_index]
        .generated_content
        .running_element(&PageContentReference::new(
            "footer".into(),
            PageContentPolicy::Last,
        ))
        .expect("running footer");
    let counters = PageCounters {
        page: page_index + 1,
        pages: pages.len(),
    };
    painted_text(counters.resolve_element(footer).as_ref())
}

#[test]
fn running_element_page_counters_take_the_displaying_page_values() {
    assert_eq!(footer_on(0), "Page 1 of III");
    assert_eq!(footer_on(1), "Page 2 of III");
    assert_eq!(footer_on(2), "Page 3 of III");
}
