use std::borrow::Cow;

use swc_atoms::Atom;
use swc_common::{
    errors::{DiagnosticBuilder, Handler},
    Span,
};

/// Represents a diagnostic message for Markdown parsing.
/// In Markdown, these are typically warnings or informational messages
/// rather than fatal errors, as Markdown is designed to be forgiving
/// and render unexpected input as literal text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    inner: Box<(Span, DiagnosticKind)>,
}

impl Diagnostic {
    pub fn kind(&self) -> &DiagnosticKind {
        &self.inner.1
    }

    pub fn into_inner(self) -> Box<(Span, DiagnosticKind)> {
        self.inner
    }

    pub fn new(span: Span, kind: DiagnosticKind) -> Self {
        Diagnostic {
            inner: Box::new((span, kind)),
        }
    }

    pub fn message(&self) -> Cow<'static, str> {
        match &self.inner.1 {
            DiagnosticKind::Eof => "Unexpected end of file".into(),

            // Character reference warnings (these don't prevent parsing)
            DiagnosticKind::MalformedCharacterReference => {
                "Malformed character reference (rendered as literal text)".into()
            }
            DiagnosticKind::UnknownNamedCharacterReference => {
                "Unknown named character reference (rendered as literal text)".into()
            }
            DiagnosticKind::MissingSemicolonAfterCharacterReference => {
                "Missing semicolon after character reference (rendered as literal text)".into()
            }
            DiagnosticKind::CharacterReferenceOutsideUnicodeRange => {
                "Character reference outside Unicode range (rendered as replacement character)"
                    .into()
            }
            DiagnosticKind::InvalidNumericCharacterReference => {
                "Invalid numeric character reference (rendered as literal text)".into()
            }
            DiagnosticKind::NullCharacterReference => {
                "Null character reference (replaced with U+FFFD)".into()
            }
            DiagnosticKind::SurrogateCharacterReference => {
                "Surrogate character reference (replaced with U+FFFD)".into()
            }
            DiagnosticKind::ControlCharacterReference => {
                "Control character reference (handled gracefully)".into()
            }
            DiagnosticKind::NoncharacterCharacterReference => {
                "Noncharacter character reference (handled gracefully)".into()
            }
            DiagnosticKind::UnexpectedNullCharacter => {
                "Unexpected null character (replaced with U+FFFD)".into()
            }
            DiagnosticKind::InvalidEscapeSequence => {
                "Invalid escape sequence (rendered as literal text)".into()
            }

            // Structural diagnostics (informational, don't prevent parsing)
            DiagnosticKind::UnclosedFencedCodeBlock => {
                "Unclosed fenced code block (rendered as literal text)".into()
            }
            DiagnosticKind::UnclosedHtmlBlock => {
                "Unclosed HTML block (rendered as literal text)".into()
            }
            DiagnosticKind::InconsistentListMarker => {
                "Inconsistent list marker (treated as separate list)".into()
            }
            DiagnosticKind::DuplicateLinkDefinition(label) => format!(
                "Duplicate link reference definition '{}' (first definition used)",
                label
            )
            .into(),
            DiagnosticKind::UnresolvedLinkReference(label) => format!(
                "Unresolved link reference '{}' (rendered as literal text)",
                label
            )
            .into(),
            DiagnosticKind::MalformedAutolink => {
                "Malformed autolink (rendered as literal text)".into()
            }
            DiagnosticKind::UnclosedEmphasisDelimiter => {
                "Unclosed emphasis delimiter (rendered as literal text)".into()
            }
            DiagnosticKind::MalformedLinkDestination => {
                "Malformed link destination (rendered as literal text)".into()
            }
            DiagnosticKind::MalformedLinkTitle => {
                "Malformed link title (rendered as literal text)".into()
            }

            // Processing diagnostics
            DiagnosticKind::MaxNestingDepthExceeded => {
                "Maximum nesting depth exceeded (further nesting ignored)".into()
            }
            DiagnosticKind::InvalidUnicodeEscape => {
                "Invalid Unicode escape sequence (rendered as literal text)".into()
            }
            DiagnosticKind::AbsenceOfDigitsInNumericCharacterReference => {
                "Numeric character reference without digits (rendered as literal text)".into()
            }
        }
    }

    pub fn to_diagnostics<'a>(&self, handler: &'a Handler) -> DiagnosticBuilder<'a> {
        // Most Markdown diagnostics are warnings rather than errors
        match &self.inner.1 {
            // These are more serious and might warrant error level
            DiagnosticKind::Eof => handler.struct_span_err(self.inner.0, &self.message()),
            DiagnosticKind::MaxNestingDepthExceeded => {
                handler.struct_span_err(self.inner.0, &self.message())
            }

            // Most others are warnings since Markdown handles them gracefully
            _ => handler.struct_span_warn(self.inner.0, &self.message()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// Unexpected end of file (only when truly unexpected, not for unclosed
    /// constructs)
    Eof,

    // Character reference diagnostics (CommonMark section 6.2)
    /// Malformed character reference that will be rendered as literal text
    MalformedCharacterReference,
    /// Unknown named character reference that will be rendered as literal text
    UnknownNamedCharacterReference,
    /// Missing semicolon after character reference (rendered as literal text)
    MissingSemicolonAfterCharacterReference,
    /// Character reference value outside valid Unicode range (replaced with
    /// U+FFFD)
    CharacterReferenceOutsideUnicodeRange,
    /// Invalid numeric character reference format (rendered as literal text)
    InvalidNumericCharacterReference,
    /// Character reference resolves to null character (replaced with U+FFFD)
    NullCharacterReference,
    /// Character reference resolves to surrogate character (replaced with
    /// U+FFFD)
    SurrogateCharacterReference,
    /// Character reference resolves to control character (handled gracefully)
    ControlCharacterReference,
    /// Character reference resolves to noncharacter (handled gracefully)
    NoncharacterCharacterReference,
    /// Unexpected null character in input (replaced with U+FFFD)
    UnexpectedNullCharacter,
    /// Invalid backslash escape sequence (rendered as literal text)
    InvalidEscapeSequence,
    /// Numeric character reference without any digits (rendered as literal
    /// text)
    AbsenceOfDigitsInNumericCharacterReference,

    // Block structure diagnostics (informational, don't prevent parsing)
    /// Unclosed fenced code block (rendered as literal text)
    UnclosedFencedCodeBlock,
    /// Unclosed HTML block (rendered as literal text)
    UnclosedHtmlBlock,
    /// Inconsistent list marker within the same list context (treated as
    /// separate list)
    InconsistentListMarker,
    /// Duplicate link reference definition with same label (first definition
    /// used)
    DuplicateLinkDefinition(Atom),

    // Inline structure diagnostics (informational, don't prevent parsing)
    /// Unclosed emphasis delimiter (rendered as literal text)
    UnclosedEmphasisDelimiter,
    /// Reference to undefined link label (rendered as literal text)
    UnresolvedLinkReference(Atom),
    /// Malformed autolink (rendered as literal text)
    MalformedAutolink,
    /// Malformed link destination (rendered as literal text)
    MalformedLinkDestination,
    /// Malformed link title (rendered as literal text)
    MalformedLinkTitle,

    // Processing diagnostics
    /// Nesting depth exceeds implementation limits (further nesting ignored)
    MaxNestingDepthExceeded,
    /// Invalid Unicode escape sequence (rendered as literal text)
    InvalidUnicodeEscape,
}

// Legacy alias for compatibility
pub type Error = Diagnostic;
pub type ErrorKind = DiagnosticKind;
