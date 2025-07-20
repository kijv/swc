use std::mem;

use swc_common::{Span, DUMMY_SP};
use swc_markdown_ast::Document;

use self::{
    input::ParserInput,
    node::{Data, Node, RcNode},
};
use crate::{error::Error, parser::input::Buffer};

pub mod input;
mod node;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParserConfig {
    pub gfm: bool,
    pub mdx: bool,
}

pub type PResult<T> = Result<T, Error>;

pub struct Parser<I>
where
    I: ParserInput,
{
    #[allow(dead_code)]
    config: ParserConfig,
    input: Buffer<I>,
    stopped: bool,
    // is_fragment_case: bool,
    // context_element: Option<RcNode>,
    // insertion_mode: InsertionMode,
    // original_insertion_mode: InsertionMode,
    // template_insertion_mode_stack: Vec<InsertionMode>,
    document: Option<RcNode>,
    // head_element_pointer: Option<RcNode>,
    // form_element_pointer: Option<RcNode>,
    // open_elements_stack: OpenElementsStack,
    // active_formatting_elements: ActiveFormattingElementStack,
    // pending_character_tokens: Vec<TokenAndInfo>,
    // frameset_ok: bool,
    // foster_parenting_enabled: bool,
    errors: Vec<Error>,
}

impl<I> Parser<I>
where
    I: ParserInput,
{
    pub fn new(input: I, config: ParserConfig) -> Self {
        Parser {
            config,
            input: Buffer::new(input),
            stopped: false,
            // is_fragment_case: false,
            // context_element: None,
            // insertion_mode: Default::default(),
            // original_insertion_mode: Default::default(),
            // template_insertion_mode_stack: Vec::with_capacity(16),
            document: None,
            // head_element_pointer: None,
            // form_element_pointer: None,
            // open_elements_stack: OpenElementsStack::new(),
            // active_formatting_elements: ActiveFormattingElementStack::new(),
            // pending_character_tokens: Vec::with_capacity(16),
            // frameset_ok: true,
            // foster_parenting_enabled: false,
            errors: Default::default(),
        }
    }

    pub fn dump_cur(&mut self) -> String {
        format!("{:?}", self.input.cur())
    }

    pub fn take_errors(&mut self) -> Vec<Error> {
        mem::take(&mut self.errors)
    }

    // pub fn parse_document(&mut self) -> PResult<Document> {
    //     let start = self.input.cur_span()?;

    //     self.document = Some(self.create_document(None));

    //     self.run()?;

    //     let document = &mut self.document.take().unwrap();
    //     let nodes = document.children.take();
    //     let mut children = Vec::with_capacity(nodes.len());

    //     for node in nodes {
    //         children.push(self.node_to_child(node));
    //     }

    //     let last = self.input.last_pos()?;
    //     let mode = match &document.data {
    //         Data::Document { mode, .. } => *mode.borrow(),
    //         _ => {
    //             unreachable!();
    //         }
    //     };

    //     Ok(Document {
    //         span: Span::new(start.lo(), last),
    //         children,
    //     })
    // }

    fn create_document(&self) -> RcNode {
        Node::new(Data::Document {}, DUMMY_SP)
    }
}
