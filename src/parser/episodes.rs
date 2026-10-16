use crate::{
    lexer::{ReleaseToken as Token, Separator},
    model::{EpisodeNumbering, ExpectedKind},
};

use super::technical::{ascii_one_of, dimension, is_release_year};
use super::{filename_stem, token_group_start};

#[derive(Debug)]
pub(super) struct EpisodeMarker {
    pub(super) token_index: usize,
    pub(super) last_token_index: usize,
    pub(super) season: Option<u16>,
    pub(super) episodes: Vec<u16>,
    pub(super) is_pack: bool,
    pub(super) numbering: EpisodeNumbering,
    /// An explicit E/EP anchor can inherit the preceding marker's season.
    inherits_season: bool,
}

impl EpisodeMarker {
    fn new(
        token_range: (usize, usize),
        season: Option<u16>,
        episodes: Vec<u16>,
        is_pack: bool,
        numbering: EpisodeNumbering,
        inherits_season: bool,
    ) -> Self {
        Self {
            token_index: token_range.0,
            last_token_index: token_range.1,
            season,
            episodes,
            is_pack,
            numbering,
            inherits_season,
        }
    }
}

pub(super) fn episode_markers(
    name: &str,
    tokens: &[Token<'_>],
    technical_index: Option<usize>,
    expected: Option<ExpectedKind>,
    season_hint: Option<u16>,
    extra_boundary: Option<usize>,
) -> Vec<EpisodeMarker> {
    let mut markers = Vec::new();
    let mut consumed_until = None;
    let anime_prefix = expected.is_none()
        && name.starts_with('[')
        && tokens.first().is_some_and(|token| !token.is_technical());
    for (i, token) in tokens.iter().enumerate() {
        if consumed_until.is_some_and(|last| i <= last) {
            continue;
        }
        if extra_boundary.is_some_and(|boundary| i >= boundary) {
            break;
        }
        if token.bracketed
            && let Some(season) = ordinal_season(token.text)
            && let Some(next) = tokens.get(i + 1)
            && next.bracketed
            && separator_gap_is_dash(name, token, next)
            && let Some(episode) = episode_number(next.text)
        {
            consumed_until = Some(i + 1);
            markers.push(EpisodeMarker::new(
                (i, i + 1),
                Some(season),
                vec![episode],
                false,
                EpisodeNumbering::SeasonEpisode,
                false,
            ));
            continue;
        }
        if is_season_word(token.text)
            && let Some(next) = tokens.get(i + 1)
            && let Some(season) = season_number(next.text).or_else(|| roman_season(next.text))
        {
            let episode = tokens.get(i + 2).and_then(|anchor| {
                if ascii_one_of(anchor.text, &["episode", "episodes", "ep"]) {
                    tokens.get(i + 3).and_then(|number| {
                        episode_number(number.text).map(|episode| (i + 3, episode))
                    })
                } else {
                    parse_e_token(anchor.text).map(|episode| (i + 2, episode))
                }
            });
            if season > 99 && episode.is_none() && expected != Some(ExpectedKind::Series) {
                continue;
            }
            let mut episodes = episode.map_or_else(Vec::new, |(_, episode)| vec![episode]);
            let last = episode.map_or(i + 1, |(anchor, _)| {
                extend_episode_suffixes(name, tokens, anchor, &mut episodes)
            });
            consumed_until = Some(last);
            markers.push(EpisodeMarker::new(
                (i, last),
                Some(season),
                episodes,
                episode.is_none(),
                if episode.is_some() {
                    EpisodeNumbering::SeasonEpisode
                } else {
                    EpisodeNumbering::SeasonUnspecified
                },
                false,
            ));
            continue;
        }
        if let Some((season, episodes)) = parse_sxxe_x(token.text) {
            markers.push(EpisodeMarker::new(
                (i, i),
                Some(season),
                episodes,
                false,
                EpisodeNumbering::SeasonEpisode,
                false,
            ));
            continue;
        }
        if let Some((season, mut episodes)) = parse_sxe(token.text) {
            let last = extend_episode_suffixes(name, tokens, i, &mut episodes);
            if last > i {
                consumed_until = Some(last);
            }
            markers.push(EpisodeMarker::new(
                (i, last),
                Some(season),
                episodes,
                false,
                EpisodeNumbering::SeasonEpisode,
                false,
            ));
            continue;
        }
        if token.text.len() == 5
            && token.text.as_bytes()[0].eq_ignore_ascii_case(&b's')
            && token.text[1..].bytes().all(|byte| byte.is_ascii_digit())
            && !(is_release_year(&token.text[1..])
                && tokens
                    .get(i + 1)
                    .is_some_and(|next| parse_e_token(next.text).is_some()))
        {
            let season = token.text[1..3].parse::<u16>().ok();
            let episode = token.text[3..5].parse::<u16>().ok();
            if let (Some(season), Some(episode)) = (season, episode) {
                markers.push(EpisodeMarker::new(
                    (i, i),
                    Some(season),
                    vec![episode],
                    false,
                    EpisodeNumbering::SeasonEpisode,
                    false,
                ));
                continue;
            }
        }
        if let Some((season, mut episodes)) = parse_x_episode_sequence(token.text)
            && (season <= 99
                || is_release_year(token.text.split(['x', 'X']).next().unwrap_or_default()))
            && !episodes.is_empty()
            && episodes.iter().all(|episode| *episode <= 999)
            && dimension(token.text).is_none()
        {
            let last = extend_episode_suffixes(name, tokens, i, &mut episodes);
            consumed_until = Some(last);
            markers.push(EpisodeMarker::new(
                (i, last),
                Some(season),
                episodes,
                false,
                EpisodeNumbering::SeasonEpisode,
                false,
            ));
            continue;
        }
        if let Some(season) = season_hint
            && let Some(episode) = compact_episode_for_season(token.text, season)
        {
            markers.push(EpisodeMarker::new(
                (i, i),
                Some(season),
                vec![episode],
                false,
                EpisodeNumbering::SeasonEpisode,
                false,
            ));
            continue;
        }
        if ascii_one_of(token.text, &["episode", "episodes", "ep"])
            && let Some(next) = tokens.get(i + 1)
            && let Some(episode) = episode_number(next.text)
        {
            let mut episodes = vec![episode];
            let last = extend_episode_suffixes(name, tokens, i + 1, &mut episodes);
            consumed_until = Some(last);
            markers.push(EpisodeMarker::new(
                (i, last),
                None,
                episodes,
                false,
                EpisodeNumbering::SeasonUnspecified,
                false,
            ));
            continue;
        }
        if let Some(mut episodes) = parse_e_sequence(token.text) {
            let follows_season = i > 0 && parse_s_token(tokens[i - 1].text).is_some();
            if !follows_season {
                let last = extend_episode_suffixes(name, tokens, i, &mut episodes);
                consumed_until = Some(last);
                markers.push(EpisodeMarker::new(
                    (i, last),
                    None,
                    episodes,
                    false,
                    EpisodeNumbering::SeasonUnspecified,
                    true,
                ));
            }
            continue;
        }
        if let Some(season) = parse_s_token(token.text).or_else(|| {
            let digits = token.text.strip_prefix(['S', 's'])?;
            (is_release_year(digits)
                && tokens
                    .get(i + 1)
                    .is_some_and(|next| season_episode_token(next.text).is_some()))
            .then(|| digits.parse().ok())
            .flatten()
        }) {
            if let Some(next) = tokens.get(i + 1)
                && let Some(episode) = season_episode_token(next.text)
            {
                let mut episodes = vec![episode];
                let last = extend_episode_suffixes(name, tokens, i + 1, &mut episodes);
                consumed_until = Some(last);
                markers.push(EpisodeMarker::new(
                    (i, last),
                    Some(season),
                    episodes,
                    false,
                    EpisodeNumbering::SeasonEpisode,
                    false,
                ));
            } else if let Some(next) = tokens.get(i + 1)
                && let Ok(episode) = next.text.parse::<u16>()
                && separator_gap_is_dash(name, token, next)
                && episode < 1000
            {
                markers.push(EpisodeMarker::new(
                    (i, i + 1),
                    Some(season),
                    vec![episode],
                    false,
                    EpisodeNumbering::SeasonEpisode,
                    false,
                ));
            } else {
                markers.push(EpisodeMarker::new(
                    (i, i),
                    Some(season),
                    Vec::new(),
                    true,
                    EpisodeNumbering::SeasonUnspecified,
                    false,
                ));
            }
        }
        if let Some(episode) = episode_number(token.text)
            && i > 0
            && technical_index.is_some_and(|technical| i < technical)
            && expected != Some(ExpectedKind::Movie)
            && ((token.bracketed && isolated_bracket_number(name, token))
                || (anime_prefix
                    && !token.bracketed
                    && technical_index == Some(i + 1)
                    && !tokens[i - 1].bracketed))
        {
            markers.push(EpisodeMarker::new(
                (i, i),
                None,
                vec![episode],
                false,
                EpisodeNumbering::Absolute,
                false,
            ));
            continue;
        }
        if (expected == Some(ExpectedKind::Series) || anime_prefix)
            && !token.bracketed
            && i > 0
            && separator_gap_is_dash(name, &tokens[i - 1], token)
            && let Some(episode) = episode_number(token.text)
            && technical_index.is_some_and(|technical| i < technical)
            && !(i > 0
                && parse_s_token(tokens[i - 1].text).is_some()
                && separator_gap_is_dash(name, &tokens[i - 1], token))
        {
            let season = season_hint;
            let mut episodes = vec![episode];
            let last = extend_episode_suffixes(name, tokens, i, &mut episodes);
            consumed_until = Some(last);
            markers.push(EpisodeMarker::new(
                (i, last),
                season,
                episodes,
                false,
                if season.is_some() {
                    EpisodeNumbering::SeasonEpisode
                } else {
                    EpisodeNumbering::Absolute
                },
                false,
            ));
        }
    }
    coalesce_episode_markers(name, tokens, markers)
}

fn isolated_bracket_number(name: &str, token: &Token<'_>) -> bool {
    let start = token_group_start(name, token);
    start > 0
        && name
            .get(start + 1..token.start)
            .is_some_and(|prefix| prefix.trim().is_empty())
        && name
            .get(token.end..)
            .is_some_and(|suffix| suffix.trim_start().starts_with([']', ')', '}']))
}

fn coalesce_episode_markers(
    name: &str,
    tokens: &[Token<'_>],
    markers: Vec<EpisodeMarker>,
) -> Vec<EpisodeMarker> {
    if markers.len() < 2 {
        return markers;
    }
    let mut combined: Vec<EpisodeMarker> = Vec::with_capacity(markers.len());
    for marker in markers {
        let Some(previous) = combined.last_mut() else {
            combined.push(marker);
            continue;
        };
        let same_seasoning = previous.season == marker.season
            && previous.numbering == marker.numbering
            || previous.season.is_some()
                && previous.numbering == EpisodeNumbering::SeasonEpisode
                && marker.numbering == EpisodeNumbering::SeasonUnspecified
                && marker.inherits_season;
        let adjacent = !previous.is_pack
            && !marker.is_pack
            && same_seasoning
            && marker.token_index > previous.last_token_index
            && episode_marker_gap(name, tokens, previous.last_token_index, marker.token_index);
        if adjacent {
            if separator_gap_is_dash(
                name,
                &tokens[previous.last_token_index],
                &tokens[marker.token_index],
            ) && marker.episodes.len() == 1
                && previous
                    .episodes
                    .last()
                    .is_some_and(|start| *start < marker.episodes[0])
            {
                let start = *previous.episodes.last().unwrap();
                let end = marker.episodes[0];
                previous.episodes.extend(start + 1..=end);
            } else {
                previous.episodes.extend(marker.episodes);
            }
            previous.last_token_index = marker.last_token_index;
        } else {
            combined.push(marker);
        }
    }
    combined
}

fn episode_marker_gap(name: &str, tokens: &[Token<'_>], previous: usize, next: usize) -> bool {
    let Some(gap) = tokens
        .get(previous)
        .zip(tokens.get(next))
        .and_then(|(left, right)| name.get(left.end..right.start))
    else {
        return false;
    };
    gap.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '_'))
        .eq_ignore_ascii_case("and")
        || !gap.is_empty()
            && gap.chars().all(|ch| {
                ch.is_whitespace()
                    || matches!(
                        ch,
                        '.' | ','
                            | ';'
                            | '&'
                            | '+'
                            | '-'
                            | '‐'
                            | '‑'
                            | '‒'
                            | '–'
                            | '—'
                            | '_'
                            | '/'
                            | '\\'
                            | '['
                            | ']'
                            | '('
                            | ')'
                            | '{'
                            | '}'
                    )
            })
}

fn parse_episode_suffix(text: &str) -> Option<u16> {
    parse_e_token(text).or_else(|| episode_number(text))
}

fn strip_episode_revision(text: &str) -> &str {
    if text
        .as_bytes()
        .last()
        .is_none_or(|byte| !byte.is_ascii_digit())
    {
        return text;
    }
    let Some(offset) = text
        .bytes()
        .position(|byte| byte.eq_ignore_ascii_case(&b'v'))
    else {
        return text;
    };
    let revision = &text[offset + 1..];
    if offset > 0 && !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit()) {
        &text[..offset]
    } else {
        text
    }
}

pub(super) fn episode_number(text: &str) -> Option<u16> {
    if !text.as_bytes().first()?.is_ascii_digit() {
        return None;
    }
    let digits = strip_episode_revision(text);
    (digits.len() <= 3 && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse::<u16>().ok())
        .flatten()
}

fn season_number(text: &str) -> Option<u16> {
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u16>()
        .ok()
        .filter(|season| *season <= 99 || is_release_year(text))
}

fn ordinal_season(text: &str) -> Option<u16> {
    if text.len() < 3
        || !text.as_bytes().first()?.is_ascii_digit()
        || !text.as_bytes().last()?.is_ascii_alphabetic()
    {
        return None;
    }
    let suffix_start = text.len().checked_sub(2)?;
    let digits = text.get(..suffix_start)?;
    let season = season_number(digits).filter(|number| (1..=99).contains(number))?;
    let suffix = if (11..=13).contains(&(season % 100)) {
        "th"
    } else {
        match season % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    text.get(suffix_start..)?
        .eq_ignore_ascii_case(suffix)
        .then_some(season)
}

pub(super) fn roman_season(text: &str) -> Option<u16> {
    let mut buffer = [0u8; 8];
    let key = buffer.get_mut(..text.len())?;
    for (output, input) in key.iter_mut().zip(text.bytes()) {
        *output = input.to_ascii_uppercase();
    }
    let mut remaining = &*key;
    let tens = if let Some(rest) = remaining.strip_prefix(b"XC") {
        remaining = rest;
        90
    } else if let Some(rest) = remaining.strip_prefix(b"XL") {
        remaining = rest;
        40
    } else {
        let mut value = 0;
        if let Some(rest) = remaining.strip_prefix(b"L") {
            remaining = rest;
            value = 50;
        }
        for _ in 0..3 {
            if let Some(rest) = remaining.strip_prefix(b"X") {
                remaining = rest;
                value += 10;
            }
        }
        value
    };
    let units = match remaining {
        b"" => 0,
        b"I" => 1,
        b"II" => 2,
        b"III" => 3,
        b"IV" => 4,
        b"V" => 5,
        b"VI" => 6,
        b"VII" => 7,
        b"VIII" => 8,
        b"IX" => 9,
        _ => return None,
    };
    let season = tens + units;
    (season > 0).then_some(season)
}

fn is_season_word(text: &str) -> bool {
    match text.len() {
        6 => ascii_one_of(text, &["season", "saison"]),
        7 => ascii_one_of(text, &["seasons", "saisons"]),
        9 => text.eq_ignore_ascii_case("temporada"),
        _ => false,
    }
}

fn extend_episode_suffixes(
    name: &str,
    tokens: &[Token<'_>],
    start: usize,
    episodes: &mut Vec<u16>,
) -> usize {
    let mut last = start;
    while let Some(next) = tokens.get(last + 1)
        && let Some(episode) = parse_episode_suffix(next.text)
    {
        let gap = &name[tokens[last].end..next.start];
        if separator_gap_is_dash(name, &tokens[last], next) {
            let bare = parse_e_token(next.text).is_none();
            if bare && gap.chars().any(char::is_whitespace) {
                break;
            }
            append_episode_range(episodes, episode);
        } else if gap.contains(['+', '&'])
            && gap
                .chars()
                .all(|ch| ch.is_whitespace() || matches!(ch, '+' | '&'))
        {
            episodes.push(episode);
        } else {
            break;
        }
        last += 1;
    }
    last
}

pub(super) fn separator_gap_is_dash(name: &str, left: &Token<'_>, right: &Token<'_>) -> bool {
    right.before == Separator::Dash
        || name.get(left.end..right.start).is_some_and(|gap| {
            gap.contains(['-', '‐', '‑', '–', '—'])
                && gap.chars().all(|ch| {
                    ch.is_whitespace() || matches!(ch, '.' | '_' | '-' | '‐' | '‑' | '–' | '—')
                })
        })
}

fn append_episode_range(episodes: &mut Vec<u16>, end: u16) {
    if let Some(&start) = episodes.last()
        && end > start.saturating_add(1)
    {
        episodes.extend(start + 1..=end);
    } else {
        episodes.push(end);
    }
}

pub(super) fn parse_s_token(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    (bytes.len() > 1
        && bytes[0].eq_ignore_ascii_case(&b's')
        && bytes[1..].iter().all(u8::is_ascii_digit))
    .then(|| season_number(&text[1..]))
    .flatten()
    .filter(|season| *season <= 99)
}

fn parse_e_token(text: &str) -> Option<u16> {
    let digit_start = if text
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("ep"))
    {
        2
    } else if text
        .get(..1)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("e"))
    {
        1
    } else {
        return None;
    };
    episode_number(text.get(digit_start..)?)
}

fn season_episode_token(text: &str) -> Option<u16> {
    parse_e_token(text).or_else(|| text.strip_prefix(['x', 'X']).and_then(episode_number))
}

fn parse_e_sequence(text: &str) -> Option<Vec<u16>> {
    if !text.as_bytes().first()?.eq_ignore_ascii_case(&b'e') {
        return None;
    }
    if let Some(episode) = parse_e_token(text) {
        return Some(vec![episode]);
    }
    let text = strip_episode_revision(text);
    let bytes = text.as_bytes();
    if !bytes.first()?.eq_ignore_ascii_case(&b'e') {
        return None;
    }
    parse_episode_sequence(
        text,
        1 + usize::from(bytes.get(1).is_some_and(|b| b.eq_ignore_ascii_case(&b'p'))),
    )
}

pub(super) fn parse_sxe(text: &str) -> Option<(u16, Vec<u16>)> {
    if !text.as_bytes().first()?.eq_ignore_ascii_case(&b's') {
        return None;
    }
    let text = strip_episode_revision(text);
    let bytes = text.as_bytes();
    if bytes.len() < 4 || !bytes[0].eq_ignore_ascii_case(&b's') {
        return None;
    }
    let e = bytes.iter().position(|b| b.eq_ignore_ascii_case(&b'e'))?;
    if e <= 1 || e + 1 >= bytes.len() || !bytes[1..e].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let season = season_number(&text[1..e])?;
    parse_episode_sequence(
        text,
        e + 1
            + usize::from(
                bytes
                    .get(e + 1)
                    .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'p')),
            ),
    )
    .map(|episodes| (season, episodes))
}

fn parse_episode_sequence(text: &str, mut cursor: usize) -> Option<Vec<u16>> {
    let bytes = text.as_bytes();
    let mut episodes = Vec::new();
    while cursor < bytes.len() {
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if start == cursor {
            return None;
        }
        episodes.push(text[start..cursor].parse::<u16>().ok()?);
        if cursor == bytes.len() {
            break;
        }
        let next = bytes[cursor];
        if next.eq_ignore_ascii_case(&b'e') || next == b'-' {
            cursor += 1;
            if next.eq_ignore_ascii_case(&b'e')
                && bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'p'))
            {
                cursor += 1;
            }
        } else {
            return None;
        }
    }
    (!episodes.is_empty() && episodes.iter().all(|episode| *episode <= 999)).then_some(episodes)
}

fn parse_sxxe_x(text: &str) -> Option<(u16, Vec<u16>)> {
    let bytes = text.as_bytes();
    if !bytes.first()?.eq_ignore_ascii_case(&b's') {
        return None;
    }
    let mut cursor = 1;
    let season_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == season_start || !bytes.get(cursor)?.eq_ignore_ascii_case(&b'x') {
        return None;
    }
    let season = text[season_start..cursor].parse::<u16>().ok()?;
    cursor += 1;
    let mut episodes = Vec::new();
    loop {
        if bytes.get(cursor)?.eq_ignore_ascii_case(&b'e') {
            cursor += 1;
        }
        let episode_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == episode_start {
            return None;
        }
        episodes.push(text[episode_start..cursor].parse::<u16>().ok()?);
        if cursor == bytes.len() {
            break;
        }
        if !bytes.get(cursor)?.eq_ignore_ascii_case(&b'x') {
            return None;
        }
        cursor += 1;
    }
    (season <= 99 && episodes.iter().all(|episode| *episode <= 999)).then_some((season, episodes))
}

pub(super) fn parse_x_format(text: &str) -> Option<(u16, u16)> {
    let x = text.bytes().position(|b| b.eq_ignore_ascii_case(&b'x'))?;
    if x == 0 || x + 1 >= text.len() {
        return None;
    }
    let (s, e) = text.split_at(x);
    let e = &e[1..];
    if !s.bytes().all(|b| b.is_ascii_digit()) || !e.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((s.parse().ok()?, e.parse().ok()?))
}

fn parse_x_episode_sequence(text: &str) -> Option<(u16, Vec<u16>)> {
    if !text.as_bytes().first()?.is_ascii_digit() {
        return None;
    }
    let text = strip_episode_revision(text);
    let mut parts = text.split(|ch: char| ch.eq_ignore_ascii_case(&'x'));
    let season = parts.next()?;
    if season.is_empty() || !season.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let season = season_number(season)?;
    let episodes: Vec<u16> = parts
        .map(|part| {
            (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse::<u16>().ok())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (!episodes.is_empty()).then_some((season, episodes))
}

fn compact_episode_for_season(text: &str, season: u16) -> Option<u16> {
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let digits = text.as_bytes();
    let prefix_len = season.to_string().len();
    if digits.len() != prefix_len + 2 || text.get(..prefix_len)?.parse::<u16>().ok()? != season {
        return None;
    }
    text.get(prefix_len..)?
        .parse::<u16>()
        .ok()
        .filter(|episode| *episode <= 99)
}

pub(super) fn season_directory_number(name: &str) -> Option<u16> {
    let (stem, _) = filename_stem(name);
    let mut words = stem
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .peekable();
    while let Some(word) = words.next() {
        if is_season_word(word)
            && let Some(next) = words.peek()
            && let Some(season) = season_number(next).or_else(|| roman_season(next))
        {
            return Some(season);
        }
        if let Some(season) = parse_s_token(word) {
            return Some(season);
        }
    }
    None
}
