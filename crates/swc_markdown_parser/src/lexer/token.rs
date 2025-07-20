use swc_atoms::Atom;
use swc_common::{EqIgnoreSpan, Span};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EqIgnoreSpan)]
pub struct TokenAndSpan {
    pub span: Span,
    pub token: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EqIgnoreSpan)]
pub enum Raw {
    Same,
    Atom(Atom),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EqIgnoreSpan)]
pub enum Token {
    Document,
    SoftBreak,
    ThematicBreak,
    Heading { level: u8, value: String },
    Paragraph(String),
    Block { block: Block, children: Vec<Token> },
    Character { value: char, raw: Option<Raw> },
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EqIgnoreSpan)]
pub enum Block {
    ThematicBreak(Atom),
}
