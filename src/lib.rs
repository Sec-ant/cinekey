#![forbid(unsafe_code)]

//! A deterministic, evidence-carrying parser for movie and television release names.
//!
//! Parsing is split into a small lexer and recognizers for structural markers and
//! technical vocabulary. Ambiguous structural interpretations produce separate,
//! ranked candidates with field-level evidence.
//!
//! # Example
//!
//! ```
//! use cinekey::{MediaKind, parse};
//!
//! let report = parse("Arrival.2016.1080p.BluRay.mkv");
//! let candidate = &report.candidates[0];
//! assert_eq!(candidate.kind, MediaKind::Movie);
//! assert_eq!(candidate.title.as_deref(), Some("Arrival"));
//! assert_eq!(candidate.year, Some(2016));
//! ```

mod lexer;
mod parser;

pub use lexer::{Separator, Token, tokenize};
pub use parser::{
    Candidate, Confidence, EpisodeNumbering, Evidence, EvidenceField, EvidenceSource, ExpectedKind,
    ExtraCategory, ExtraInfo, MediaKind, ParseContext, ParseReport, Span, parse,
    parse_with_context,
};
