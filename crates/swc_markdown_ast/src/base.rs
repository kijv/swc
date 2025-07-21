use swc_common::{ast_node, EqIgnoreSpan, Span};

use crate::{ContainerBlock, InlineBlock, LeafBlock};

#[ast_node("Document")]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Document {
    pub span: Span,
    pub mdx: bool,
    pub gfm: bool,
    pub children: Vec<Block>,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub enum Block {
    #[tag("LeafBlock")]
    Leaf(LeafBlock),
    #[tag("ContainerBlock")]
    Container(ContainerBlock),
    #[tag("InlineBlock")]
    Inline(InlineBlock),
}
