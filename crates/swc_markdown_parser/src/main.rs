// use std::path::PathBuf;

// use swc_common::{errors::Handler, input::SourceFileInput, Spanned};
// use swc_markdown_ast::*;
// use swc_markdown_parser::{
//     lexer::Lexer,
//     parser::{PResult, Parser, ParserConfig},
// };
// use swc_markdown_visit::{Visit, VisitMut, VisitMutWith, VisitWith};
// use testing::NormalizedOutput;

// #[allow(dead_code)]
// pub fn document_test(input: PathBuf, config: ParserConfig) {
//     testing::run_test2(false, |cm, handler| {
//         let json_path = input.parent().unwrap().join("output.json");
//         let fm = cm.load_file(&input).unwrap();
//         let lexer = Lexer::new(SourceFileInput::from(&*fm));

//         Ok(())
//     })
//     .unwrap();
// }

use swc_common::{input::SourceFileInput, BytePos};
use swc_markdown_parser::{
    lexer::Lexer,
    parser::{
        input::{Buffer, ParserInput},
        PResult, Parser, ParserConfig,
    },
};

fn main() {
    let input = "> abc\n> def";
    let lexer = Lexer::new(SourceFileInput::new(
        input,
        BytePos(0),
        BytePos(input.len().try_into().unwrap()),
    ));
    let mut parser_input = Buffer::new(lexer);

    let token = parser_input.cur();

    dbg!(&token);

    // assert!(token.is_ok());
}
