use swc_common::{ast_node, EqIgnoreSpan, Span};

use crate::InlineBlock;

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub enum LeafBlock {
    #[tag("ThematicBreak")]
    ThematicBreak(ThematicBreak),
    #[tag("ATXHeading")]
    ATXHeading(ATXHeading),
    #[tag("SetextHeading")]
    SetextHeading(SetextHeading),
    #[tag("IndentedCodeBlock")]
    IndentedCodeBlock(IndentedCodeBlock),
    #[tag("FencedCodeBlock")]
    FencedCodeBlock(FencedCodeBlock),
    #[tag("HTMLBlock")]
    HTMLBlock(HTMLBlock),
    #[tag("LinkReferenceDefinition")]
    LinkReferenceDefinition(LinkReferenceDefinition),
    #[tag("Paragraph")]
    Paragraph(Paragraph),
    #[tag("BlankLine")]
    BlankLine(BlankLine),
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct ThematicBreak {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct ATXHeading {
    pub span: Span,
    pub level: u8,
    pub children: Vec<InlineBlock>,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct SetextHeading {
    pub span: Span,
    pub level: u8,
    pub children: Vec<InlineBlock>,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct IndentedCodeBlock {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct FencedCodeBlock {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct HTMLBlock {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct LinkReferenceDefinition {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Paragraph {
    pub span: Span,
    pub children: Vec<InlineBlock>,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct BlankLine {
    pub span: Span,
}
