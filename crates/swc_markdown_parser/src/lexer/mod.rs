use std::{cell::RefCell, char::REPLACEMENT_CHARACTER, collections::VecDeque, mem::take, rc::Rc};

use rustc_hash::FxHashSet;
use swc_atoms::{atom, Atom};
use swc_common::{input::Input, BytePos, Span};

pub mod token;

// use swc_html_utils::{Entity, HTML_ENTITIES};
use crate::{
    error::{Error, ErrorKind},
    lexer::token::{Raw, Token, TokenAndSpan},
    parser::input::ParserInput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Document,
    BlockQuote,
    ContinuedBlockQuote,
    ListItem,
    ContinuedListItem,
    List,
    ContinuedList,
    Inline,
}

pub(crate) type LexResult<T> = Result<T, ErrorKind>;

pub struct Lexer<'a, I>
where
    I: Input<'a>,
{
    input: I,
    cur: Option<char>,
    cur_pos: BytePos,
    cur_line: Option<String>,
    last_token_pos: BytePos,
    finished: bool,
    state: State,
    return_state: State,
    errors: Vec<Error>,
    last_start_tag_name: Option<Atom>,
    pending_tokens: VecDeque<TokenAndSpan>,
    buf: Rc<RefCell<String>>,
    sub_buf: Rc<RefCell<String>>,
    current_token: Option<Token>,
    // attributes_validator: FxHashSet<Atom>,
    // attribute_start_position: Option<BytePos>,
    // character_reference_code: Option<Vec<(u8, u32, Option<char>)>>,
    temporary_buffer: String,
    // is_adjusted_current_node_is_element_in_html_namespace: Option<bool>,
    phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a, I> Lexer<'a, I>
where
    I: Input<'a>,
{
    pub fn new(input: I) -> Self {
        let start_pos = input.last_pos();

        let mut lexer = Lexer {
            input,
            cur: None,
            cur_pos: start_pos,
            cur_line: None,
            last_token_pos: start_pos,
            finished: false,
            state: State::Document,
            return_state: State::Document,
            errors: Vec::new(),
            last_start_tag_name: None,
            pending_tokens: VecDeque::with_capacity(16),
            buf: Rc::new(RefCell::new(String::with_capacity(256))),
            sub_buf: Rc::new(RefCell::new(String::with_capacity(256))),
            current_token: None,
            // attributes_validator: Default::default(),
            // attribute_start_position: None,
            // character_reference_code: None,
            // Do this without a new allocation.
            temporary_buffer: String::with_capacity(33),
            // is_adjusted_current_node_is_element_in_html_namespace: None,
            phantom: std::marker::PhantomData,
        };

        // A leading Byte Order Mark (BOM) causes the character encoding argument to be
        // ignored and will itself be skipped.
        if lexer.input.is_at_start() && lexer.input.cur() == Some('\u{feff}') {
            unsafe {
                // Safety: We know that the current character is '\u{feff}'.
                lexer.input.bump();
            }
        }

        lexer
    }
}

impl<'a, I: Input<'a>> Iterator for Lexer<'a, I> {
    type Item = TokenAndSpan;

    fn next(&mut self) -> Option<Self::Item> {
        let token_and_span = self.read_token_and_span();

        match token_and_span {
            Ok(token_and_span) => {
                return Some(token_and_span);
            }
            Err(..) => {
                return None;
            }
        }
    }
}

impl<'a, I> ParserInput for Lexer<'a, I>
where
    I: Input<'a>,
{
    fn start_pos(&mut self) -> BytePos {
        self.input.cur_pos()
    }

    fn last_pos(&mut self) -> BytePos {
        self.input.last_pos()
    }

    fn take_errors(&mut self) -> Vec<Error> {
        take(&mut self.errors)
    }

    fn set_last_start_tag_name(&mut self, tag_name: &Atom) {
        self.last_start_tag_name = Some(tag_name.clone());
    }

    // fn set_adjusted_current_node_to_html_namespace(&mut self, value: bool) {
    //     self.is_adjusted_current_node_is_element_in_html_namespace = Some(value);
    // }

    fn set_input_state(&mut self, state: State) {
        self.state = state;
    }
}

impl<'a, I> Lexer<'a, I>
where
    I: Input<'a>,
{
    #[inline(always)]
    fn next(&mut self) -> Option<char> {
        self.input.cur()
    }

    // Any occurrences of surrogates are surrogate-in-input-stream parse errors. Any
    // occurrences of noncharacters are noncharacter-in-input-stream parse errors
    // and any occurrences of controls other than ASCII whitespace and U+0000 NULL
    // characters are control-character-in-input-stream parse errors.
    //
    // Postpone validation for each character for perf reasons and do it in
    // `anything else`
    #[inline(always)]
    fn validate_input_stream_character(&mut self, c: char) {
        let code = c as u32;

        if is_surrogate(code) {
            self.emit_error(ErrorKind::SurrogateInInputStream);
        } else if is_allowed_control_character(code) {
            self.emit_error(ErrorKind::ControlCharacterInInputStream);
        } else if is_noncharacter(code) {
            self.emit_error(ErrorKind::NoncharacterInInputStream);
        }
    }

    #[inline(always)]
    fn consume(&mut self) {
        self.cur = self.input.cur();
        self.cur_pos = self.input.cur_pos();

        if self.cur_line.is_none() {
            self.cur_line = self.cur.map(|c| c.to_string());
        } else if let Some(line) = &self.cur_line {
            self.cur_line = self.cur.map(|c| line.clone() + &c.to_string());
        }

        if self.cur.is_some() {
            unsafe {
                // Safety: self.cur is Some()
                self.input.bump();
            }
        }
    }

    #[inline(always)]
    fn reconsume(&mut self) {
        unsafe {
            // Safety: self.cur_pos is valid position because we got it from self.input
            self.input.reset_to(self.cur_pos);
        }
    }

    #[inline(always)]
    fn reconsume_in_state(&mut self, state: State) {
        self.state = state;
        self.reconsume();
    }

    #[inline(always)]
    fn consume_next_char(&mut self) -> Option<char> {
        // The next input character is the first character in the input stream that has
        // not yet been consumed or explicitly ignored by the requirements in this
        // section. Initially, the next input character is the first character in the
        // input. The current input character is the last character to have been
        // consumed.
        let c = self.next();

        self.consume();

        c
    }

    #[inline(always)]
    fn consume_next_line(&mut self) -> Option<String> {
        let mut line = String::new();

        if let Some(c) = self.consume_next_char() {
            if c != '\n' {
                line.push(c);
            }
        } else {
            return None;
        }

        while let Some(c) = self.consume_next_char() {
            if c == '\n' {
                break;
            }

            line.push(c);
        }

        self.cur_line = Some(line.to_owned());

        Some(line)
    }

    #[inline(always)]
    fn reconsume_line(&mut self) {
        if let Some(line) = &self.cur_line {
            for _ in 0..line.len() {
                self.reconsume();
            }
        }
    }

    #[cold]
    fn emit_error(&mut self, kind: ErrorKind) {
        self.errors.push(Error::new(
            Span::new(self.cur_pos, self.input.cur_pos()),
            kind,
        ));
    }

    #[inline(always)]
    fn emit_token(&mut self, token: Token) {
        let cur_pos = self.input.cur_pos();

        let span = Span::new(self.last_token_pos, cur_pos);

        self.last_token_pos = cur_pos;
        self.pending_tokens.push_back(TokenAndSpan { span, token });
    }

    // An appropriate end tag token is an end tag token whose tag name matches the
    // tag name of the last start tag to have been emitted from this tokenizer, if
    // any. If no start tag has been emitted from this tokenizer, then no end tag
    // token is appropriate.
    // #[inline(always)]
    // fn current_end_tag_token_is_an_appropriate_end_tag_token(&mut self) -> bool {
    //     if let Some(last_start_tag_name) = &self.last_start_tag_name {
    //         let b = self.buf.clone();
    //         let buf = b.borrow();

    //         return *last_start_tag_name == *buf;
    //     }

    //     false
    // }

    #[inline(always)]
    fn emit_temporary_buffer_as_character_tokens(&mut self) {
        for c in take(&mut self.temporary_buffer).chars() {
            self.emit_token(Token::Character {
                value: c,
                raw: Some(Raw::Same),
            });
        }
    }

    fn flush_code_points_consumed_as_character_reference(&mut self, raw: Option<String>) {
        // When the length of raw is more than the length of temporary buffer we emit a
        // raw character in the first character token
        let mut once_raw = raw;

        let is_value_eq_raw = if let Some(raw) = &once_raw {
            *raw == self.temporary_buffer
        } else {
            true
        };

        for c in take(&mut self.temporary_buffer).chars() {
            self.emit_token(Token::Character {
                value: c,
                raw: if is_value_eq_raw {
                    Some(Raw::Same)
                } else {
                    once_raw.take().map(|x| Raw::Atom(Atom::new(x)))
                },
            });
        }
    }

    fn append_block_token(&mut self, token: Token) {
        if self.current_token.is_none() {
            self.current_token = Some(token);
        } else if let Some(Token::Block { children, .. }) = &mut self.current_token {
            children.push(token);
        }
    }

    fn get_preprended_block_token(&self) -> Option<Token> {
        if let Some(Token::Block { children, .. }) = &self.current_token {
            if let Some(token) = children.last() {
                return Some(token.clone());
            }
        }

        self.current_token.clone()
    }

    #[inline(always)]
    fn close_block_token(&mut self) {
        if let Some(current_token) = self.current_token.take() {
            self.emit_token(current_token);
        }
    }

    fn flush_current_container(&mut self, state: State) {
        self.state = state;
        self.reconsume_line();
        self.close_block_token();
    }

    #[inline(always)]
    fn emit_character_token(&mut self, value: char) {
        self.emit_token(Token::Character {
            value,
            raw: Some(Raw::Same),
        });
    }

    #[inline(always)]
    fn emit_character_token_with_raw(&mut self, c: char, raw_c: char) {
        let b = self.buf.clone();
        let mut buf = b.borrow_mut();

        buf.push(raw_c);

        self.emit_token(Token::Character {
            value: c,
            raw: Some(Raw::Atom(Atom::new(&**buf))),
        });

        buf.clear();
    }

    fn handle_raw_and_emit_character_token(&mut self, c: char) {
        let is_cr = c == '\r';

        if is_cr {
            let b = self.buf.clone();
            let mut buf = b.borrow_mut();

            buf.push(c);

            if self.input.cur() == Some('\n') {
                unsafe {
                    // Safety: cur() is Some('\n')
                    self.input.bump();
                }
                buf.push('\n');
            }

            self.emit_token(Token::Character {
                value: '\n',
                raw: Some(Raw::Atom(Atom::new(&**buf))),
            });

            buf.clear();
        } else {
            self.emit_token(Token::Character {
                value: c,
                raw: Some(Raw::Same),
            });
        }
    }

    fn read_token_and_span(&mut self) -> LexResult<TokenAndSpan> {
        if self.finished {
            return Err(ErrorKind::Eof);
        } else {
            while self.pending_tokens.is_empty() {
                self.run()?;
            }
        }

        let token_and_span = self.pending_tokens.pop_front().unwrap();

        match token_and_span.token {
            Token::Eof => {
                self.finished = true;

                return Err(ErrorKind::Eof);
            }
            _ => {
                return Ok(token_and_span);
            }
        }
    }

    fn run(&mut self) -> LexResult<()> {
        dbg!(&self.state);

        match self.state {
            State::Document => match self.consume_next_line() {
                Some(line) => {
                    let line = line.trim_end();

                    // https://spec.commonmark.org/0.31.2/#blank-lines
                    if line.is_empty() {
                        return Ok(());
                    }

                    if let Some(space_offset) = if line.starts_with('>') {
                        Some(0)
                    } else {
                        let spaces = line.chars().take_while(|c| c == &' ').count();

                        if spaces <= 3 {
                            Some(spaces)
                        } else {
                            None
                        }
                    } {
                        let mut chars = line.chars().skip(space_offset);

                        if chars.next().eq(&Some('>')) {
                            self.flush_current_container(State::BlockQuote);

                            return Ok(());
                        }
                    }

                    // https://spec.commonmark.org/0.31.2/#thematic-breaks (4.1)
                    if let Some(space_offset) =
                        if line.starts_with("-") || line.starts_with("*") || line.starts_with("_") {
                            Some(0)
                        } else {
                            let spaces = line.chars().take_while(|c| c == &' ').count();

                            if spaces <= 3 {
                                Some(spaces)
                            } else {
                                None
                            }
                        }
                    {
                        let chars = line.chars().skip(space_offset);
                        let mut chars = chars.filter(|c| !c.is_whitespace());

                        if chars.all(|c| c == '-')
                            || chars.all(|c| c == '*')
                            || chars.all(|c| c == '_')
                        {
                            self.close_block_token();
                            self.emit_token(Token::ThematicBreak);

                            return Ok(());
                        }
                    }

                    // https://spec.commonmark.org/0.31.2/#atx-headings (4.2)
                    if let Some(space_offset) = if line.starts_with('#') {
                        Some(0)
                    } else {
                        let spaces = line.chars().take_while(|c| c == &' ').count();

                        if spaces <= 3 {
                            Some(spaces)
                        } else {
                            None
                        }
                    } {
                        let start = space_offset;
                        let hash_count = line.chars().skip(start).take_while(|c| c == &'#').count();

                        if (1..=6).contains(&hash_count) {
                            if let Some(inline_content_offset) = if line.len() - start == hash_count
                            {
                                Some(0)
                            } else {
                                let next_char = line.chars().nth(start + hash_count);
                                if next_char == Some(' ') || next_char == Some('\t') {
                                    Some(1)
                                } else {
                                    None
                                }
                            } {
                                let start = start + hash_count + inline_content_offset;

                                self.close_block_token();
                                self.emit_token(Token::Heading {
                                    level: hash_count as u8,
                                    value: line.chars().skip(start).collect(),
                                });

                                return Ok(());
                            }
                        }
                    }

                    // https://spec.commonmark.org/0.31.2/#setext-headings (4.3)
                    // TODO with line buffer (setext heading is multiple lines)

                    // https://spec.commonmark.org/0.31.2/#indented-code-blocks (4.4)
                    // TODO line buffer

                    // https://spec.commonmark.org/0.31.2/#fenced-code-blocks (4.5)
                    // TODO line buffer

                    // https://spec.commonmark.org/0.31.2/#html-blocks (4.6)
                    // TODO line buffer

                    // https://spec.commonmark.org/0.31.2/#link-reference-definitions (4.7)
                    // TODO line buffer

                    // https://spec.commonmark.org/0.31.2/#paragraphs (4.8)
                    // TODO line buffer
                }
                None => {
                    self.state = State::Inline;
                    self.reconsume();
                }
            },
            // https://spec.commonmark.org/0.31.2/#block-quotes (5.1)
            State::BlockQuote | State::ContinuedBlockQuote => match self.consume_next_line() {
                Some(line) => {
                    let line = line.trim_end();

                    if line.is_empty() {
                        self.flush_current_container(State::Document);

                        return Ok(());
                    }

                    let preprended_block_token = self.get_preprended_block_token();

                    if let Some(Token::Paragraph(_)) = preprended_block_token {
                        self.append_block_token(Token::SoftBreak);
                        self.append_block_token(Token::Paragraph(line.to_string()));
                    } else if !line.starts_with('>') {
                        self.flush_current_container(State::Document);
                    } else {
                        // TODO add some other kind of token
                    }
                }
                None => self.flush_current_container(State::Inline),
            },
            // https://spec.commonmark.org/0.31.2/#list-items (5.2)
            State::ListItem | State::ContinuedListItem => match self.consume_next_char() {
                Some('\n') => self.state = State::ContinuedListItem,
                Some(c) => {}
                None => {
                    self.state = State::Inline;
                    self.reconsume();
                }
            },
            // https://spec.commonmark.org/0.31.2/#lists (5.3)
            State::List | State::ContinuedList => match self.consume_next_char() {
                Some('\n') => self.state = State::ContinuedList,
                Some(c) => {}
                None => {
                    self.state = State::Inline;
                    self.reconsume();
                }
            },
            // https://spec.commonmark.org/0.31.2/#phase-2-inline-structure
            State::Inline => {
                // https://spec.commonmark.org/0.31.2/#code-spans (6.1)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis (6.2)
                // TODO doing this last (the most technicalities)

                // https://spec.commonmark.org/0.31.2/#links (6.3)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#images (6.4)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#autolinks (6.5)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#raw-html (6.6)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#hard-line-breaks (6.7)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#soft-line-breaks (6.8)
                // TODO line buffer

                // https://spec.commonmark.org/0.31.2/#textual-content (6.9)
                // self.append_block_token(Token::Paragraph(line.to_string()));

                self.close_block_token();
                self.emit_token(Token::Eof);
            }
            _ => {}
        }

        Ok(())
    }

    #[inline(always)]
    fn skip_whitespaces(&mut self, c: char) {
        if c == '\r' && self.input.cur() == Some('\n') {
            unsafe {
                // Safety: cur() is Some
                self.input.bump();
            }
        }
    }
}

// By spec '\r` removed before tokenizer, but we keep them to have better AST
// and don't break logic to ignore characters
#[inline(always)]
fn is_spacy(c: char) -> bool {
    matches!(c, '\x09' | '\x0a' | '\x0d' | '\x0c' | '\x20')
}

#[inline(always)]
fn is_control(c: u32) -> bool {
    matches!(c, c @ 0x00..=0x1f | c @ 0x7f..=0x9f if !matches!(c, 0x09 | 0x0a | 0x0c | 0x0d | 0x20))
}

#[inline(always)]
fn is_surrogate(c: u32) -> bool {
    matches!(c, 0xd800..=0xdfff)
}

// A noncharacter is a code point that is in the range U+FDD0 to U+FDEF,
// inclusive, or U+FFFE, U+FFFF, U+1FFFE, U+1FFFF, U+2FFFE, U+2FFFF, U+3FFFE,
// U+3FFFF, U+4FFFE, U+4FFFF, U+5FFFE, U+5FFFF, U+6FFFE, U+6FFFF, U+7FFFE,
// U+7FFFF, U+8FFFE, U+8FFFF, U+9FFFE, U+9FFFF, U+AFFFE, U+AFFFF, U+BFFFE,
// U+BFFFF, U+CFFFE, U+CFFFF, U+DFFFE, U+DFFFF, U+EFFFE, U+EFFFF, U+FFFFE,
// U+FFFFF, U+10FFFE, or U+10FFFF.
#[inline(always)]
fn is_noncharacter(c: u32) -> bool {
    matches!(
        c,
        0xfdd0
            ..=0xfdef
                | 0xfffe
                | 0xffff
                | 0x1fffe
                | 0x1ffff
                | 0x2fffe
                | 0x2ffff
                | 0x3fffe
                | 0x3ffff
                | 0x4fffe
                | 0x4ffff
                | 0x5fffe
                | 0x5ffff
                | 0x6fffe
                | 0x6ffff
                | 0x7fffe
                | 0x7ffff
                | 0x8fffe
                | 0x8ffff
                | 0x9fffe
                | 0x9ffff
                | 0xafffe
                | 0xaffff
                | 0xbfffe
                | 0xbffff
                | 0xcfffe
                | 0xcffff
                | 0xdfffe
                | 0xdffff
                | 0xefffe
                | 0xeffff
                | 0xffffe
                | 0xfffff
                | 0x10fffe
                | 0x10ffff,
    )
}

#[inline(always)]
fn is_upper_hex_digit(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='F')
}

#[inline(always)]
fn is_lower_hex_digit(c: char) -> bool {
    matches!(c, '0'..='9' | 'a'..='f')
}

#[inline(always)]
fn is_ascii_hex_digit(c: char) -> bool {
    is_upper_hex_digit(c) || is_lower_hex_digit(c)
}

#[inline(always)]
fn is_ascii_upper_alpha(c: char) -> bool {
    c.is_ascii_uppercase()
}

#[inline(always)]
fn is_ascii_lower_alpha(c: char) -> bool {
    c.is_ascii_lowercase()
}

#[inline(always)]
fn is_ascii_alpha(c: char) -> bool {
    is_ascii_upper_alpha(c) || is_ascii_lower_alpha(c)
}

#[inline(always)]
fn is_allowed_control_character(c: u32) -> bool {
    c != 0x00 && is_control(c)
}

#[inline(always)]
fn is_allowed_character(c: char) -> bool {
    let c = c as u32;

    if is_surrogate(c) || is_allowed_control_character(c) || is_noncharacter(c) {
        return false;
    }

    return true;
}
