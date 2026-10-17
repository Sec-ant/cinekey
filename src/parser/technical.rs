use crate::{
    lexer::{ReleaseToken as Token, Separator},
    model::MediaKind,
    vocabulary::Word,
};

use super::episodes::{parse_s_token, parse_x_format};
use super::{metadata, title_before};

pub(super) fn leading_content_token(tokens: &[Token<'_>]) -> usize {
    tokens
        .iter()
        .position(|token| !token.bracketed)
        .unwrap_or(0)
}

pub(super) fn first_technical_token(tokens: &[Token<'_>]) -> Option<usize> {
    technical_from(tokens, 0, false)
}

pub(super) fn episode_technical_token(
    tokens: &[Token<'_>],
    boundary: Option<usize>,
    episode_end: usize,
) -> Option<usize> {
    let start = boundary?;
    if start > episode_end + 1
        && metadata::edition_word(&tokens[start])
        && tokens[episode_end + 1..start]
            .iter()
            .any(|token| !is_release_year(token.text))
    {
        technical_from(tokens, start, true)
    } else {
        boundary
    }
}

#[inline]
fn technical_from(tokens: &[Token<'_>], start: usize, episode_title: bool) -> Option<usize> {
    let content_start = leading_content_token(tokens);
    tokens
        .iter()
        .enumerate()
        .skip(content_start.max(start))
        .filter(|(_, token)| !episode_title || !metadata::edition_word(token))
        .find_map(|(index, token)| {
            let technical = token.is_technical()
                && technical_word_context(tokens, index)
                && !(index == content_start
                    && !token.bracketed
                    && ascii_one_of(
                        token.text,
                        &[
                            "web",
                            "proper",
                            "repack",
                            "remastered",
                            "unrated",
                            "extended",
                            "uncut",
                            "limited",
                            "vhs",
                            "pal",
                            "ntsc",
                            "dual",
                            "imax",
                        ],
                    ))
                && (!token.text.eq_ignore_ascii_case("web")
                    || tokens.get(index + 1).is_some_and(|next| {
                        next.text.eq_ignore_ascii_case("dl") || next.is_technical()
                    }));
            (technical
                || technical_phrase(tokens, index)
                || dimension(token.text).is_some()
                || (token.bracketed
                    && token.text.len() == 8
                    && token.text.bytes().all(|byte| byte.is_ascii_hexdigit()))
                || (index > 0 && release_language_boundary(tokens, index)))
            .then_some(index)
        })
}

fn technical_word_context(tokens: &[Token<'_>], index: usize) -> bool {
    let token = &tokens[index];
    let ambiguous = match token.text.as_bytes().first().map(u8::to_ascii_lowercase) {
        Some(b'l') => token.text.eq_ignore_ascii_case("limited"),
        Some(b'u') => token.text.eq_ignore_ascii_case("uncut"),
        Some(b'v') => token.text.eq_ignore_ascii_case("vhs"),
        Some(b'p') => token.text.eq_ignore_ascii_case("pal"),
        Some(b'n') => token.text.eq_ignore_ascii_case("ntsc"),
        Some(b'c') => token.text.eq_ignore_ascii_case("convert"),
        Some(b's') => token.text.eq_ignore_ascii_case("subbed"),
        _ => false,
    };
    if !ambiguous {
        return true;
    }
    token.bracketed
        || tokens.get(index + 1).is_some_and(|next| {
            next.is_technical()
                || ascii_one_of(
                    next.text,
                    &["multi", "french", "german", "vostfr", "truefrench"],
                )
        })
}

fn technical_phrase(tokens: &[Token<'_>], index: usize) -> bool {
    let word = tokens[index].text;
    let next = tokens.get(index + 1).map(|token| token.text);
    match word.as_bytes().first().map(u8::to_ascii_lowercase) {
        Some(b'b') => {
            word.eq_ignore_ascii_case("blu")
                && next.is_some_and(|text| text.eq_ignore_ascii_case("ray"))
                || word
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("br"))
                    && word.get(2..).and_then(resolution).is_some()
        }
        Some(b'h' | b'x') => {
            word.eq_ignore_ascii_case("hd")
                && next.is_some_and(|text| text.eq_ignore_ascii_case("light"))
                || separated_video_codec(tokens, index).is_some()
        }
        Some(b'd' | b't' | b'u') => {
            ascii_one_of(word, &["director", "directors", "theatrical", "ultimate"])
                && metadata::phrase(tokens, index)
        }
        Some(b'c') => {
            word.eq_ignore_ascii_case("complete")
                && next.is_some_and(|text| {
                    tokens[index + 1].is_technical()
                        || parse_s_token(text).is_some()
                        || ascii_one_of(text, &["multi", "season", "seasons"])
                })
        }
        Some(b'3') => {
            word.eq_ignore_ascii_case("3d")
                && tokens
                    .iter()
                    .skip(index + 1)
                    .take(7)
                    .any(|token| ascii_one_of(token.text, &["hou", "hsbs", "sbs"]))
        }
        Some(b'i' | b'm' | b'n' | b'a' | b'p') => metadata::service_prefix(tokens, index),
        Some(b'1' | b'8') => video_bit_depth(tokens, index).is_some(),
        _ => false,
    }
}

fn video_bit_depth(tokens: &[Token<'_>], index: usize) -> Option<u8> {
    let token = &tokens[index];
    if let Some(Word::VideoBitDepth(depth)) = token.word {
        return Some(depth);
    }
    let depth = match token.text {
        "8" => 8,
        "10" => 10,
        "12" => 12,
        _ => return None,
    };
    let next = tokens.get(index + 1)?;
    (matches!(
        next.before,
        Separator::Dot | Separator::Dash | Separator::Underscore | Separator::Space
    ) && ascii_one_of(next.text, &["bit", "bits"]))
    .then_some(depth)
}

fn separated_video_codec(tokens: &[Token<'_>], index: usize) -> Option<&'static str> {
    if !ascii_one_of(tokens.get(index)?.text, &["h", "x"]) {
        return None;
    }
    let next = tokens.get(index + 1)?;
    if !matches!(
        next.before,
        Separator::Dot | Separator::Dash | Separator::Underscore | Separator::Space
    ) {
        return None;
    }
    match next.text {
        "264" => Some("H.264"),
        "265" => Some("H.265"),
        _ => None,
    }
}

fn release_language_boundary(tokens: &[Token<'_>], index: usize) -> bool {
    let text = tokens[index].text;
    if tokens[index].bracketed
        && text.len() <= 3
        && let Some(next) = tokens.get(index + 1)
        && next.bracketed
        && matches!(
            next.before,
            Separator::Dash | Separator::Plus | Separator::Dot
        )
        && language_code(text)
        && language_code(next.text)
    {
        return true;
    }
    let Some(first) = text.as_bytes().first() else {
        return false;
    };
    let (language, standalone) = match first.to_ascii_lowercase() {
        b'm' => (ascii_one_of(text, &["multi", "multilingual"]), true),
        b't' => (text.eq_ignore_ascii_case("truefrench"), true),
        b'v' => (
            ascii_one_of(
                text,
                &["vostfr", "vost", "vff", "vfq", "vfi", "vf2", "vf", "vo"],
            ),
            true,
        ),
        b'f' => (text.eq_ignore_ascii_case("french"), false),
        b'g' => (text.eq_ignore_ascii_case("german"), false),
        b'e' => (text.eq_ignore_ascii_case("english"), false),
        b's' => (text.eq_ignore_ascii_case("spanish"), false),
        b'i' => (text.eq_ignore_ascii_case("italian"), false),
        b'd' => (ascii_one_of(text, &["dl", "dubbed"]), true),
        _ => return false,
    };
    language
        && (standalone
            || tokens.get(index + 1).is_some_and(|next| {
                next.is_technical() || ascii_one_of(next.text, &["dl", "dubbed"])
            }))
}

fn language_code(text: &str) -> bool {
    match text.len() {
        2 => ascii_one_of(
            text,
            &[
                "en", "fr", "de", "es", "it", "ja", "jp", "zh", "ko", "ru", "pt",
            ],
        ),
        3 => ascii_one_of(
            text,
            &[
                "eng", "fre", "fra", "ger", "deu", "spa", "ita", "jpn", "chi", "zho", "kor", "rus",
                "por",
            ],
        ),
        _ => false,
    }
}

pub(super) fn is_technical(token: &str) -> bool {
    crate::vocabulary::classify(token).is_some_and(crate::vocabulary::Word::boundary)
}

pub(super) fn is_release_year(token: &str) -> bool {
    token.len() == 4
        && token.bytes().all(|b| b.is_ascii_digit())
        && token
            .parse::<u16>()
            .is_ok_and(|year| (1870..=2035).contains(&year))
}

pub(super) fn fill_technical(
    candidate: &mut super::Candidate,
    name: &str,
    tokens: &[Token<'_>],
    first_tech: Option<usize>,
    group: Option<Group<'_>>,
) {
    let metadata_start = if candidate.media.kind == MediaKind::Movie {
        candidate.title_span.map(|span| span.end)
    } else {
        first_tech
            .and_then(|index| tokens.get(index))
            .map(|token| token.start)
    }
    .unwrap_or(name.len());
    let group_start = group.map_or(name.len(), |group| group.start);
    let candidate = &mut candidate.media;
    if let Some(value) = group {
        candidate.release_group = Some(value.text.to_owned());
    }
    let mut encoded = false;
    let mut copied = false;
    let mut downloaded = false;
    let mut native_codec = false;
    let mut bit_depths = 0u16;
    for (i, token) in tokens.iter().enumerate() {
        let before_technical = !token.bracketed && first_tech.is_some_and(|start| i < start);
        let metadata_area =
            (token.bracketed || token.start >= metadata_start) && token.start < group_start;
        if before_technical && !metadata_area {
            continue;
        }
        let text = token.text;
        let mut key_buffer = [0u8; 16];
        let key_length = text.len().min(key_buffer.len());
        let key = &mut key_buffer[..key_length];
        for (output, input) in key.iter_mut().zip(text.bytes()) {
            *output = input.to_ascii_lowercase();
        }
        let key = &*key;
        if metadata_area
            && let Some(marker) = metadata::recognize(tokens, i)
            && marker.end <= group_start
        {
            marker.value.apply(candidate);
        }
        if metadata_area && let Some(depth) = video_bit_depth(tokens, i) {
            bit_depths |= 1 << depth;
        }
        if before_technical {
            continue;
        }
        let combined_resolution = key
            .strip_prefix(b"br")
            .and_then(|_| text.get(2..))
            .and_then(resolution);
        if candidate.resolution.is_none() {
            let resolution = dimension(text)
                .map(|(_, height)| format!("{height}p"))
                .or_else(|| {
                    let value = match key {
                        b"480p" => "480p",
                        b"480i" => "480i",
                        b"576p" => "576p",
                        b"576i" => "576i",
                        b"720p" => "720p",
                        b"1080p" => "1080p",
                        b"1080i" => "1080i",
                        b"1440p" => "1440p",
                        b"2160p" => "2160p",
                        b"4320p" => "4320p",
                        _ => return None,
                    };
                    Some(value.to_owned())
                })
                .or_else(|| combined_resolution.map(str::to_owned));
            if let Some(resolution) = resolution {
                candidate.resolution = Some(resolution);
            }
        }
        let (source, source_transcoded) = match key {
            b"bluray" | b"bd" => (Some("Blu-ray"), None),
            b"bdrip" | b"brrip" => (Some("Blu-ray"), Some(true)),
            b"bdmux" | b"brmux" | b"bdripmux" | b"brripmux" | b"bdremux" | b"uhdremux" => {
                (Some("Blu-ray"), Some(false))
            }
            _ if combined_resolution.is_some() => (Some("Blu-ray"), None),
            b"blu"
                if tokens
                    .get(i + 1)
                    .is_some_and(|next| next.text.eq_ignore_ascii_case("ray")) =>
            {
                (Some("Blu-ray"), None)
            }
            b"web"
                if tokens.get(i + 1).is_some_and(|next| {
                    next.before == Separator::Dash && next.text.eq_ignore_ascii_case("dl")
                }) =>
            {
                downloaded |= metadata_area;
                (Some("WEB-DL"), None)
            }
            b"webdl" => {
                downloaded |= metadata_area;
                (Some("Web"), None)
            }
            b"webrip" => (Some("Web"), Some(true)),
            b"web" => (Some("Web"), None),
            b"hdtv" | b"pdtv" => (Some("HDTV"), None),
            b"hdtvrip" => (Some("HDTV"), Some(true)),
            b"sdtv" => (Some("SDTV"), None),
            b"tvrip" => (Some("SDTV"), Some(true)),
            b"dvd" | b"dvdr" | b"dvdscr" => (Some("DVD"), None),
            b"dvdrip" => (Some("DVD"), Some(true)),
            b"vhs" => (Some("VHS"), None),
            b"hdcam" | b"camrip" => (Some("CAM"), None),
            b"hdrip" => (Some("HD Rip"), Some(true)),
            b"remux" => (Some("Remux"), Some(false)),
            _ => (None, None),
        };
        if metadata_area {
            encoded |= source_transcoded == Some(true);
            copied |= source_transcoded == Some(false);
        }
        if let Some(source) = source {
            candidate.source.get_or_insert_with(|| source.to_owned());
        }
        let (codec, encoder) = match token.word {
            Some(Word::VideoCodec(codec)) => (Some(codec.as_str()), false),
            Some(Word::VideoEncoder(codec)) => (Some(codec.as_str()), true),
            _ if matches!(key, b"h" | b"x") => (separated_video_codec(tokens, i), key == b"x"),
            _ => (None, false),
        };
        if let Some(codec) = codec {
            if metadata_area {
                encoded |= encoder;
                native_codec |= !encoder;
            }
            candidate
                .video_codec
                .get_or_insert_with(|| codec.to_owned());
        }
        if matches!(key, b"dv" | b"dovi" | b"dolbyvision") {
            candidate.dynamic_range.push("Dolby Vision".to_owned());
        }
        let hdr = match key {
            b"hdr10plus" => Some("HDR10+"),
            b"hdr10" if name.as_bytes().get(token.end) == Some(&b'+') => Some("HDR10+"),
            b"hdr10" => Some("HDR10"),
            b"hdr" => Some("HDR"),
            _ => None,
        };
        if let Some(value) = hdr {
            candidate.dynamic_range.push(value.to_owned());
        }
        let codec = if key == b"dd"
            && tokens
                .get(i + 1)
                .is_some_and(|next| next.before == Separator::Plus)
        {
            Some("E-AC-3")
        } else {
            audio_codec(key)
        };
        if let Some(codec) = codec
            && !candidate.audio_codec.iter().any(|c| c == codec)
        {
            candidate.audio_codec.push(codec.to_owned());
        }
        if text
            .as_bytes()
            .last()
            .is_some_and(|b| matches!(b, b'1' | b'2' | b'5' | b'7'))
            && (token.before == Separator::Dot
                || (token.before == Separator::Plus
                    && i > 0
                    && tokens[i - 1].text.eq_ignore_ascii_case("dd")))
            && tokens.get(i + 1).is_some_and(|next| {
                next.before == Separator::Dot && (next.text == "0" || next.text == "1")
            })
        {
            let channel = *text.as_bytes().last().unwrap_or(&b'0') as char;
            let layout = tokens.get(i + 1).map_or("0", |next| next.text);
            candidate
                .audio_channels
                .get_or_insert_with(|| format!("{channel}.{layout}"));
        }
    }

    candidate.video_bit_depth = bit_depths
        .is_power_of_two()
        .then(|| bit_depths.trailing_zeros() as u8);
    candidate.video_transcoded = match (encoded, copied) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (true, true) => None,
        (false, false) => {
            let disc = candidate.source.as_deref() == Some("Blu-ray")
                && candidate
                    .release_flags
                    .contains(crate::ReleaseFlag::Complete)
                && matches!(candidate.video_codec.as_deref(), Some("H.264" | "H.265"));
            (downloaded || native_codec && (candidate.source.as_deref() == Some("Web") || disc))
                .then_some(false)
        }
    };

    if candidate.source.as_deref() == Some("Web")
        && tokens
            .iter()
            .any(|t| t.text.eq_ignore_ascii_case("dl") && t.before == Separator::Dash)
    {
        candidate.source = Some("WEB-DL".to_owned());
    }
    if let Some(start) = first_tech
        && candidate.title.is_none()
        && candidate.kind != MediaKind::Extra
    {
        candidate.title = title_before(name, tokens, start);
    }
    candidate.dynamic_range.sort();
    candidate.dynamic_range.dedup();
}

pub(super) fn ascii_one_of(value: &str, choices: &[&str]) -> bool {
    choices
        .iter()
        .any(|choice| value.eq_ignore_ascii_case(choice))
}

fn audio_codec(key: &[u8]) -> Option<&'static str> {
    if key == b"eac3" || key.starts_with(b"ddp") || key.starts_with(b"dd+") {
        Some("E-AC-3")
    } else if matches!(key, b"ac3" | b"ac3d") || key.starts_with(b"dd") {
        Some("AC-3")
    } else if key == b"atmos" {
        Some("Dolby Atmos")
    } else if key == b"truehd" {
        Some("TrueHD")
    } else if key.starts_with(b"flac") {
        Some("FLAC")
    } else if key.starts_with(b"aac") {
        Some("AAC")
    } else if key.starts_with(b"dts") {
        Some("DTS")
    } else if key == b"opus" {
        Some("Opus")
    } else {
        None
    }
}

pub(super) fn ascii_starts_with(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        value
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    })
}

pub(super) fn dimension(text: &str) -> Option<(u16, u16)> {
    let (width, height) = parse_x_format(text)?;
    (width > height && width >= 640 && height >= 360).then_some((width, height))
}

fn resolution(text: &str) -> Option<&'static str> {
    [
        "480p", "480i", "576p", "576i", "720p", "1080p", "1080i", "1440p", "2160p", "4320p",
    ]
    .iter()
    .copied()
    .find(|known| text.eq_ignore_ascii_case(known))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Group<'a> {
    pub text: &'a str,
    pub start: usize,
}

pub(super) fn release_group<'a>(
    name: &'a str,
    tokens: &[Token<'_>],
    stem_end: usize,
    episode_end: Option<usize>,
) -> Option<Group<'a>> {
    let stem = name.get(..stem_end)?;
    let dash = stem.rfind('-')?;
    let suffix = stem[dash + 1..].trim();
    let last_index = tokens.len().checked_sub(1)?;
    let last = &tokens[last_index];
    let fallback =
        (last.text == suffix && valid_group(suffix) && !is_web_dl_suffix(tokens, last_index))
            .then_some(Group {
                text: suffix,
                start: last.start,
            });
    if fallback.is_some()
        && last_index > 0
        && last.before == Separator::Dash
        && tokens[last_index - 1].before != Separator::Dash
        && !group_start_word(tokens, last_index - 1)
    {
        return fallback;
    }
    let component_start = |mut index: usize| {
        while index > 0
            && tokens[index].before == Separator::Dash
            && tokens[index - 1].end + 1 == tokens[index].start
        {
            index -= 1;
        }
        index.max(1)
    };
    if fallback.is_some() {
        return group_component(
            stem,
            tokens,
            component_start(last_index)..=last_index,
            episode_end,
        )
        .or(fallback);
    }
    let mut index = last_index;
    while index > 0 {
        let token = &tokens[index];
        if token.before == Separator::Dash
            && stem.as_bytes().get(token.start.checked_sub(1)?) == Some(&b'-')
        {
            let first_index = component_start(index);
            if let Some(group) = group_component(stem, tokens, first_index..=index, episode_end) {
                return Some(group);
            }
            index = first_index;
        }
        index -= 1;
    }
    None
}

fn group_component<'a>(
    stem: &'a str,
    tokens: &[Token<'_>],
    component: std::ops::RangeInclusive<usize>,
    episode_end: Option<usize>,
) -> Option<Group<'a>> {
    let end = stem[tokens[*component.end()].start..]
        .find('.')
        .map_or(stem.len(), |offset| tokens[*component.end()].start + offset);
    for index in component {
        let token = &tokens[index];
        if !matches!(token.before, Separator::Dash | Separator::Dot)
            || !matches!(
                stem.as_bytes().get(token.start.checked_sub(1)?),
                Some(b'-' | b'.')
            )
            || !group_start_word(tokens, index)
            || is_web_dl_suffix(tokens, index)
            || !group_anchor(tokens, index, episode_end)
        {
            continue;
        }
        if let Some(group) = stem.get(token.start..end)
            && valid_group(group)
            && group.len() > 1
            && !is_technical(group)
            && group.chars().any(char::is_alphabetic)
            && !group.contains(['[', ']', '(', ')'])
            && !tokens[index + 1..].iter().any(|later| later.is_technical())
            && !tokens[index + 1..]
                .iter()
                .filter(|later| later.start >= end)
                .any(|later| later.text.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Some(Group {
                text: group,
                start: token.start,
            });
        }
    }
    None
}

fn group_start_word(tokens: &[Token<'_>], index: usize) -> bool {
    let text = tokens[index].text;
    if !text.chars().any(char::is_alphabetic)
        || tokens[index].is_technical()
        || technical_phrase(tokens, index)
        || compound_technical(tokens, index)
        || dimension(text).is_some()
        || release_language_boundary(tokens, index)
        || language_code(text)
        || index > 0
            && (technical_phrase(tokens, index - 1) || compound_technical(tokens, index - 1))
    {
        return false;
    }
    let mut buffer = [0u8; 16];
    let Some(key) = buffer.get_mut(..text.len()) else {
        return true;
    };
    for (output, input) in key.iter_mut().zip(text.bytes()) {
        *output = input.to_ascii_lowercase();
    }
    if metadata::recognize(tokens, index).is_some()
        || matches!(
            &*key,
            b"mp2"
                | b"mp3"
                | b"mp4"
                | b"pcm"
                | b"lpcm"
                | b"preair"
                | b"svid"
                | b"line"
                | b"hdma"
                | b"sdh"
        )
    {
        return false;
    }
    !["bit", "bits", "ch", "audio"].iter().any(|unit| {
        text.get(..text.len().saturating_sub(unit.len()))
            .is_some_and(|number| {
                !number.is_empty()
                    && number.bytes().all(|byte| byte.is_ascii_digit())
                    && text[number.len()..].eq_ignore_ascii_case(unit)
            })
    })
}

fn compound_technical(tokens: &[Token<'_>], index: usize) -> bool {
    let Some(first) = tokens.get(index) else {
        return false;
    };
    let Some(second) = tokens.get(index + 1) else {
        return false;
    };
    if !matches!(
        second.before,
        Separator::Dash | Separator::Dot | Separator::Underscore | Separator::Space
    ) {
        return false;
    }
    let mut buffer = [0u8; 16];
    let Some(key) = buffer.get_mut(..first.text.len() + second.text.len()) else {
        return false;
    };
    for (output, input) in key
        .iter_mut()
        .zip(first.text.bytes().chain(second.text.bytes()))
    {
        *output = input.to_ascii_lowercase();
    }
    matches!(&*key, b"dtsx" | b"dtshd" | b"aaclc")
        || std::str::from_utf8(key).is_ok_and(is_technical)
}

fn valid_group(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 32
        && !text.contains([' ', '.', '_'])
        && text.chars().any(char::is_alphanumeric)
}

fn is_web_dl_suffix(tokens: &[Token<'_>], index: usize) -> bool {
    tokens[index].text.eq_ignore_ascii_case("DL")
        && index > 0
        && tokens[index - 1].text.eq_ignore_ascii_case("WEB")
}

fn group_anchor(tokens: &[Token<'_>], index: usize, episode_end: Option<usize>) -> bool {
    let Some(previous_index) = index.checked_sub(1) else {
        return false;
    };
    let previous = &tokens[previous_index];
    previous.word.is_some_and(crate::vocabulary::Word::boundary)
        || is_release_year(previous.text)
        || episode_end == Some(previous_index)
        || is_web_dl_suffix(tokens, previous_index)
        || previous_index
            .checked_sub(1)
            .is_some_and(|start| compound_technical(tokens, start))
        || metadata::recognize(tokens, previous_index)
            .is_some_and(|marker| marker.end == previous.end)
}
