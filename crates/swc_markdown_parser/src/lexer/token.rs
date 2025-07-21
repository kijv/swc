use swc_common::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenAndSpan {
    pub span: Span,
    pub token: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    // Whitespace tokens
    Tab,                   // Tab character (equivalent to 4 spaces)
    Space(u32),            // Run of spaces (count)
    Newline,               // Line break (normalized to \n)
    Marker(char, u32),     // Potential block markers (e.g., '#', 3)
    Text(String),          // Sequence of non-special chars
    BackslashEscape(char), // Escaped char (e.g., \\punct)
    Entity(String),        // Resolved entity (e.g., &amp; -> '&')
    Raw(char),             // Uninterpreted chars in code/HTML contexts
    Eof,
}
