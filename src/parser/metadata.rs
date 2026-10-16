use crate::{AudioProfile, Edition, ParsedMedia, ReleaseFlag, StreamingService, Token};

#[derive(Clone, Copy, Debug)]
pub(super) enum Attribute {
    Service(StreamingService),
    Edition(Edition),
    Flag(ReleaseFlag),
    AudioProfile(AudioProfile),
}

impl Attribute {
    pub(super) fn apply(self, media: &mut ParsedMedia) {
        match self {
            Self::Service(value) => {
                media.streaming_service.get_or_insert(value);
            }
            Self::Edition(value) => {
                media.edition.insert(value);
            }
            Self::Flag(value) => {
                media.release_flags.insert(value);
            }
            Self::AudioProfile(value) => {
                media.audio_profile.insert(value);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Word {
    pub value: Option<Attribute>,
    pub boundary: bool,
}

#[inline]
pub(super) fn word(key: &[u8], original: &str) -> Option<Word> {
    use Attribute::{Edition as E, Flag as F, Service as S};
    use Edition::*;
    use ReleaseFlag::*;
    use StreamingService::*;
    let (value, boundary) = match key {
        b"480p" | b"480i" | b"576p" | b"576i" | b"720p" | b"1080p" | b"1080i" | b"1440p"
        | b"2160p" | b"4320p" | b"4k" | b"8k" | b"bluray" | b"brrip" | b"bdrip" | b"bdmux"
        | b"brmux" | b"bdripmux" | b"brripmux" | b"bd" | b"web" | b"webdl" | b"webrip"
        | b"hdtv" | b"pdtv" | b"sdtv" | b"tvrip" | b"hdrip" | b"hdtvrip" | b"hdcam" | b"camrip"
        | b"hdlight" | b"mhd" | b"dvdrip" | b"dvdr" | b"dvdscr" | b"vhs" | b"pal" | b"ntsc"
        | b"dvd" | b"x264" | b"h264" | b"avc" | b"x265" | b"h265" | b"hevc" | b"av1" | b"vp9"
        | b"xvid" | b"divx" | b"hdr" | b"hdr10" | b"hdr10plus" | b"dv" | b"dovi"
        | b"dolbyvision" | b"sdr" | b"atmos" | b"truehd" | b"flac" | b"aac" | b"opus" | b"dts"
        | b"dd" | b"ddp" | b"ac3" | b"ac3d" | b"eac3" | b"dual" | b"convert" | b"10bit"
        | b"8bit" => {
            return Some(Word {
                value: None,
                boundary: true,
            });
        }
        b"nf" => (S(Netflix), true),
        b"netflix" | b"nflx" => (S(Netflix), false),
        b"amzn" => (S(Amazon), true),
        b"amazon" => (S(Amazon), false),
        b"atvp" => (S(AppleTvPlus), true),
        b"dsnp" => (S(DisneyPlus), true),
        b"hmax" => (S(HboMax), true),
        b"hulu" => (S(Hulu), true),
        b"cr" => (S(Crunchyroll), true),
        b"crunchyroll" => (S(Crunchyroll), false),
        b"pcok" | b"peacock" => (S(Peacock), false),
        b"pmtp" | b"paramountplus" => (S(ParamountPlus), false),
        b"itunes" | b"itn" => (S(ITunes), false),
        b"it" if original == "iT" => (S(ITunes), false),
        b"max" if original == "MAX" => (S(Max), false),
        b"limited" => (E(Limited), true),
        b"unrated" => (E(Unrated), true),
        b"extended" => (E(Extended), true),
        b"directorscut" => (E(DirectorsCut), true),
        b"uncut" => (E(Uncut), true),
        b"remastered" => (E(Remastered), true),
        b"imax" => (E(Imax), true),
        b"remaster" => (E(Remastered), false),
        b"restored" | b"restore" => (E(Restored), false),
        b"uncensored" => (E(Uncensored), false),
        b"theatrical" => (E(Theatrical), false),
        b"criterion" => (E(Criterion), false),
        b"special" => (E(Special), false),
        b"ultimate" => (E(Ultimate), false),
        b"collector" | b"collectors" => (E(Collector), false),
        b"deluxe" => (E(Deluxe), false),
        b"anniversary" => (E(Anniversary), false),
        b"festival" => (E(Festival), false),
        b"proper" => (F(Proper), true),
        b"repack" => (F(Repack), true),
        b"nfofix" => (F(NfoFix), true),
        b"remux" => (F(Remux), true),
        b"subbed" => (F(Subbed), true),
        b"real" => (F(Real), false),
        b"internal" | b"int" => (F(Internal), false),
        b"readnfo" => (F(ReadNfo), false),
        b"prooffix" => (F(ProofFix), false),
        b"hybrid" => (F(Hybrid), false),
        b"complete" => (F(Complete), false),
        _ => return None,
    };
    Some(Word {
        value: Some(value),
        boundary,
    })
}

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
pub(super) fn recognize(tokens: &[Token<'_>], index: usize, key: &[u8]) -> Option<Marker> {
    use Attribute::{AudioProfile as A, Edition as E, Flag as F, Service as S};
    let token = tokens.get(index)?;
    let next = index + 1;
    let previous = index.checked_sub(1);
    let dts_context = || {
        previous.is_some_and(|i| {
            is(tokens, i, "dts")
                || is(tokens, i, "dtshd")
                || is(tokens, i, "hd") && i > 0 && is(tokens, i - 1, "dts")
        })
    };
    let (value, count) = match key {
        b"ma" if dts_context() => (A(AudioProfile::MasterAudio), 1),
        b"master" if dts_context() && is(tokens, next, "audio") => {
            (A(AudioProfile::MasterAudio), 2)
        }
        b"hra" | b"hr" if dts_context() => (A(AudioProfile::HighResolutionAudio), 1),
        b"es" if previous.is_some_and(|i| is(tokens, i, "dts")) => {
            (A(AudioProfile::ExtendedSurround), 1)
        }
        b"lc" if previous.is_some_and(|i| is(tokens, i, "aac")) => {
            (A(AudioProfile::LowComplexity), 1)
        }
        b"he" if is(tokens, next, "aac") || previous.is_some_and(|i| is(tokens, i, "aac")) => {
            (A(AudioProfile::HighEfficiency), 1)
        }
        b"ma" if web_context(tokens, index) => (S(StreamingService::MoviesAnywhere), 1),
        b"director" | b"directors" => {
            let cut = next + usize::from(is(tokens, next, "s"));
            if !is(tokens, cut, "cut") {
                return None;
            }
            (E(Edition::DirectorsCut), cut - index + 1)
        }
        b"alternative" | b"alternate" if is(tokens, next, "cut") => (E(Edition::AlternativeCut), 2),
        b"ultimate" if is(tokens, next, "collector") || is(tokens, next, "collectors") => {
            (E(Edition::Ultimate), 2)
        }
        b"read" if is(tokens, next, "nfo") => (F(ReleaseFlag::ReadNfo), 2),
        b"nfo" if is(tokens, next, "fix") => (F(ReleaseFlag::NfoFix), 2),
        b"proof" if is(tokens, next, "fix") => (F(ReleaseFlag::ProofFix), 2),
        _ => {
            let value = word(key, token.text)?.value?;
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

pub(super) fn at(tokens: &[Token<'_>], index: usize) -> Option<Marker> {
    let text = tokens.get(index)?.text;
    let mut buffer = [0u8; 16];
    let key = buffer.get_mut(..text.len())?;
    for (output, input) in key.iter_mut().zip(text.bytes()) {
        *output = input.to_ascii_lowercase();
    }
    recognize(tokens, index, key)
}

pub(super) fn phrase(tokens: &[Token<'_>], index: usize) -> bool {
    at(tokens, index).is_some_and(|marker| marker.end > tokens[index].end)
}

pub(super) fn service_prefix(tokens: &[Token<'_>], index: usize) -> bool {
    tokens.get(index + 1).is_some_and(|token| {
        ["web", "webdl", "webrip"]
            .iter()
            .any(|word| token.text.eq_ignore_ascii_case(word))
    }) && at(tokens, index).is_some_and(|marker| matches!(marker.value, Attribute::Service(_)))
}

pub(super) fn edition_word(text: &str) -> bool {
    if text.as_bytes().first().is_none_or(u8::is_ascii_digit) {
        return false;
    }
    let mut buffer = [0u8; 16];
    let Some(key) = buffer.get_mut(..text.len()) else {
        return false;
    };
    for (output, input) in key.iter_mut().zip(text.bytes()) {
        *output = input.to_ascii_lowercase();
    }
    matches!(
        word(key, text),
        Some(Word {
            value: Some(Attribute::Edition(_)),
            ..
        })
    )
}
