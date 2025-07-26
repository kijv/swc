use swc_common::{input::StringInput, SourceFile};
use swc_markdown_ast::Document;

use crate::{
    error::Error,
    lexer::Lexer,
    parser::{PResult, Parser, ParserConfig},
};

pub mod error;
pub mod lexer;
pub mod parser;

/// Parse a given file as `Document`.
pub fn parse_file_as_document(
    fm: &SourceFile,
    config: ParserConfig,
    diagnostics: &mut Vec<Error>,
) -> PResult<Document> {
    let lexer = Lexer::new(StringInput::from(fm));
    let mut parser = Parser::new(lexer, config);
    let result = parser.parse_document();

    diagnostics.extend(parser.take_errors());

    result
}
