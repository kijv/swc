use swc_common::{ast_node, EqIgnoreSpan, Span};

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub enum InlineBlock {
    #[tag("CodeSpan")]
    CodeSpan(CodeSpan),
    #[tag("EmphasisAndStrongEmphasis")]
    EmphasisAndStrongEmphasis(EmphasisAndStrongEmphasis),
    #[tag("Link")]
    Link(Link),
    #[tag("Image")]
    Image(Image),
    #[tag("Autolink")]
    Autolink(Autolink),
    #[tag("RawHTML")]
    RawHTML(RawHTML),
    #[tag("HardLineBreak")]
    HardLineBreak(HardLineBreak),
    #[tag("SoftLineBreak")]
    SoftLineBreak(SoftLineBreak),
    #[tag("TextualContent")]
    TextualContent(TextualContent),
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct CodeSpan {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct EmphasisAndStrongEmphasis {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Link {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Image {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct Autolink {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct RawHTML {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct HardLineBreak {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct SoftLineBreak {
    pub span: Span,
}

#[ast_node]
#[derive(Eq, Hash, EqIgnoreSpan)]
pub struct TextualContent {
    pub span: Span,
    pub content: String,
}
