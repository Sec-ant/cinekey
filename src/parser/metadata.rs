use crate::lexer::ReleaseToken as Token;
use crate::vocabulary::{Attribute, Phrase, Word};
use crate::{AudioProfile, Edition, ReleaseFlag, StreamingService};

#[derive(Clone, Copy, Debug)]
pub(super) struct Marker {
    pub value: Attribute,
    pub end: usize,
}

fn is(tokens: &[Token<'_>], index: usize, expected: &str) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| token.text.eq_ignore_ascii_case(expected))
}

fn web_context(tokens: &[Token<'_>], index: usize) -> bool {
    tokens
        .iter()
        .skip(index.saturating_sub(2))
        .take(5)
        .any(|token| {
            ["web", "webdl", "webrip"]
                .iter()
                .any(|word| token.text.eq_ignore_ascii_case(word))
        })
}

#[inline]
pub(super) fn recognize(tokens: &[Token<'_>], index: usize) -> Option<Marker> {
    use Attribute::{AudioProfile as A, Edition as E, Flag as F, Service as S};
    let token = tokens.get(index)?;
    let word = token.word?;
    let next = index + 1;
    let previous = index.checked_sub(1);
    let dts_context = || {
        previous.is_some_and(|i| {
            is(tokens, i, "dts")
                || is(tokens, i, "dtshd")
                || is(tokens, i, "hd") && i > 0 && is(tokens, i - 1, "dts")
        })
    };
    let (value, count) = match word {
        Word::Phrase(Phrase::Ma) if dts_context() => (A(AudioProfile::MasterAudio), 1),
        Word::Phrase(Phrase::Master) if dts_context() && is(tokens, next, "audio") => {
            (A(AudioProfile::MasterAudio), 2)
        }
        Word::Phrase(Phrase::HighResolution) if dts_context() => {
            (A(AudioProfile::HighResolutionAudio), 1)
        }
        Word::Phrase(Phrase::ExtendedSurround)
            if previous.is_some_and(|i| is(tokens, i, "dts")) =>
        {
            (A(AudioProfile::ExtendedSurround), 1)
        }
        Word::Phrase(Phrase::LowComplexity) if previous.is_some_and(|i| is(tokens, i, "aac")) => {
            (A(AudioProfile::LowComplexity), 1)
        }
        Word::Phrase(Phrase::HighEfficiency)
            if is(tokens, next, "aac") || previous.is_some_and(|i| is(tokens, i, "aac")) =>
        {
            (A(AudioProfile::HighEfficiency), 1)
        }
        Word::Phrase(Phrase::Ma) if web_context(tokens, index) => {
            (S(StreamingService::MoviesAnywhere), 1)
        }
        Word::Phrase(Phrase::Director) => {
            let cut = next + usize::from(is(tokens, next, "s"));
            if !is(tokens, cut, "cut") {
                return None;
            }
            (E(Edition::DirectorsCut), cut - index + 1)
        }
        Word::Phrase(Phrase::Alternative) if is(tokens, next, "cut") => {
            (E(Edition::AlternativeCut), 2)
        }
        Word::Edition(Edition::Ultimate)
            if is(tokens, next, "collector") || is(tokens, next, "collectors") =>
        {
            (E(Edition::Ultimate), 2)
        }
        Word::Phrase(Phrase::Read) if is(tokens, next, "nfo") => (F(ReleaseFlag::ReadNfo), 2),
        Word::Phrase(Phrase::Nfo) if is(tokens, next, "fix") => (F(ReleaseFlag::NfoFix), 2),
        Word::Phrase(Phrase::Proof) if is(tokens, next, "fix") => (F(ReleaseFlag::ProofFix), 2),
        _ => {
            let value = word.attribute()?;
            if matches!(value, E(Edition::Special | Edition::Anniversary))
                && !is(tokens, next, "edition")
            {
                return None;
            }
            let mut count = 1;
            if matches!(value, E(_)) {
                let qualifier = next + usize::from(is(tokens, next, "s"));
                if is(tokens, qualifier, "edition")
                    || is(tokens, qualifier, "cut")
                    || is(tokens, qualifier, "collection")
                {
                    count = qualifier - index + 1;
                }
            }
            (value, count)
        }
    };
    Some(Marker {
        value,
        end: tokens.get(index + count - 1)?.end,
    })
}

pub(super) fn phrase(tokens: &[Token<'_>], index: usize) -> bool {
    recognize(tokens, index).is_some_and(|marker| marker.end > tokens[index].end)
}

pub(super) fn service_prefix(tokens: &[Token<'_>], index: usize) -> bool {
    tokens.get(index + 1).is_some_and(|token| {
        ["web", "webdl", "webrip"]
            .iter()
            .any(|word| token.text.eq_ignore_ascii_case(word))
    }) && recognize(tokens, index)
        .is_some_and(|marker| matches!(marker.value, Attribute::Service(_)))
}

pub(super) fn edition_word(token: &Token<'_>) -> bool {
    token.word.is_some_and(crate::vocabulary::Word::is_edition)
}
