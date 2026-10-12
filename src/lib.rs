#![forbid(unsafe_code)]

//! A deterministic parser for movie and television release names.
//!
//! Parsing is split into a small lexer and recognizers for structural markers and
//! technical vocabulary. Each input yields one identification and metadata record.
//!
//! # Example
//!
//! ```
//! use cinekey::{MediaKind, parse};
//!
//! let media = parse("Arrival.2016.1080p.BluRay.mkv");
//! assert_eq!(media.kind, MediaKind::Movie);
//! assert_eq!(media.title.as_deref(), Some("Arrival"));
//! assert_eq!(media.year, Some(2016));
//! ```

mod lexer;
mod model;
mod parser;

pub use lexer::{Separator, Token, tokenize};
pub use model::{
    EpisodeNumbering, ExpectedKind, ExtraCategory, ExtraInfo, MediaKind, ParseContext, ParsedMedia,
};
pub use parser::{parse, parse_with_context};
