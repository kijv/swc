use swc_common::{ast_node, EqIgnoreSpan, Span};

#[ast_node("Document")]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Document {
    pub span: Span,
    // pub mode: DocumentMode,
    pub children: Vec<Child>,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub enum Child {
    #[tag("ThematicBreak")]
    ThematicBreak(ThematicBreak),
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct ThematicBreak {
    pub span: Span,
}
