use swc_common::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenAndSpan {
    pub span: Span,
    pub token: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    ThematicBreak,
    ATXHeading {
        level: u8,
        content: String,
    },
    SetextHeading {
        level: u8,
        content: String,
    },
    IndentedCodeBlock(String),
    FencedCodeBlock {
        info: String,
        content: String,
    },
    HTMLBlock(String),
    LinkReferenceDefinition {
        label: String,
        destination: String,
        title: Option<String>,
    },
    Paragraph(String),
    BlankLine,
    BlockQuoteStart,
    ListItemStart,
    ListStart,
    Eof,
}

// impl Token {
//     pub fn to_string(&self) -> Option<String> {
//         match self {
//             Token::Line(s) => Some(s.to_owned()),
//             Token::Eof => None,
//         }
//     }
// }
