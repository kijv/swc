use std::{cell::RefCell, collections::VecDeque, mem::take, rc::Rc};

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

                // Validate code point
                let final_char = if code_point == 0 {
                    self.emit_error(ErrorKind::NullCharacterReference);
                    '\u{FFFD}'
                } else if code_point > 0x10ffff {
                    self.emit_error(ErrorKind::CharacterReferenceOutsideUnicodeRange);
                    '\u{FFFD}'
                } else if is_surrogate(code_point) {
                    self.emit_error(ErrorKind::SurrogateCharacterReference);
                    '\u{FFFD}'
                } else if is_noncharacter(code_point) {
                    self.emit_error(ErrorKind::NoncharacterCharacterReference);
                    char::from_u32(code_point).unwrap_or('\u{FFFD}')
                } else if is_control(code_point)
                    && !is_spacy(char::from_u32(code_point).unwrap_or('\0'))
                {
                    self.emit_error(ErrorKind::ControlCharacterReference);
                    // Map specific control characters
                    match code_point {
                        0x80 => '\u{20AC}',
                        0x82 => '\u{201A}',
                        0x83 => '\u{0192}',
                        0x84 => '\u{201E}',
                        0x85 => '\u{2026}',
                        0x86 => '\u{2020}',
                        0x87 => '\u{2021}',
                        0x88 => '\u{02C6}',
                        0x89 => '\u{2030}',
                        0x8a => '\u{0160}',
                        0x8b => '\u{2039}',
                        0x8c => '\u{0152}',
                        0x8e => '\u{017D}',
                        0x91 => '\u{2018}',
                        0x92 => '\u{2019}',
                        0x93 => '\u{201C}',
                        0x94 => '\u{201D}',
                        0x95 => '\u{2022}',
                        0x96 => '\u{2013}',
                        0x97 => '\u{2014}',
                        0x98 => '\u{02DC}',
                        0x99 => '\u{2122}',
                        0x9a => '\u{0161}',
                        0x9b => '\u{203A}',
                        0x9c => '\u{0153}',
                        0x9e => '\u{017E}',
                        0x9f => '\u{0178}',
                        _ => char::from_u32(code_point).unwrap_or('\u{FFFD}'),
                    }
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
            if c == ' ' || c == '\t' {
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
                ' ' => space_count += 1,
                '\t' => {
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
                    Some(' ') | Some('\t') => {
                        if self.at_line_start {
                            // At line start: treat as whitespace
                            // Set token start to beginning of whitespace
                            self.token_start_pos = BytePos(self.input.cur_pos().0 - 1);
                            if self.cur.unwrap() == ' ' {
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
                            self.buf.borrow_mut().push(self.cur.unwrap());
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
                    // U+0000 NULL: Error
                    Some('\x00') => {
                        self.emit_error(ErrorKind::UnexpectedNullCharacter);
                        self.emit_token(Token::Text(String::from('\u{FFFD}')));
                        self.at_line_start = false;
                    }
                    // EOF
                    None => {
                        self.emit_token(Token::Eof);
                        return Ok(());
                    }
                    // Anything else: Append to text buffer
                    Some(c) => {
                        self.validate_input_stream_character(c);
                        self.buf.borrow_mut().push(c);
                        self.state = State::Text;
                        self.at_line_start = false;
                    }
                }
            }
            State::Whitespace => {
                match self.consume_next_char() {
                    // Space: Increment count
                    Some(' ') => {
                        self.whitespace_count += 1;
                    }
                    // Tab: Emit spaces first, then tab
                    Some('\t') => {
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
                    Some('\n') | Some('\r') | Some('\\') | Some('&') => {
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
                    Some(c)
                        if matches!(
                            c,
                            '!' | '"'
                                | '#'
                                | '$'
                                | '%'
                                | '&'
                                | '\''
                                | '('
                                | ')'
                                | '*'
                                | '+'
                                | ','
                                | '-'
                                | '.'
                                | '/'
                                | ':'
                                | ';'
                                | '<'
                                | '='
                                | '>'
                                | '?'
                                | '@'
                                | '['
                                | '\\'
                                | ']'
                                | '^'
                                | '_'
                                | '`'
                                | '{'
                                | '|'
                                | '}'
                                | '~'
                        ) =>
                    {
                        self.emit_token(Token::BackslashEscape(c));
                        self.state = State::Data;
                        self.at_line_start = false;
                    }
                    // Newline: Hard break
                    Some('\n') | Some('\r') => {
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
                    Some(c @ (' ' | '\t')) => {
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
fn is_allowed_control_character(c: u32) -> bool {
    c != 0x00 && is_control(c)
}
