// #![deny(warnings)]

use std::path::PathBuf;

// use common::document_span_visualizer;
use crate::common::document_test;

#[path = "common/mod.rs"]
mod common;

#[testing::fixture("tests/fixture/commonmark/4_leaf_blocks/1_thematic_breaks/*/*.md")]
fn pass(input: PathBuf) {
    document_test(input, Default::default())
}

// #[testing::fixture("tests/fixture/commonmark/4_leaf_blocks/1_thematic_breaks/
// **/*.md")] fn span_visualizer(input: PathBuf) {
//     document_span_visualizer(input, Default::default(), false)
// }

// #[testing::fixture("tests/fixture/**/*.html")]
// fn span_visualizer(input: PathBuf) {
//     document_span_visualizer(input, Default::default(), false)
// }

// #[testing::fixture("tests/fixture/**/*.html")]
// fn dom_visualizer(input: PathBuf) {
//     document_dom_visualizer(input, Default::default())
// }
