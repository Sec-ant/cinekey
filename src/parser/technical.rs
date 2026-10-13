use crate::{
    lexer::{Separator, Token},
    model::{MediaKind, ParsedMedia},
};

use super::episodes::{parse_s_token, parse_x_format};
use super::title_before;

pub(super) fn leading_content_token(tokens: &[Token<'_>]) -> usize {
    tokens
        .iter()
        .position(|token| !token.bracketed)
        .unwrap_or(0)
}

pub(super) fn first_technical_token(tokens: &[Token<'_>]) -> Option<usize> {
    let content_start = leading_content_token(tokens);
    tokens
        .iter()
        .enumerate()
        .skip(content_start)
        .find_map(|(index, token)| {
            let technical = is_technical(token.text)
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
                        next.text.eq_ignore_ascii_case("dl") || is_technical(next.text)
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
            is_technical(next.text)
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
        Some(b'h') => {
            word.eq_ignore_ascii_case("hd")
                && next.is_some_and(|text| text.eq_ignore_ascii_case("light"))
                || separated_video_codec(tokens, index).is_some()
        }
        Some(b'd') => {
            if !ascii_one_of(word, &["director", "directors"]) {
                return false;
            }
            next.is_some_and(|text| text.eq_ignore_ascii_case("cut"))
                || next.is_some_and(|text| text.eq_ignore_ascii_case("s"))
                    && tokens
                        .get(index + 2)
                        .is_some_and(|token| token.text.eq_ignore_ascii_case("cut"))
        }
        Some(b't') => {
            word.eq_ignore_ascii_case("theatrical")
                && next.is_some_and(|text| ascii_one_of(text, &["edition", "cut"]))
        }
        Some(b'u') => {
            word.eq_ignore_ascii_case("ultimate")
                && next.is_some_and(|text| ascii_one_of(text, &["collector", "collectors"]))
        }
        Some(b'c') => {
            word.eq_ignore_ascii_case("complete")
                && next.is_some_and(|text| {
                    is_technical(text)
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
        _ => false,
    }
}

fn separated_video_codec(tokens: &[Token<'_>], index: usize) -> Option<&'static str> {
    if !tokens.get(index)?.text.eq_ignore_ascii_case("h") {
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
                is_technical(next.text) || ascii_one_of(next.text, &["dl", "dubbed"])
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
    if token.get(..3).is_some_and(|prefix| {
        prefix.eq_ignore_ascii_case("dd+") || prefix.eq_ignore_ascii_case("ddp")
    }) {
        return true;
    }
    let mut buffer = [0u8; 16];
    let Some(key) = buffer.get_mut(..token.len()) else {
        return false;
    };
    for (output, input) in key.iter_mut().zip(token.bytes()) {
        *output = input.to_ascii_lowercase();
    }
    matches!(
        &*key,
        b"480p"
            | b"480i"
            | b"576p"
            | b"576i"
            | b"720p"
            | b"1080p"
            | b"1080i"
            | b"1440p"
            | b"2160p"
            | b"4320p"
            | b"4k"
            | b"8k"
            | b"bluray"
            | b"brrip"
            | b"bdrip"
            | b"bdmux"
            | b"brmux"
            | b"bdripmux"
            | b"brripmux"
            | b"bd"
            | b"web"
            | b"webdl"
            | b"webrip"
            | b"hdtv"
            | b"pdtv"
            | b"sdtv"
            | b"tvrip"
            | b"hdrip"
            | b"hdtvrip"
            | b"hdcam"
            | b"camrip"
            | b"hdlight"
            | b"mhd"
            | b"dvdrip"
            | b"dvdr"
            | b"dvdscr"
            | b"vhs"
            | b"pal"
            | b"ntsc"
            | b"dvd"
            | b"remux"
            | b"x264"
            | b"h264"
            | b"avc"
            | b"x265"
            | b"h265"
            | b"hevc"
            | b"av1"
            | b"vp9"
            | b"xvid"
            | b"divx"
            | b"hdr"
            | b"hdr10"
            | b"hdr10plus"
            | b"dv"
            | b"dovi"
            | b"dolbyvision"
            | b"sdr"
            | b"atmos"
            | b"truehd"
            | b"flac"
            | b"aac"
            | b"opus"
            | b"dts"
            | b"dd"
            | b"ddp"
            | b"ac3"
            | b"ac3d"
            | b"eac3"
            | b"dual"
            | b"proper"
            | b"repack"
            | b"remastered"
            | b"unrated"
            | b"extended"
            | b"directorscut"
            | b"uncut"
            | b"limited"
            | b"convert"
            | b"nfofix"
            | b"subbed"
            | b"imax"
            | b"10bit"
            | b"8bit"
            | b"nf"
            | b"amzn"
            | b"atvp"
            | b"dsnp"
            | b"hmax"
            | b"hulu"
            | b"cr"
    )
}

pub(super) fn is_release_year(token: &str) -> bool {
    token.len() == 4
        && token.bytes().all(|b| b.is_ascii_digit())
        && token
            .parse::<u16>()
            .is_ok_and(|year| (1870..=2035).contains(&year))
}

pub(super) fn fill_technical(
    candidate: &mut ParsedMedia,
    name: &str,
    tokens: &[Token<'_>],
    first_tech: Option<usize>,
    group: Option<&str>,
) {
    if let Some(value) = group {
        candidate.release_group = Some(value.to_owned());
    }
    for (i, token) in tokens.iter().enumerate() {
        if !token.bracketed && first_tech.is_some_and(|start| i < start) {
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
        let source = match key {
            b"bluray" | b"bdrip" | b"brrip" | b"bd" | b"bdmux" | b"brmux" | b"bdripmux"
            | b"brripmux" => Some("Blu-ray"),
            _ if combined_resolution.is_some() => Some("Blu-ray"),
            b"blu"
                if tokens
                    .get(i + 1)
                    .is_some_and(|next| next.text.eq_ignore_ascii_case("ray")) =>
            {
                Some("Blu-ray")
            }
            b"web"
                if tokens.get(i + 1).is_some_and(|next| {
                    next.before == Separator::Dash && next.text.eq_ignore_ascii_case("dl")
                }) =>
            {
                Some("WEB-DL")
            }
            b"webdl" | b"webrip" | b"web" => Some("Web"),
            b"hdtv" | b"pdtv" | b"hdtvrip" => Some("HDTV"),
            b"sdtv" | b"tvrip" => Some("SDTV"),
            b"dvd" | b"dvdrip" | b"dvdr" | b"dvdscr" => Some("DVD"),
            b"vhs" => Some("VHS"),
            b"hdcam" | b"camrip" => Some("CAM"),
            b"hdrip" => Some("HD Rip"),
            b"remux" => Some("Remux"),
            _ => None,
        };
        if let Some(source) = source {
            candidate.source.get_or_insert_with(|| source.to_owned());
        }
        let codec = match key {
            b"x264" | b"h264" | b"avc" => Some("H.264"),
            b"x265" | b"h265" | b"hevc" => Some("H.265"),
            b"av1" => Some("AV1"),
            b"vp9" => Some("VP9"),
            b"xvid" => Some("Xvid"),
            b"divx" => Some("DivX"),
            b"h" => separated_video_codec(tokens, i),
            _ => None,
        };
        if let Some(codec) = codec {
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

pub(super) fn release_group<'a>(
    name: &'a str,
    tokens: &[Token<'_>],
    stem_end: usize,
    episode_end: Option<usize>,
) -> Option<&'a str> {
    let stem = name.get(..stem_end)?;
    let dash = stem.rfind('-')?;
    let suffix = stem[dash + 1..].trim();
    let last_index = tokens.len().checked_sub(1)?;
    let last = &tokens[last_index];
    let fallback =
        (last.text == suffix && valid_group(suffix) && !is_web_dl_suffix(tokens, last_index))
            .then_some(suffix);
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
) -> Option<&'a str> {
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
            && !tokens[index + 1..]
                .iter()
                .any(|later| is_technical(later.text))
            && !tokens[index + 1..]
                .iter()
                .filter(|later| later.start >= end)
                .any(|later| later.text.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Some(group);
        }
    }
    None
}

fn group_start_word(tokens: &[Token<'_>], index: usize) -> bool {
    let text = tokens[index].text;
    if !text.chars().any(char::is_alphabetic)
        || is_technical(text)
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
    if matches!(
        &*key,
        b"mp2"
            | b"mp3"
            | b"mp4"
            | b"pcm"
            | b"lpcm"
            | b"hi10p"
            | b"internal"
            | b"int"
            | b"preair"
            | b"prooffix"
            | b"svid"
            | b"line"
            | b"hdma"
            | b"sdh"
    ) {
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
    is_technical(previous.text)
        || is_release_year(previous.text)
        || episode_end == Some(previous_index)
        || is_web_dl_suffix(tokens, previous_index)
        || previous_index
            .checked_sub(1)
            .is_some_and(|start| compound_technical(tokens, start))
}
