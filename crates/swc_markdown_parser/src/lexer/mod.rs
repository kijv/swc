use std::{cell::RefCell, char::REPLACEMENT_CHARACTER, collections::VecDeque, mem::take, rc::Rc};

use swc_common::{input::Input, BytePos, Span};

use self::token::{Token, TokenAndSpan};
use crate::{
    error::{Error, ErrorKind},
    parser::input::ParserInput,
};

pub mod token;

#[derive(Debug, Clone)]
pub enum State {
    Data,
    ThematicBreak,
    ATXHeading,
    SetextHeading,
    IndentedCodeBlock,
    IndentedCodeChunk,
    FencedCodeBlock,
    HTMLBlock,
    HTMLTagName,
    BeforeHTMLBlockType1,
    HTMLBlockType1,
    HTMLDeclarationOpen,
    HTMLComment,
    HTMLProcessingInstruction,
    HTMLDeclaration,
    HTMLCDATASection,
    HTMLBlockType6,
    HTMLOpenTagName,
    HTMLClosingTagName,
    HTMLClosingTag,
    HTMLBlankLine,
    HTMLAttributes,
    AfterHTMLAttributes,
    HTMLAttribute,
    HTMLAttributeName,
    HTMLAttributeValueSpecification,
    HTMLAttributeValue,
    HTMLUnquotedAttribute,
    HTMLSingleQuotedAttribute,
    HTMLDoubleQuotedAttribute,
    LinkLabel,
    AfterLinkLabel,
    LinkDestination,
    BracedLinkDestination,
    UnquotedLinkDestination,
    BalancedParenthesisPairState,
    BeforeLinkTitle,
    DoubleQuotedLinkTitle,
    SingleQuotedLinkTitle,
    ParentheticalLinkTitle,
    AfterLinkTitle,
}

pub(crate) type LexResult<T> = Result<T, ErrorKind>;

pub struct Lexer<I> {
    input: I,
    cur: Option<char>,
    cur_pos: BytePos,
    last_token_pos: BytePos,
    finished: bool,
    state: State,
    errors: Vec<Error>,
    pending_tokens: VecDeque<TokenAndSpan>,
    buf: Rc<RefCell<String>>,
    sub_buf: Rc<RefCell<String>>,
    temporary_buffer: String,
    line_ending_count: u32,
}

impl<'a, I> Lexer<I>
where
    I: Input<'a>,
{
    pub fn new(input: I) -> Self {
        let start_pos = input.last_pos();

        let mut lexer = Lexer {
            input,
            cur: None,
            cur_pos: start_pos,
            last_token_pos: start_pos,
            finished: false,
            state: State::Data,
            errors: Vec::new(),
            pending_tokens: VecDeque::with_capacity(16),
            buf: Rc::new(RefCell::new(String::with_capacity(256))),
            sub_buf: Rc::new(RefCell::new(String::with_capacity(256))),
            temporary_buffer: String::with_capacity(33),
            line_ending_count: 0,
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

impl<'a, I: Input<'a>> Iterator for Lexer<I> {
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

impl<'a, I> ParserInput for Lexer<I>
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

impl<'a, I> Lexer<I>
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

    // fn consume_and_append_to_doctype_token_name<F>(&mut self, c: char, f: F)
    // where
    //     F: Fn(char) -> bool,
    // {
    //     let b = self.buf.clone();
    //     let mut buf = b.borrow_mut();
    //     let b = self.sub_buf.clone();
    //     let mut sub_buf = b.borrow_mut();

    //     buf.push(c.to_ascii_lowercase());
    //     sub_buf.push(c);

    //     let value = self.input.uncons_while(f);

    //     buf.push_str(&value.to_ascii_lowercase());
    //     sub_buf.push_str(value);
    // }

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
        match self.state {
            State::Data => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+003D EQUALS SIGN (=)
                    // U+002D HYPHEN-MINUS (-)
                    Some(c) if c == '=' || c == '-' => {
                        if self.temporary_buffer.is_empty() {
                            self.state = State::ThematicBreak;
                        } else {
                            self.state = State::SetextHeading;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+002A ASTERISK (*)
                    // U+005F LOW LINE (_)
                    Some(c) if c == '*' || c == '_' => {
                        if self.temporary_buffer.len() == 2
                            && self.temporary_buffer.chars().all(|buf_c| buf_c == c)
                        {
                            self.state = State::ThematicBreak;
                        } else {
                            let leading_space_count = self
                                .temporary_buffer
                                .chars()
                                .take_while(|c| *c == '\x20')
                                .count();

                            if (0..=3).contains(&leading_space_count) && {
                                let remaining_temporary_buffer = self
                                    .temporary_buffer
                                    .chars()
                                    .skip(leading_space_count)
                                    .collect::<String>();

                                !remaining_temporary_buffer.is_empty()
                                    && remaining_temporary_buffer.chars().all(|buf_c| buf_c == c)
                            } {
                                self.state = State::ThematicBreak;
                            }
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+0020 SPACE
                    Some(c) if c == '\x20' => {
                        if self.temporary_buffer == "\x20".repeat(3) {
                            self.state = State::IndentedCodeChunk;
                            self.temporary_buffer.clear();
                        } else {
                            let leading_space_count = self
                                .temporary_buffer
                                .chars()
                                .take_while(|c| *c == '\x20')
                                .count();

                            let remaining_temporary_buffer = self
                                .temporary_buffer
                                .chars()
                                .skip(leading_space_count)
                                .collect::<String>();

                            if (0..=3).contains(&leading_space_count)
                                && !remaining_temporary_buffer.is_empty()
                                && remaining_temporary_buffer.chars().all(|c| c == '#')
                            {
                                self.state = State::ATXHeading;
                                self.temporary_buffer = remaining_temporary_buffer;
                            } else {
                                self.temporary_buffer.push(c);
                            }
                        }
                    }
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x09' => {
                        if self.temporary_buffer.is_empty() {
                            self.state = State::IndentedCodeBlock;
                            self.temporary_buffer.clear();
                        } else {
                            let leading_space_count = self
                                .temporary_buffer
                                .chars()
                                .take_while(|c| *c == '\x20')
                                .count();
                            let remaining_temporary_buffer = &self.temporary_buffer;

                            if (1..=3).contains(&leading_space_count) {
                                if remaining_temporary_buffer.chars().all(|c| c == '#') {
                                    self.state = State::ATXHeading;
                                }
                            } else {
                                self.temporary_buffer.push(c);
                            }
                        }
                    }
                    // U+0060 GRAVE ACCENT (`)
                    // U+007E TILDE (~)
                    Some(c) if c == '`' || c == '~' => {
                        if self.temporary_buffer.len() == 2
                            && self.temporary_buffer.chars().all(|buf_c| buf_c == c)
                        {
                            self.state = State::FencedCodeBlock;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+005B LEFT SQUARE BRACKET ([)
                    Some(c) if c == '[' => {
                        if (0..=3).contains(&self.temporary_buffer.len())
                            && (0..=3).contains(
                                &self
                                    .temporary_buffer
                                    .chars()
                                    .take_while(|c| *c == '\x20')
                                    .count(),
                            )
                        {
                            self.state = State::LinkLabel;
                            self.line_ending_count = 0;
                            self.temporary_buffer.clear();
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '>' => {
                        if (0..=3).contains(&self.temporary_buffer.len())
                            && self.temporary_buffer.chars().all(|c| c == '\x20')
                        {
                            self.emit_token(Token::BlockQuoteStart);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        if self.temporary_buffer.ends_with('\n') {
                            self.emit_token(Token::Paragraph(self.temporary_buffer.to_owned()));
                            self.emit_token(Token::BlankLine); // TODO do this better
                            self.temporary_buffer.clear();
                        } else if !self.temporary_buffer.is_empty() {
                            self.temporary_buffer.push(c);
                        } else {
                            self.last_token_pos = self.input.cur_pos();
                        }
                    }
                    // U+0000 NULL
                    Some('\x00') => self.temporary_buffer.push(REPLACEMENT_CHARACTER),
                    // EOF
                    None => {
                        if !self.temporary_buffer.is_empty() {
                            self.emit_token(Token::Paragraph(self.temporary_buffer.to_owned()));
                            self.temporary_buffer.clear();
                        }

                        self.emit_token(Token::Eof);

                        return Ok(());
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::ThematicBreak => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    // EOF
                    Some('\x0a') | Some('\x0d') | None => {
                        dbg!(&self.temporary_buffer);
                        self.reconsume_in_state(State::Data);
                        self.emit_token(Token::ThematicBreak);
                        self.temporary_buffer.clear();
                    }
                    // Unicode whitespace
                    Some(c) if is_spacy(c) => {
                        // Ignore the character.
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.chars().any(|buf_c| buf_c == c) {
                            // Ignore the character.
                        } else {
                            self.reconsume_in_state(State::Data);
                        }
                    }
                }
            }
            State::ATXHeading => {
                // Consume the maximum amount of characters possible, until a
                // U+000A LINE FEED (LF) character is the current input
                // character. Append each character to the temporary buffer when
                // it’s consumed.
                let mut s = self.temporary_buffer.clone();

                while let Some(c) = self.consume_next_char() {
                    if is_line_ending(c) {
                        break;
                    }

                    s.push(c);
                }

                let closing_hashes_count = s.chars().rev().take_while(|c| *c == '#').count();

                if (1..=6).contains(&closing_hashes_count) {
                    // Remove the matched characters from the temporary buffer.
                    s.truncate(self.temporary_buffer.len() - closing_hashes_count);
                }

                let level = s.chars().take(6).take_while(|c| *c == '#').count();

                s = s.chars().skip_while(|c| *c == '#').collect::<String>();

                self.emit_token(Token::ATXHeading {
                    level: level as u8,
                    content: s,
                });

                self.state = State::Data;

                self.temporary_buffer.clear();

                return Ok(());
            }
            State::SetextHeading => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+000A LINE FEED (LF)
                    // EOF
                    Some('\x0a') | None => {
                        let level = if self.temporary_buffer.chars().next().unwrap() == '=' {
                            1
                        } else {
                            2
                        };

                        self.emit_token(Token::SetextHeading {
                            level,
                            content: self.temporary_buffer.to_owned(),
                        });
                    }
                    // Unicode whitespace
                    Some(c) if is_spacy(c) => {
                        self.state = State::ThematicBreak;
                    }
                    // Anything else
                    Some(c) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::IndentedCodeBlock => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    Some(c) if c == '\x20' => {
                        if self.temporary_buffer == "\x20".repeat(3) {
                            self.state = State::IndentedCodeChunk;
                            self.temporary_buffer.clear();
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x09' => {
                        if self.temporary_buffer.is_empty() {
                            self.state = State::IndentedCodeChunk;
                        }
                    }
                    // Anything else
                    None | Some(_) => {
                        if !self.temporary_buffer.is_empty() {
                            self.emit_token(Token::IndentedCodeBlock(
                                self.temporary_buffer.to_owned(),
                            ));
                            self.temporary_buffer.clear();
                        }

                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::IndentedCodeChunk => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+000A LINE FEED (LF)
                    Some(c) if c == '\x0a' => {
                        self.state = State::IndentedCodeBlock;
                        self.temporary_buffer.push(c);
                    }
                    None => {
                        if !self.temporary_buffer.is_empty() {
                            self.emit_token(Token::IndentedCodeBlock(
                                self.temporary_buffer.to_owned(),
                            ));
                            self.temporary_buffer.clear();
                        }
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::FencedCodeBlock => {
                let first_char = self.temporary_buffer.chars().next().unwrap();
                let opening_fence_length = self
                    .temporary_buffer
                    .chars()
                    .take_while(|buf_c| *buf_c == first_char)
                    .count();

                let mut has_closing_fence = false;

                while let Some(c) = self.consume_next_char() {
                    if is_line_ending(c) {
                        self.skip_whitespaces(c);

                        if let Some(last_line) = self.temporary_buffer.lines().last() {
                            let last_line = last_line.trim_end();
                            let last_line_leading_space_count =
                                last_line.chars().take_while(|c| *c == '\x20').count();

                            if (0..=3).contains(&last_line_leading_space_count) && {
                                let closing_fence_length = self
                                    .temporary_buffer
                                    .chars()
                                    .rev()
                                    .take_while(|buf_c| *buf_c == first_char)
                                    .count();

                                opening_fence_length == closing_fence_length
                            } {
                                has_closing_fence = true;
                                break;
                            }
                        }
                    }

                    self.temporary_buffer.push(c);
                }

                let info = self
                    .temporary_buffer
                    .lines()
                    .next()
                    .unwrap()
                    .chars()
                    .skip(opening_fence_length)
                    .collect::<String>()
                    .trim()
                    .to_owned();

                // content is all lines except the first and last
                let content = self
                    .temporary_buffer
                    .lines()
                    .skip(1)
                    .take(self.temporary_buffer.lines().count() - 2)
                    .collect::<Vec<_>>()
                    .join("\n");

                self.emit_token(Token::FencedCodeBlock { info, content });

                if has_closing_fence {
                    self.state = State::Data;
                } else {
                    self.reconsume_in_state(State::Data);
                }

                self.temporary_buffer.clear();
            }
            State::HTMLBlock => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0021 EXCLAMATION MARK (!)
                    Some(c) if c == '!' => {
                        self.state = State::HTMLDeclarationOpen;
                        self.temporary_buffer.push(c);
                    }
                    // U+003F QUESTION MARK (?)
                    Some(c) if c == '?' => {
                        if self.temporary_buffer == "<" {
                            self.state = State::HTMLProcessingInstruction;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // ASCII alpha
                    Some(c) if is_ascii_alpha(c) => {
                        if self.temporary_buffer.chars().next() == Some('<') {
                            self.reconsume_in_state(State::HTMLTagName);
                        }
                        self.temporary_buffer.push(c);
                    }
                    // U+002F SOLIDUS (/)
                    Some(c) if c == '/' => {
                        if self.temporary_buffer == "<" {
                            self.state = State::HTMLTagName;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::HTMLTagName => {
                // Consume the maximum amount of characters possible, until the
                // next input character does not match an ASCII alpha. Append
                // each character to the temporary buffer when it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    if !is_ascii_alpha(c) {
                        self.reconsume();
                        break;
                    }

                    s.push(c);
                }

                if ["pre", "script", "style", "textarea"].contains(&s.to_lowercase().as_str()) {
                    if self.temporary_buffer.starts_with("</") {
                        self.state = State::HTMLClosingTagName;
                    } else {
                        self.state = State::BeforeHTMLBlockType1;
                    }
                } else if [
                    "address",
                    "article",
                    "aside",
                    "base",
                    "basefont",
                    "blockquote",
                    "body",
                    "caption",
                    "center",
                    "col",
                    "colgroup",
                    "dd",
                    "details",
                    "dialog",
                    "dir",
                    "div",
                    "dl",
                    "dt",
                    "fieldset",
                    "figcaption",
                    "figure",
                    "footer",
                    "form",
                    "frame",
                    "frameset",
                    "h1",
                    "h2",
                    "h3",
                    "h4",
                    "h5",
                    "h6",
                    "head",
                    "header",
                    "hr",
                    "html",
                    "iframe",
                    "legend",
                    "li",
                    "link",
                    "main",
                    "menu",
                    "menuitem",
                    "nav",
                    "noframes",
                    "ol",
                    "optgroup",
                    "option",
                    "p",
                    "param",
                    "section",
                    "source",
                    "summary",
                    "table",
                    "tbody",
                    "td",
                    "tfoot",
                    "th",
                    "thead",
                    "title",
                    "tr",
                    "track",
                    "ul",
                ]
                .contains(&s.to_lowercase().as_str())
                {
                    self.state = State::HTMLBlockType6;
                } else {
                    if self.temporary_buffer.starts_with("</") {
                        self.state = State::HTMLClosingTagName;
                    } else {
                        self.state = State::HTMLOpenTagName;
                    }
                }
            }
            State::BeforeHTMLBlockType1 => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if c == '\x20' || c == '\x09' || is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        self.state = State::HTMLBlockType1;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                        self.temporary_buffer.clear();

                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.reconsume_in_state(State::HTMLBlock);
                    }
                }
            }
            State::HTMLBlockType1 => {
                // Consume the maximum amount of characters possible, until the
                // current input character matches the U+003E GREATER-THAN SIGN
                // character. Append each character to the temporary buffer when
                // it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    s.push(c);

                    if c == '>' {
                        break;
                    }
                }

                // The last characters match the U+003C LESS-THAN SIGN character
                // with the U+002F SOLIDUS, tag name—the case-insensitive ASCII
                // “pre”, “script”, “style”, or “textarea”, and the U+003E
                // GREATER-THAN SIGN character after
                if s.chars().last() == Some('>') && {
                    let tag_name = s
                        .chars()
                        .rev()
                        .skip(1)
                        .take_while(|c| is_ascii_alpha(*c))
                        .collect::<String>();

                    ["pre", "script", "style", "textarea"]
                        .contains(&tag_name.to_lowercase().as_str())
                        && {
                            // Get the two characters before the tag name
                            let before_tag_name = s
                                .chars()
                                .rev()
                                .skip(tag_name.len() + 1)
                                .take(2)
                                .collect::<String>();

                            before_tag_name == "</"
                        }
                } {
                    self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                    self.state = State::Data;

                    self.temporary_buffer.clear();
                } else {
                    self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                    self.reconsume_in_state(State::Data);

                    self.temporary_buffer.clear();
                }
            }
            State::HTMLDeclarationOpen => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+002D HYPHEN-MINUS (-)
                    Some(c) if c == '-' => {
                        if self.temporary_buffer == "<!" {
                            self.state = State::HTMLComment;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // ASCII alpha
                    Some(c) if is_ascii_alpha(c) => {
                        if self.temporary_buffer == "<!" {
                            self.state = State::HTMLDeclaration;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+005B LEFT SQUARE BRACKET ([)
                    Some(c) if c == '[' => {
                        if self.temporary_buffer == "<![CDATA" {
                            self.state = State::HTMLCDATASection;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLDeclaration => {
                // Consume the maximum amount of characters possible, until the
                // current input character matches the U+003E GREATER-THAN SIGN
                // character. Append each character to the temporary buffer when
                // it’s consumed.
                while let Some(c) = self.consume_next_char() {
                    self.temporary_buffer.push(c);

                    if c == '>' {
                        break;
                    }
                }

                self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                if self.temporary_buffer.ends_with(">") {
                    self.state = State::Data;
                } else {
                    self.reconsume_in_state(State::Data);
                }
            }
            State::HTMLComment => {
                // Consume the maximum amount of characters possible, until the current input
                // character matches the U+003E GREATER-THAN SIGN character. Append each
                // character to the temporary buffer when it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    s.push(c);

                    if c == '>' {
                        break;
                    }
                }

                self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                if s.len() >= "<!--".len() && s.ends_with("-->") {
                    self.state = State::Data;
                } else {
                    self.reconsume_in_state(State::Data);
                }

                self.temporary_buffer.clear();
            }
            State::HTMLProcessingInstruction => {
                // Consume the maximum amount of characters possible, until the current input
                // character matches the U+003E GREATER-THAN SIGN character. Append each
                // character to the temporary buffer when it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    s.push(c);

                    if c == '>' {
                        break;
                    }
                }

                self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                if s.len() >= "<?".len() && s.ends_with("?>") {
                    self.state = State::Data;
                } else {
                    self.reconsume_in_state(State::Data);
                }

                self.temporary_buffer.clear();
            }
            State::HTMLCDATASection => {
                // Consume the maximum amount of characters possible, until the current input
                // character matches the U+003E GREATER-THAN SIGN character. Append each
                // character to the temporary buffer when it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    s.push(c);

                    if c == '>' {
                        break;
                    }
                }

                self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                if s.len() >= "<![CDATA[".len() && s.ends_with("]]>") {
                    self.state = State::Data;
                } else {
                    self.reconsume_in_state(State::Data);
                }

                self.temporary_buffer.clear();
            }
            State::HTMLBlockType6 => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '\x20' || c == '\x09' || is_line_ending(c) || c == '>' => {
                        self.skip_whitespaces(c);

                        self.state = State::HTMLBlankLine;

                        self.temporary_buffer.push(c);
                    }
                    // U+002F SOLIDUS (/)
                    Some(c) if c == '/' => {
                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                        self.reconsume_in_state(State::Data);

                        self.temporary_buffer.clear();
                    }
                    // Anything else
                    Some(c) => {
                        self.reconsume_in_state(State::HTMLBlock);
                    }
                }
            }
            State::HTMLOpenTagName => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // ASCII alpha
                    // ASCII digit
                    // U+002D HYPHEN-MINUS (-)
                    Some(c) if is_ascii_alpha(c) || c.is_ascii_digit() || c == '-' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_spacy(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::HTMLAttributes);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLClosingTagName => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // ASCII alpha
                    // ASCII digit
                    // U+002D HYPHEN-MINUS (-)
                    Some(c) if is_ascii_alpha(c) || c.is_ascii_digit() || c == '-' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_spacy(c) => {
                        self.reconsume_in_state(State::HTMLClosingTag);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLClosingTag => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_spacy(c) => {
                        self.skip_whitespaces(c);

                        self.state = State::HTMLBlankLine;

                        self.temporary_buffer.push(c);
                    }
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '>' => {
                        if self.temporary_buffer.is_empty()
                            || self.temporary_buffer.chars().next() == Some('/')
                        {
                            self.state = State::HTMLBlankLine;
                        } else {
                            self.state = State::HTMLClosingTag;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+002F SOLIDUS (/)
                    Some(c) if c == '/' => {
                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                        self.reconsume_in_state(State::Data);

                        self.temporary_buffer.clear();
                    }
                    // Anything else
                    Some(c) => {
                        self.reconsume_in_state(State::HTMLBlock);
                    }
                }
            }
            State::HTMLBlankLine => {
                // Consume the maximum amount of characters possible, until the
                // line of the current input character is a blank line. Append
                // each character to the temporary buffer when it’s consumed.
                let mut s = String::with_capacity(self.temporary_buffer.len());

                while let Some(c) = self.consume_next_char() {
                    s.push(c);

                    if is_line_ending(c) {
                        break;
                    }
                }

                if s.lines().last().is_some_and(is_blank_line) {
                    self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                    self.state = State::Data;

                    self.temporary_buffer.clear();
                } else {
                    self.emit_token(Token::HTMLBlock(self.temporary_buffer.to_owned()));

                    self.reconsume_in_state(State::Data);

                    self.temporary_buffer.clear();
                }
            }
            State::HTMLAttributes => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    // ASCII alpha
                    // U+005F LOW LINE (_)
                    // U+003A COLON (:)
                    Some(c) if is_spacy(c) || is_ascii_alpha(c) || c == '_' || c == ':' => {
                        self.reconsume_in_state(State::HTMLAttribute);
                        self.line_ending_count = 0;
                    }
                    // U+002F SOLIDUS (/)
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '/' || c == '>' => {
                        self.reconsume_in_state(State::HTMLAttributes);
                    }
                    // EOF
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::AfterHTMLAttributes => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+002F SOLIDUS (/)
                    Some(c) if c == '\x20' || c == '\x09' || c == '/' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        if self.line_ending_count == 1 {
                            self.reconsume_in_state(State::Data);

                            self.line_ending_count = 0;
                        } else {
                            self.line_ending_count += 1;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '>' => {
                        self.reconsume_in_state(State::HTMLBlankLine);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLAttribute => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x20' || c == '\x09' => {}
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        if self.line_ending_count == 1 {
                            self.reconsume_in_state(State::Data);

                            self.line_ending_count = 0;
                        } else {
                            self.line_ending_count += 1;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // ASCII alpha
                    // U+005F LOW LINE (_)
                    // U+003A COLON (:)
                    Some(c) if is_ascii_alpha(c) || c == '_' || c == ':' => {
                        self.reconsume_in_state(State::HTMLAttributeName);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLAttributeName => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // ASCII alpha
                    // U+005F LOW LINE (_)
                    // U+003A COLON (:)
                    Some(c) if is_ascii_alpha(c) || c == '_' || c == ':' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+002E FULL STOP (.)
                    // U+002D HYPHEN-MINUS (-)
                    Some(c) if c == '.' || c == '-' => {
                        if self.temporary_buffer.is_empty() {
                            self.reconsume_in_state(State::Data);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self
                            .temporary_buffer
                            .chars()
                            .last()
                            .is_some_and(|c| is_ascii_alpha(c) || c == ':' || c == '.' || c == '-')
                        {
                            self.reconsume_in_state(State::HTMLAttributeValueSpecification);

                            self.line_ending_count = 0;
                        } else {
                            self.reconsume_in_state(State::Data);
                        }
                    }
                }
            }
            State::HTMLAttributeValueSpecification => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x20' || c == '\x09' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        if self.line_ending_count == 1 {
                            self.reconsume_in_state(State::Data);

                            self.line_ending_count = 0;
                        } else {
                            self.line_ending_count += 1;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+003D EQUALS SIGN (=)
                    Some(c) if c == '=' => {
                        self.state = State::HTMLAttributeValue;

                        self.line_ending_count = 0;

                        self.temporary_buffer.push(c);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::HTMLAttributeValue => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0027 APOSTROPHE (')
                    Some(c) if c == '\'' => {
                        self.state = State::HTMLSingleQuotedAttribute;

                        self.temporary_buffer.push(c);
                    }
                    // U+0022 QUOTATION MARK (")
                    Some(c) if c == '"' => {
                        self.state = State::HTMLDoubleQuotedAttribute;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.reconsume_in_state(State::HTMLUnquotedAttribute);
                    }
                }
            }
            State::HTMLUnquotedAttribute => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_spacy(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::HTMLAttributes);
                    }
                    // U+0027 APOSTROPHE (')
                    // U+0022 QUOTATION MARK (")
                    // U+003D EQUALS SIGN (=)
                    // U+003C LESS-THAN SIGN (<)
                    // U+003E GREATER-THAN SIGN (>)
                    // U+0060 GRAVE ACCENT (`)
                    Some('\'') | Some('"') | Some('=') | Some('<') | Some('>') | Some('`')
                    | None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::HTMLSingleQuotedAttribute => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0027 APOSTROPHE (')
                    Some(c) if c == '\'' => {
                        self.state = State::HTMLAttributes;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::HTMLDoubleQuotedAttribute => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0022 QUOTATION MARK (")
                    Some(c) if c == '"' => {
                        self.state = State::HTMLAttributes;

                        self.temporary_buffer.push(c);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::LinkLabel => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_spacy(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::Data);
                    }
                    // U+005D RIGHT SQUARE BRACKET (])
                    Some(c) if c == ']' => {
                        self.state = State::AfterLinkLabel;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.len() == 999 {
                            self.reconsume_in_state(State::Data);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                }
            }
            State::AfterLinkLabel => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+003A COLON (:)
                    Some(c) if c == ':' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x20' || c == '\x09' => {
                        let link_label_length = self
                            .temporary_buffer
                            .chars()
                            .take_while(|c| *c != ']')
                            .count();

                        if self
                            .temporary_buffer
                            .chars()
                            .nth(link_label_length)
                            .unwrap()
                            == ':'
                        {
                            self.temporary_buffer.push(c);
                        } else {
                            self.reconsume_in_state(State::Data);
                        }
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        if self.line_ending_count == 1 {
                            self.reconsume_in_state(State::Data);

                            self.line_ending_count = 0;
                        } else {
                            self.line_ending_count += 1;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.chars().next() == Some(':') {
                            self.state = State::LinkDestination;
                        }
                    }
                }
            }
            State::LinkDestination => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+003C LESS-THAN SIGN (<)
                    Some(c) if c == '<' => {
                        if self.temporary_buffer.is_empty() {
                            self.state = State::BracedLinkDestination;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::BracedLinkDestination => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+003E GREATER-THAN SIGN (>)
                    Some(c) if c == '>' => {
                        self.state = State::BeforeLinkTitle;

                        self.temporary_buffer.push(c);
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::Data);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::UnquotedLinkDestination => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // ASCII control
                    Some(c) if c == '\x20' || is_control(c as u32) => {
                        self.reconsume_in_state(State::BeforeLinkTitle);
                    }
                    // U+0028 LEFT PARENTHESIS
                    Some(c) if c == '(' => {
                        if self.temporary_buffer.chars().last() == Some('\x5c') {
                            self.temporary_buffer.push(c);
                        } else {
                            self.reconsume_in_state(State::BalancedParenthesisPairState);
                        }
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::Data);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::BalancedParenthesisPairState => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0029 RIGHT PARENTHESIS
                    Some(c) if c == ')' => {
                        self.reconsume_in_state(State::UnquotedLinkDestination);
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        self.reconsume_in_state(State::Data);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        self.temporary_buffer.push(c);
                    }
                }
            }
            State::BeforeLinkTitle => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0020 SPACE
                    // U+0009 CHARACTER TABULATION (tab)
                    Some(c) if c == '\x20' || c == '\x09' => {
                        self.temporary_buffer.push(c);
                    }
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        if self.line_ending_count == 1 {
                            self.reconsume_in_state(State::Data);

                            self.line_ending_count = 0;
                        } else {
                            self.line_ending_count += 1;
                        }

                        self.temporary_buffer.push(c);
                    }
                    // U+0022 QUOTATION MARK (")
                    Some(c) if c == '"' => {
                        self.reconsume_in_state(State::DoubleQuotedLinkTitle);
                    }
                    // U+0027 APOSTROPHE (')
                    Some(c) if c == '\'' => {
                        self.reconsume_in_state(State::SingleQuotedLinkTitle);
                    }
                    // U+0028 LEFT PARENTHESIS
                    Some(c) if c == '(' => {
                        self.reconsume_in_state(State::ParentheticalLinkTitle);
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
            State::DoubleQuotedLinkTitle => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0022 QUOTATION MARK (")
                    Some(c) if c == '"' => {
                        self.state = State::AfterLinkTitle;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.chars().last() == Some('"')
                            && self.temporary_buffer.chars().rev().nth(1) != Some('\\')
                        {
                            self.reconsume_in_state(State::Data);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                }
            }
            State::SingleQuotedLinkTitle => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0027 APOSTROPHE (')
                    Some(c) if c == '\'' => {
                        self.state = State::AfterLinkTitle;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.chars().last() == Some('\'')
                            && self.temporary_buffer.chars().rev().nth(1) != Some('\\')
                        {
                            self.reconsume_in_state(State::Data);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                }
            }
            State::ParentheticalLinkTitle => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+0029 RIGHT PARENTHESIS
                    Some(c) if c == ')' => {
                        self.state = State::AfterLinkTitle;

                        self.temporary_buffer.push(c);
                    }
                    // EOF
                    None => {
                        self.reconsume_in_state(State::Data);
                    }
                    // Anything else
                    Some(c) => {
                        if self.temporary_buffer.chars().last() == Some('\'')
                            && self.temporary_buffer.chars().rev().nth(1) != Some('\\')
                        {
                            self.reconsume_in_state(State::Data);
                        } else {
                            self.temporary_buffer.push(c);
                        }
                    }
                }
            }
            State::AfterLinkTitle => {
                // Consume the next input character:
                match self.consume_next_char() {
                    // U+000A LINE FEED (LF)
                    // U+000D CARRIAGE RETURN (CR)
                    Some(c) if is_line_ending(c) => {
                        self.skip_whitespaces(c);

                        // TODO Parse the link reference definition
                    }
                    // Anything else
                    None | Some(_) => {
                        self.reconsume_in_state(State::Data);
                    }
                }
            }
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

// A line containing no characters, or a line containing only spaces (U+0020)
// or tabs (U+0009)
#[inline(always)]
fn is_blank_line(l: &str) -> bool {
    l.is_empty() || l.chars().all(|c| c == '\x20' || c == '\x09')
}

// U+0020 SPACE
// U+00A0 NO-BREAK SPACE (NBSP)
// U+1680 OGHAM SPACE MARK
// U+2000 EN QUAD
// U+2001 EM QUAD
// U+2002 EN SPACE
// U+2003 EM SPACE
// U+2004 THREE-PER-EM SPACE
// U+2005 FOUR-PER-EM
// U+2006 SIX-PER-EM SPACE
// U+2007 FIGURE SPACE
// U+2008 PUNCTUATION SPACE
// U+2009 THIN SPACE
// U+200A HAIR SPACE
// U+202F NARROW NO-BREAK SPACE (NNBSP)
// U+205F MEDIUM MATHEMATICAL SPACE (MMSP)
// U+3000 IDEOGRAPHIC SPACE
const UNICODE_SPACE_SEPERATORS: [char; 13] = [
    '\x20', '\u{00a0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
    '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}',
];

// By spec '\r' is replaced with '\n' before tokenizer, but we keep them to have
// better AST and don't break logic to ignore characters
#[inline(always)]
fn is_line_ending(c: char) -> bool {
    // U+000A LINE FEED (LF)
    // U+000D CARRIAGE RETURN (CR)
    matches!(c, '\x0a' | '\x0d')
}

// By spec '\r' removed before tokenizer, but we keep them to have better AST
// and don't break logic to ignore characters
#[inline(always)]
fn is_spacy(c: char) -> bool {
    // U+0009 CHARACTER TABULATION (tab)
    // U+000A LINE FEED (LF)
    // U+000C FORM FEED (FF)
    // U+000D CARRIAGE RETURN (CR)
    // U+0020 SPACE
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
