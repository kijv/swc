use std::{cell::RefCell, char::REPLACEMENT_CHARACTER, collections::VecDeque, mem::take, rc::Rc};

use swc_common::{input::Input, BytePos, Span};
use swc_html_utils::HTML_ENTITIES;

use self::token::{Token, TokenAndSpan};
use crate::{
    error::{Error, ErrorKind},
    parser::input::ParserInput,
};

pub mod token;

#[derive(Debug, Clone)]
pub enum State {
    Data,          // Initial state for general content
    Whitespace,    // Accumulating spaces/tabs
    Text,          // Building text token
    Escape,        // After backslash
    Entity,        // After '&'
    NamedEntity,   // Named entity accumulation
    NumericEntity, // After '&#'
    HexEntity,     // Hex numeric entity
    DecimalEntity, // Decimal numeric entity
    Marker,        // Potential block marker
}

pub(crate) type LexResult<T> = Result<T, ErrorKind>;

pub struct Lexer<'a, I>
where
    I: Input<'a>,
{
    input: I,
    cur: Option<char>,
    cur_pos: BytePos,
    token_start_pos: BytePos,
    finished: bool,
    state: State,
    return_state: State,
    errors: Vec<Error>,
    pending_tokens: VecDeque<TokenAndSpan>,
    buf: Rc<RefCell<String>>,
    whitespace_count: u32,
    character_reference_code: Option<Vec<(u8, u32, Option<char>)>>,
    temporary_buffer: String,
    at_line_start: bool,
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
            token_start_pos: start_pos,
            finished: false,
            state: State::Data,
            return_state: State::Data,
            errors: Vec::new(),
            pending_tokens: VecDeque::with_capacity(16),
            buf: Rc::new(RefCell::new(String::with_capacity(256))),
            whitespace_count: 0,
            character_reference_code: None,
            temporary_buffer: String::with_capacity(33),
            at_line_start: true, // Start at beginning of input
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

        token_and_span.ok()
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

    #[inline(always)]
    fn consume(&mut self) {
        self.cur = self.input.cur();
        self.cur_pos = self.input.cur_pos();

        if self.cur.is_some() {
            unsafe {
                // Safety: self.cur is Some()
                self.input.bump();
            }
        }
    }

    #[inline(always)]
    fn reconsume_in_state(&mut self, state: State) {
        self.state = state;
        unsafe {
            // Safety: self.cur_pos is valid position because we got it from self.input
            self.input.reset_to(self.cur_pos);
        }
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

    #[cold]
    fn emit_error(&mut self, kind: ErrorKind) {
        self.errors.push(Error::new(
            Span::new(self.cur_pos, self.input.cur_pos()),
            kind,
        ));
    }

    #[inline(always)]
    fn start_token(&mut self) {
        self.token_start_pos = self.input.cur_pos();
    }

    #[inline(always)]
    fn emit_token(&mut self, token: Token) {
        let span = Span::new(self.token_start_pos, self.input.cur_pos());
        self.pending_tokens.push_back(TokenAndSpan { span, token });
    }

    #[inline(always)]
    fn emit_token_with_span(&mut self, token: Token, start: BytePos, end: BytePos) {
        let span = Span::new(start, end);
        self.pending_tokens.push_back(TokenAndSpan { span, token });
    }

    fn validate_and_emit_numeric_entity(&mut self) {
        if let Some(ref codes) = self.character_reference_code {
            if let Some((_, code_point, _)) = codes.first() {
                let code_point = *code_point;

                // Simplified validation - replace NULL and handle out of range
                let final_char = if code_point == 0 {
                    '\u{FFFD}'
                } else if code_point > 0x10ffff {
                    self.emit_error(ErrorKind::CharacterReferenceOutsideUnicodeRange);
                    '\u{FFFD}'
                } else {
                    char::from_u32(code_point).unwrap_or('\u{FFFD}')
                };

                self.emit_token(Token::Entity(String::from(final_char)));
            }
        }
        self.character_reference_code = None;
    }

    fn split_trailing_whitespace(&self, text: &str) -> (String, String) {
        let mut non_ws_end = text.len();
        let mut ws_start = text.len();

        for (i, c) in text.char_indices().rev() {
            if is_space(c) || is_tab(c) {
                non_ws_end = i;
            } else {
                ws_start = non_ws_end;
                break;
            }
        }

        if ws_start == 0 {
            // All whitespace
            (String::new(), text.to_string())
        } else if ws_start == text.len() {
            // No trailing whitespace
            (text.to_string(), String::new())
        } else {
            // Mixed content
            (text[..ws_start].to_string(), text[ws_start..].to_string())
        }
    }

    fn emit_whitespace_tokens(&mut self, whitespace: &str, start_pos: BytePos) {
        let mut space_count = 0;
        let mut current_pos = start_pos;

        for c in whitespace.chars() {
            match c {
                c if is_space(c) => space_count += 1,
                c if is_tab(c) => {
                    // Emit accumulated spaces first
                    if space_count > 0 {
                        let space_end = BytePos(current_pos.0 + space_count);
                        self.emit_token_with_span(
                            Token::Space(space_count),
                            current_pos,
                            space_end,
                        );
                        current_pos = space_end;
                        space_count = 0;
                    }
                    let tab_end = BytePos(current_pos.0 + 1);
                    self.emit_token_with_span(Token::Tab, current_pos, tab_end);
                    current_pos = tab_end;
                }
                _ => {} // Ignore non-whitespace (shouldn't happen)
            }
        }

        // Emit remaining spaces
        if space_count > 0 {
            let space_end = BytePos(current_pos.0 + space_count);
            self.emit_token_with_span(Token::Space(space_count), current_pos, space_end);
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

                Err(ErrorKind::Eof)
            }
            _ => Ok(token_and_span),
        }
    }

    fn run(&mut self) -> LexResult<()> {
        match self.state {
            State::Data => {
                self.start_token();
                // Consume the next input character:
                match self.consume_next_char() {
                    // Space or tab: Handle differently based on line position
                    Some(c) if is_space(c) || is_tab(c) => {
                        if self.at_line_start {
                            // At line start: treat as whitespace
                            // Set token start to beginning of whitespace
                            self.token_start_pos = BytePos(self.input.cur_pos().0 - 1);
                            if is_space(c) {
                                self.whitespace_count = 1;
                            } else {
                                // Tab: emit immediately
                                self.emit_token(Token::Tab);
                                self.at_line_start = false;
                                return Ok(());
                            }
                            self.state = State::Whitespace;
                        } else {
                            // Mid-line: add to text buffer
                            self.buf.borrow_mut().push(c);
                            self.state = State::Text;
                        }
                    }
                    // '\n' or '\r': Normalize and emit Newline
                    Some('\n') => {
                        self.emit_token(Token::Newline);
                        self.at_line_start = true;
                    }
                    Some('\r') => {
                        // Normalize \r\n or \r to \n
                        if self.input.cur() == Some('\n') {
                            self.consume();
                        }
                        self.emit_token(Token::Newline);
                        self.at_line_start = true;
                    }
                    // '\\': Switch to EscapeState
                    Some('\\') => {
                        self.state = State::Escape;
                        self.at_line_start = false;
                    }
                    // '&': Switch to EntityState
                    Some('&') => {
                        self.return_state = State::Data;
                        self.state = State::Entity;
                        self.at_line_start = false;
                    }
                    // Potential markers
                    Some(
                        c @ ('#' | '>' | '-' | '*' | '_' | '`' | '~' | '[' | '!' | '<' | '=' | '|'),
                    ) => {
                        self.temporary_buffer.clear();
                        self.temporary_buffer.push(c);
                        self.state = State::Marker;
                        self.at_line_start = false;
                    }
                    // U+0000 NULL: Replace with replacement character
                    Some('\x00') => {
                        self.emit_token(Token::Text(String::from(REPLACEMENT_CHARACTER)));
                        self.at_line_start = false;
                    }
                    // EOF
                    None => {
                        self.emit_token(Token::Eof);
                        return Ok(());
                    }
                    // Anything else: Append to text buffer
                    Some(c) => {
                        self.buf.borrow_mut().push(c);
                        self.state = State::Text;
                        self.at_line_start = false;
                    }
                }
            }
            State::Whitespace => {
                match self.consume_next_char() {
                    // Space: Increment count
                    Some(c) if is_space(c) => {
                        self.whitespace_count += 1;
                    }
                    // Tab: Emit spaces first, then tab
                    Some(c) if is_tab(c) => {
                        if self.whitespace_count > 0 {
                            let space_start = BytePos(self.token_start_pos.0);
                            let space_end = BytePos(space_start.0 + self.whitespace_count);
                            self.emit_token_with_span(
                                Token::Space(self.whitespace_count),
                                space_start,
                                space_end,
                            );
                            self.whitespace_count = 0;
                        }
                        let tab_start = BytePos(self.input.cur_pos().0 - 1);
                        self.emit_token_with_span(Token::Tab, tab_start, self.input.cur_pos());
                        // Continue in whitespace state to collect more
                    }
                    // Else: Emit remaining spaces and switch to appropriate state
                    _ => {
                        if self.whitespace_count > 0 {
                            let space_end = BytePos(self.token_start_pos.0 + self.whitespace_count);
                            self.emit_token_with_span(
                                Token::Space(self.whitespace_count),
                                self.token_start_pos,
                                space_end,
                            );
                            self.whitespace_count = 0;
                        }
                        // No longer at line start after processing whitespace
                        self.at_line_start = false;
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::Text => {
                match self.consume_next_char() {
                    // Markers: Check if we need to tokenize preceding whitespace
                    Some(
                        c @ ('#' | '>' | '-' | '*' | '_' | '`' | '~' | '[' | '!' | '<' | '=' | '|'),
                    ) => {
                        let text = self.buf.borrow().clone();
                        if !text.is_empty() {
                            // Check if text ends with whitespace - if so, separate it
                            let (non_ws_text, whitespace) = self.split_trailing_whitespace(&text);

                            if !non_ws_text.is_empty() {
                                let text_end = if whitespace.is_empty() {
                                    BytePos(self.input.cur_pos().0 - 1)
                                } else {
                                    BytePos(self.token_start_pos.0 + non_ws_text.len() as u32)
                                };
                                self.emit_token_with_span(
                                    Token::Text(non_ws_text),
                                    self.token_start_pos,
                                    text_end,
                                );
                            }

                            // Emit whitespace tokens if any
                            if !whitespace.is_empty() {
                                let ws_start = BytePos(
                                    self.token_start_pos.0 + (text.len() - whitespace.len()) as u32,
                                );
                                self.emit_whitespace_tokens(&whitespace, ws_start);
                            }

                            self.buf.borrow_mut().clear();
                        }

                        // Now handle the marker
                        self.temporary_buffer.clear();
                        self.temporary_buffer.push(c);
                        self.start_token();
                        // Move back one character to include the marker in the token span
                        self.token_start_pos = BytePos(self.input.cur_pos().0 - 1);
                        self.state = State::Marker;
                        self.at_line_start = false;
                    }
                    // Other special characters: Emit Text and reconsume
                    Some(c) if is_line_ending(c, self.input.cur()) || c == '\\' || c == '&' => {
                        let text = self.buf.borrow().clone();
                        if !text.is_empty() {
                            let text_end = BytePos(self.input.cur_pos().0 - 1);
                            self.emit_token_with_span(
                                Token::Text(text),
                                self.token_start_pos,
                                text_end,
                            );
                            self.buf.borrow_mut().clear();
                        }
                        self.reconsume_in_state(State::Data);
                    }
                    // EOF: Emit Text, then EOF
                    None => {
                        let text = self.buf.borrow().clone();
                        if !text.is_empty() {
                            self.emit_token_with_span(
                                Token::Text(text),
                                self.token_start_pos,
                                self.input.cur_pos(),
                            );
                            self.buf.borrow_mut().clear();
                        }
                        self.start_token();
                        self.emit_token(Token::Eof);
                        return Ok(());
                    }
                    // Non-special char: Append to buffer
                    Some(c) => {
                        self.buf.borrow_mut().push(c);
                    }
                }
            }
            State::Escape => {
                match self.consume_next_char() {
                    // Escapable punctuation characters (per CommonMark 2.4)
                    Some(c) if is_ascii_punctuation_character(c) => {
                        self.emit_token(Token::BackslashEscape(c));
                        self.state = State::Data;
                        self.at_line_start = false;
                    }
                    // Newline: Hard break
                    Some(c) if is_line_ending(c, self.input.cur()) => {
                        self.emit_token(Token::BackslashEscape('\n'));
                        self.state = State::Data;
                        self.at_line_start = true;
                    }
                    // EOF: Emit '\\'
                    None => {
                        self.emit_token(Token::Text(String::from("\\")));
                        self.start_token();
                        self.emit_token(Token::Eof);
                        return Ok(());
                    }
                    // Else: Emit '\\' + char
                    Some(_c) => {
                        self.emit_token(Token::Text(String::from("\\")));
                        self.reconsume_in_state(State::Data);
                        self.at_line_start = false;
                    }
                }
            }
            State::Entity => {
                match self.consume_next_char() {
                    // Alphanumeric: Buffer name
                    Some(c) if c.is_alphanumeric() => {
                        self.temporary_buffer.clear();
                        self.temporary_buffer.push(c);
                        self.state = State::NamedEntity;
                    }
                    // '#': Numeric entity
                    Some('#') => {
                        self.state = State::NumericEntity;
                    }
                    // Else: Emit '&', reconsume
                    _ => {
                        self.emit_token(Token::Text(String::from("&")));
                        self.reconsume_in_state(self.return_state.clone());
                    }
                }
            }
            State::NamedEntity => {
                match self.consume_next_char() {
                    // Continue entity name
                    Some(c) if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':') => {
                        self.temporary_buffer.push(c);
                    }
                    // ';': Try to resolve entity
                    Some(';') => {
                        let entity_name = self.temporary_buffer.clone();
                        // Look up in HTML entities
                        if let Some(entity) = HTML_ENTITIES.get(entity_name.as_str()) {
                            self.emit_token(Token::Entity(entity.characters.to_string()));
                        } else {
                            // Unknown entity: emit as text
                            self.emit_error(ErrorKind::UnknownNamedCharacterReference);
                            self.emit_token(Token::Text(format!("&{entity_name};")));
                        }
                        self.state = self.return_state.clone();
                    }
                    // Else: Missing semicolon error
                    _ => {
                        self.emit_error(ErrorKind::MissingSemicolonAfterCharacterReference);
                        self.emit_token(Token::Text(format!("&{}", self.temporary_buffer)));
                        self.reconsume_in_state(self.return_state.clone());
                    }
                }
            }
            State::NumericEntity => {
                match self.consume_next_char() {
                    // 'x' or 'X': Hex mode
                    Some('x') | Some('X') => {
                        self.character_reference_code = Some(vec![]);
                        self.state = State::HexEntity;
                    }
                    // Digit: Decimal mode
                    Some(c) if c.is_ascii_digit() => {
                        self.character_reference_code =
                            Some(vec![(0, c.to_digit(10).unwrap(), None)]);
                        self.state = State::DecimalEntity;
                    }
                    // Else: Error
                    _ => {
                        self.emit_error(ErrorKind::AbsenceOfDigitsInNumericCharacterReference);
                        self.reconsume_in_state(self.return_state.clone());
                    }
                }
            }
            State::HexEntity => {
                match self.consume_next_char() {
                    // Hex digit: Accumulate
                    Some(c) if c.is_ascii_hexdigit() => {
                        if let Some(ref mut codes) = self.character_reference_code {
                            let val = c.to_digit(16).unwrap();
                            if codes.is_empty() {
                                codes.push((0, val, None));
                            } else {
                                codes[0].1 = codes[0].1 * 16 + val;
                            }
                        }
                    }
                    // ';': Validate and emit
                    Some(';') => {
                        self.validate_and_emit_numeric_entity();
                        self.state = self.return_state.clone();
                    }
                    // Else: Missing semicolon
                    _ => {
                        self.emit_error(ErrorKind::MissingSemicolonAfterCharacterReference);
                        self.validate_and_emit_numeric_entity();
                        self.reconsume_in_state(self.return_state.clone());
                    }
                }
            }
            State::DecimalEntity => {
                match self.consume_next_char() {
                    // Decimal digit: Accumulate
                    Some(c) if c.is_ascii_digit() => {
                        if let Some(ref mut codes) = self.character_reference_code {
                            let val = c.to_digit(10).unwrap();
                            codes[0].1 = codes[0].1 * 10 + val;
                        }
                    }
                    // ';': Validate and emit
                    Some(';') => {
                        self.validate_and_emit_numeric_entity();
                        self.state = self.return_state.clone();
                    }
                    // Else: Missing semicolon
                    _ => {
                        self.emit_error(ErrorKind::MissingSemicolonAfterCharacterReference);
                        self.validate_and_emit_numeric_entity();
                        self.reconsume_in_state(self.return_state.clone());
                    }
                }
            }
            State::Marker => {
                match self.consume_next_char() {
                    // Continue marker sequence
                    Some(c)
                        if !self.temporary_buffer.is_empty()
                            && c == self.temporary_buffer.chars().next().unwrap() =>
                    {
                        self.temporary_buffer.push(c);
                    }
                    // Whitespace after marker: emit marker, then whitespace tokens
                    Some(c) if is_space(c) || is_tab(c) => {
                        let marker_char = self.temporary_buffer.chars().next().unwrap();
                        let count = self.temporary_buffer.len() as u32;
                        let marker_end = BytePos(self.token_start_pos.0 + count);
                        self.emit_token_with_span(
                            Token::Marker(marker_char, count),
                            self.token_start_pos,
                            marker_end,
                        );

                        // Now handle whitespace after marker
                        let whitespace = String::from(c);
                        let ws_start = BytePos(self.input.cur_pos().0 - 1);
                        self.emit_whitespace_tokens(&whitespace, ws_start);
                        self.whitespace_count = 0; // Reset for further whitespace collection
                        self.state = State::Whitespace; // Continue collecting whitespace
                        self.at_line_start = false;
                    }
                    // Complete marker or switch to different handling
                    _ => {
                        let marker_char = self.temporary_buffer.chars().next().unwrap();
                        let count = self.temporary_buffer.len() as u32;
                        let marker_end = BytePos(self.token_start_pos.0 + count);
                        self.emit_token_with_span(
                            Token::Marker(marker_char, count),
                            self.token_start_pos,
                            marker_end,
                        );
                        self.reconsume_in_state(State::Data);
                        self.at_line_start = false;
                    }
                }
            }
        }

        Ok(())
    }
}

// A line ending is a line feed (U+000A), a carriage return (U+000D) not
// followed by a line feed, or a carriage return and a following line feed.
#[inline(always)]
fn is_line_ending(c: char, next: Option<char>) -> bool {
    (c == '\n' || c == '\r' && next != Some('\n')) || c == '\r' && next == Some('\n')
}

// A line containing no characters, or a line containing only spaces (U+0020) or
// tabs (U+0009), is called a blank line.
#[inline(always)]
fn is_blank_line(line: &str) -> bool {
    line.trim().is_empty()
}

// A Unicode whitespace character is a character in the Unicode Zs general
// category, or a tab (U+0009), line feed (U+000A), form feed (U+000C), or
// carriage return (U+000D).
#[inline(always)]
fn is_unicode_whitespace_character(c: char) -> bool {
    matches!(c, '\x09' | '\x0a' | '\x0c' | '\x0d' | '\x20')
}

// Unicode whitespace is a sequence of one or more Unicode whitespace
// characters.
#[inline(always)]
fn is_unicode_whitespace(line: &str) -> bool {
    line.chars().all(|c| is_unicode_whitespace_character(c))
}

// A tab is U+0009.
#[inline(always)]
fn is_tab(c: char) -> bool {
    c == '\x09'
}

// A space is U+0020.
#[inline(always)]
fn is_space(c: char) -> bool {
    c == '\x20'
}

// An ASCII control character is a character between U+0000–1F (both including)
// or U+007F.
#[inline(always)]
fn is_ascii_control_character(c: char) -> bool {
    matches!(c, '\x00'..='\x1f' | '\x7f')
}

// An ASCII punctuation character is !, ", #, $, %, &, ', (, ), *, +, ,, -, ., /
// (U+0021–2F), :, ;, <, =, >, ?, @ (U+003A–0040), [, \, ], ^, _, `
// (U+005B–0060), {, |, }, or ~ (U+007B–007E).
#[inline(always)]
fn is_ascii_punctuation_character(c: char) -> bool {
    matches!(c, '\x21'..='\x2f' | '\x3a'..='\x40' | '\x5b'..='\x60' | '\x7b'..='\x7e')
}

// A Unicode punctuation character is a character in the Unicode P (punctuation)
// or S (symbol) general categories.
#[inline(always)]
fn is_unicode_punctuation_character(c: char) -> bool {
    matches!(c, '\x21'..='\x2f' | '\x3a'..='\x40' | '\x5b'..='\x60' | '\x7b'..='\x7e')
}
