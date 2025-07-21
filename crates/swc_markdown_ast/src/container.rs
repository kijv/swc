use swc_common::{ast_node, EqIgnoreSpan, Span};

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub enum ContainerBlock {
    #[tag("BlockQuote")]
    BlockQuote(BlockQuote),
    #[tag("ListItem")]
    ListItem(ListItem),
    #[tag("List")]
    List(List),
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct BlockQuote {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct ListItem {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct List {
    pub span: Span,
}
