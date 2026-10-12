use crate::{
    lexer::{Token, tokenize},
    model::{
        EpisodeNumbering, ExpectedKind, ExtraCategory, ExtraInfo, MediaKind, ParseContext,
        ParsedMedia,
    },
};

mod episodes;
mod technical;

use episodes::{episode_markers, season_directory_number, separator_gap_is_dash};
use technical::{
    ascii_one_of, ascii_starts_with, fill_technical, first_technical_token, is_release_year,
    is_technical, leading_content_token, release_group,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct Candidate {
    media: ParsedMedia,
    score: i32,
    title_span: Option<Span>,
}

/// Parse a release name or the final component of a path-like string.
///
/// Common media and subtitle extensions are recognized. Structural and release
/// markers select one best interpretation.
pub fn parse(input: &str) -> ParsedMedia {
    parse_with_context(input, &ParseContext::default())
}

/// Parse a name with movie/series hints and local ancestor context.
///
/// Structurally recognizable ancestors can supply a missing title or season.
///
/// ```
/// use cinekey::{ExpectedKind, ParseContext, parse_with_context};
///
/// let context = ParseContext {
///     expected_kind: Some(ExpectedKind::Series),
///     ancestors: vec!["Season 1", "Example.Show.S01.1080p"],
/// };
/// let media = parse_with_context("E02.mkv", &context);
/// assert_eq!(media.title.as_deref(), Some("Example Show"));
/// assert_eq!(media.season, Some(1));
/// assert_eq!(media.episodes, [2]);
/// ```
pub fn parse_with_context(input: &str, context: &ParseContext<'_>) -> ParsedMedia {
    let season_hint = context
        .ancestors
        .iter()
        .find_map(|ancestor| season_directory_number(ancestor));
    let mut candidates = parse_internal(input, context.expected_kind, season_hint);
    if let Some(season) = season_hint {
        for candidate in &mut candidates {
            if candidate.media.kind == MediaKind::Episode
                && candidate.media.season.is_none()
                && candidate.media.episode_numbering == Some(EpisodeNumbering::SeasonUnspecified)
            {
                candidate.media.season = Some(season);
                candidate.media.episode_numbering = Some(EpisodeNumbering::SeasonEpisode);
                candidate.score += 8;
            }
        }
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    }
    if !candidates
        .iter()
        .any(|candidate| candidate.media.title.is_some())
    {
        for ancestor in &context.ancestors {
            if is_context_directory(ancestor) {
                continue;
            }
            let Some(parent) = parse_internal(ancestor, context.expected_kind, None)
                .into_iter()
                .find(|candidate| {
                    candidate.media.title.is_some() && candidate.media.kind != MediaKind::Unknown
                })
            else {
                continue;
            };
            let ParsedMedia {
                title: Some(title),
                title_annotations,
                ..
            } = parent.media
            else {
                continue;
            };
            let candidate = &mut candidates[0];
            candidate.media.title = Some(title);
            candidate.media.title_annotations = title_annotations;
            break;
        }
    }
    candidates
        .into_iter()
        .next()
        .expect("every input has a structural or unknown interpretation")
        .media
}

fn parse_internal(
    input: &str,
    expected: Option<ExpectedKind>,
    season_hint: Option<u16>,
) -> Vec<Candidate> {
    let (name, extension_offset) = filename_stem(input);
    let tokens = tokenize(name);
    let mut candidates = Vec::with_capacity(1);
    let tech_index = first_technical_token(&tokens);
    let extra = extra_marker(name, &tokens, tech_index, expected);
    let extra_boundary = extra.as_ref().map(|(index, start, _)| {
        if tokens[*index].bracketed {
            tokens.partition_point(|token| token.start < *start)
        } else {
            *index
        }
    });
    let markers = episode_markers(
        name,
        &tokens,
        tech_index,
        expected,
        season_hint,
        extra_boundary,
    );
    let group = release_group(name, &tokens, extension_offset);
    let mut episode_marker = None;
    let mut pack_marker = None;
    for marker in markers {
        let selected = if marker.is_pack {
            &mut pack_marker
        } else {
            &mut episode_marker
        };
        selected.get_or_insert(marker);
    }

    if let Some(marker) = episode_marker {
        let boundary = marker.token_index;
        let title_year = boundary.checked_sub(1).filter(|index| {
            *index > leading_content_token(&tokens) && is_release_year(tokens[*index].text)
        });
        let title_boundary = title_year.unwrap_or(boundary);
        let mut title = title_before(name, &tokens, title_boundary);
        let mut title_end = tokens
            .get(title_boundary)
            .map_or(0, |token| token_group_start(name, token));
        if marker.numbering == EpisodeNumbering::Absolute
            && let Some(main_title_end) = spaced_dash_offset(&name[..title_end])
            && let Some(main_title) = title_before_offset(name, main_title_end)
        {
            title = Some(main_title);
            title_end = main_title_end;
        }
        let mut candidate = empty_candidate(MediaKind::Episode, 82);
        candidate.media.title = title;
        candidate.media.season = marker.season;
        candidate.media.episode_numbering = Some(marker.numbering);
        match marker.numbering {
            EpisodeNumbering::SeasonEpisode | EpisodeNumbering::SeasonUnspecified => {
                candidate.media.episodes = marker.episodes;
            }
            EpisodeNumbering::Absolute => candidate.media.absolute_episodes = marker.episodes,
        }
        if let Some(year_index) = title_year {
            candidate.media.year = tokens[year_index].text.parse::<u16>().ok();
        }

        if candidate.media.title.is_some() {
            candidate.title_span = Some(Span {
                start: 0,
                end: title_end,
            });
        }
        let year_index = tokens
            .iter()
            .enumerate()
            .skip(marker.last_token_index + 1)
            .take_while(|(index, _)| tech_index.is_none_or(|tech| *index < tech))
            .find_map(|(index, token)| is_release_year(token.text).then_some(index));
        if let Some(index) = year_index.filter(|_| title_year.is_none()) {
            candidate.media.year = tokens[index].text.parse::<u16>().ok();
        }
        if let Some(value) = episode_title(
            name,
            &tokens,
            marker.last_token_index,
            tech_index,
            year_index,
        ) {
            candidate.media.episode_title = Some(value);
        }
        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate.media, name, &tokens, tech_index, group);

        candidates.push(candidate);
    }

    if let Some(marker) = pack_marker {
        let mut candidate = empty_candidate(MediaKind::SeasonPack, 65);
        let year_before = marker.token_index.checked_sub(1).filter(|index| {
            *index > leading_content_token(&tokens) && is_release_year(tokens[*index].text)
        });
        let year_index = year_before.or_else(|| {
            tokens
                .iter()
                .enumerate()
                .skip(marker.last_token_index + 1)
                .take_while(|(index, _)| tech_index.is_none_or(|tech| *index < tech))
                .find_map(|(index, token)| is_release_year(token.text).then_some(index))
        });
        let boundary = year_before
            .into_iter()
            .chain(tech_index)
            .chain(std::iter::once(marker.token_index))
            .min()
            .unwrap_or(marker.token_index);
        candidate.media.title = title_before(name, &tokens, boundary);
        if candidate.media.title.is_some() {
            candidate.title_span = Some(Span {
                start: 0,
                end: tokens
                    .get(boundary)
                    .map_or(name.len(), |token| token_group_start(name, token)),
            });
        }
        if let Some(index) = year_index {
            candidate.media.year = tokens[index].text.parse().ok();
        }
        candidate.media.season = marker.season;

        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate.media, name, &tokens, tech_index, group);

        candidates.push(candidate);
    }

    if candidates.is_empty() && extra.is_none() {
        let movie_marker = explicit_movie_marker(&tokens);
        let content_start = leading_content_token(&tokens);
        let numeric_title = tech_index.unwrap_or(tokens.len()) == content_start + 1;
        let mut year_indexes = tokens.iter().enumerate().filter_map(|(i, token)| {
            (is_release_year(token.text) && !(numeric_title && i == content_start)).then_some(i)
        });
        let first_year = year_indexes.next();
        for year_index in std::iter::once(first_year).chain(year_indexes.map(Some)) {
            let boundary = year_index
                .into_iter()
                .chain(tech_index)
                .min()
                .unwrap_or(tokens.len());
            let mut candidate = empty_candidate(
                if expected == Some(ExpectedKind::Series)
                    && year_index.is_none()
                    && movie_marker.is_none()
                {
                    MediaKind::Unknown
                } else {
                    MediaKind::Movie
                },
                if movie_marker.is_some() {
                    58
                } else if year_index.is_some() {
                    56
                } else {
                    28
                },
            );
            candidate.media.title = title_before(name, &tokens, boundary);
            let mut title_start = 0;
            let mut title_end = tokens
                .get(boundary)
                .map_or(name.len(), |token| token_group_start(name, token));
            if let Some(index) = year_index
                && index == leading_content_token(&tokens)
                && let Some(next) = tokens.get(index + 1)
                && separator_gap_is_dash(name, &tokens[index], next)
            {
                let end = tech_index
                    .and_then(|tech| tokens.get(tech))
                    .map_or(name.len(), |token| token_group_start(name, token));
                candidate.media.title = name.get(next.start..end).and_then(clean_title);
                title_start = next.start;
                title_end = end;
            }
            if let Some(index) = year_index {
                let year = tokens[index].text.parse::<u16>().ok();
                candidate.media.year = year;
            }
            if candidate.media.title.is_some() {
                candidate.title_span = Some(Span {
                    start: title_start,
                    end: title_end,
                });
            }
            candidate.score += expected_score(expected, ExpectedKind::Movie);
            if let Some(index) = year_index {
                if candidate.media.title.is_some() {
                    candidate.score += 10;
                } else {
                    candidate.score -= 24;
                }
                if let Some(tech) = tech_index {
                    candidate.score +=
                        12i32.saturating_sub((tech.saturating_sub(index) as i32) * 2);
                }
            }
            fill_technical(&mut candidate.media, name, &tokens, tech_index, group);

            candidates.push(candidate);
        }
    }

    if candidates.is_empty()
        && let Some((index, group_start, info)) = extra
    {
        let mut candidate = empty_candidate(MediaKind::Extra, 61);
        let year_index = tokens[..index]
            .iter()
            .rposition(|token| token.start < group_start && is_release_year(token.text));
        let title_end = year_index
            .map_or(group_start, |index| token_group_start(name, &tokens[index]))
            .min(
                tech_index
                    .and_then(|index| tokens.get(index))
                    .map_or(group_start, |token| token_group_start(name, token)),
            );
        candidate.media.title = title_before_offset(name, title_end);
        if let Some(index) = year_index {
            candidate.media.year = tokens[index].text.parse().ok();
        }
        candidate.media.extra = Some(info);

        if candidate.media.title.is_some() {
            candidate.title_span = Some(Span {
                start: 0,
                end: title_end,
            });
        }
        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate.media, name, &tokens, tech_index, group);

        candidates.push(candidate);
    }

    if candidates.is_empty() {
        let mut candidate = empty_candidate(MediaKind::Unknown, 0);
        candidate.media.title = title_before(name, &tokens, tech_index.unwrap_or(tokens.len()));
        fill_technical(&mut candidate.media, name, &tokens, tech_index, group);

        candidates.push(candidate);
    }

    for candidate in &mut candidates {
        separate_title_annotations(name, candidate);
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    candidates.dedup_by(|a, b| {
        a.media.kind == b.media.kind
            && a.media.title == b.media.title
            && a.media.year == b.media.year
            && a.media.season == b.media.season
            && a.media.episodes == b.media.episodes
    });
    candidates
}

fn explicit_movie_marker(tokens: &[Token<'_>]) -> Option<(usize, Option<u16>)> {
    tokens.iter().enumerate().find_map(|(index, token)| {
        if token.bracketed || !token.text.eq_ignore_ascii_case("movie") {
            return None;
        }
        let ordinal = tokens
            .get(index + 1)
            .filter(|next| next.text.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|next| next.text.parse::<u16>().ok())
            .filter(|number| (1..=50).contains(number));
        ordinal.map(|ordinal| (index, Some(ordinal)))
    })
}

fn unbracketed_extra(
    tokens: &[Token<'_>],
    index: usize,
) -> Option<(ExtraCategory, usize, Option<u16>)> {
    let token = tokens.get(index)?;
    let text = token.text;
    let start = token.start;
    let number_after = || {
        tokens
            .get(index + 1)
            .and_then(|next| next.text.parse::<u16>().ok())
    };

    if text.eq_ignore_ascii_case("menu")
        && index >= 2
        && tokens[index - 2].text.eq_ignore_ascii_case("vol")
    {
        return Some((ExtraCategory::Menu, tokens[index - 2].start, None));
    }
    if index >= 2
        && tokens[index - 2].text.eq_ignore_ascii_case("non")
        && tokens[index - 1].text.eq_ignore_ascii_case("credit")
        && (text.eq_ignore_ascii_case("op") || text.eq_ignore_ascii_case("ed"))
    {
        let category = if text.eq_ignore_ascii_case("op") {
            ExtraCategory::Opening
        } else {
            ExtraCategory::Ending
        };
        return Some((category, tokens[index - 2].start, number_after()));
    }
    if text.eq_ignore_ascii_case("op") || text.eq_ignore_ascii_case("ed") {
        let has_cjk_prefix = index > 0 && !tokens[index - 1].text.is_ascii();
        if has_cjk_prefix {
            let category = if text.eq_ignore_ascii_case("op") {
                ExtraCategory::Opening
            } else {
                ExtraCategory::Ending
            };
            return Some((category, tokens[index - 1].start, number_after()));
        }
    }
    if index > 0 && !tokens[index - 1].text.is_ascii() {
        for (marker, category) in [
            ("OP", ExtraCategory::Opening),
            ("ED", ExtraCategory::Ending),
        ] {
            if ascii_starts_with(text, &[marker]) {
                let suffix = &text[marker.len()..];
                if suffix.bytes().all(|byte| byte.is_ascii_digit()) && suffix.len() <= 3 {
                    let number = (!suffix.is_empty())
                        .then(|| suffix.parse::<u16>().ok())
                        .flatten();
                    return Some((
                        category,
                        if tokens[index - 1].text.starts_with("ノンテロ") {
                            tokens[index - 1].start
                        } else {
                            start
                        },
                        number,
                    ));
                }
            }
        }
    }
    for (marker, category) in [
        ("OP", ExtraCategory::Opening),
        ("ED", ExtraCategory::Ending),
    ] {
        if let Some(offset) = find_ascii_marker(text, marker) {
            let prefix = &text[..offset];
            let suffix = &text[offset + marker.len()..];
            if !prefix.is_ascii()
                && suffix.bytes().all(|byte| byte.is_ascii_digit())
                && suffix.len() <= 3
            {
                let number = (!suffix.is_empty())
                    .then(|| suffix.parse::<u16>().ok())
                    .flatten();
                return Some((
                    category,
                    if prefix.starts_with("ノンテロ") {
                        start
                    } else {
                        start + offset
                    },
                    number,
                ));
            }
        }
    }
    if ascii_starts_with(text, &["PV"])
        && (text.len() > 2
            || tokens
                .get(index + 1)
                .is_some_and(|next| !next.text.is_ascii()))
    {
        return Some((ExtraCategory::PromotionalVideo, start, None));
    }
    for (marker, category) in [
        ("PV", ExtraCategory::PromotionalVideo),
        ("CM", ExtraCategory::Commercial),
    ] {
        if let Some(offset) = find_ascii_marker(text, marker) {
            let prefix = &text[..offset];
            let suffix = &text[offset + marker.len()..];
            let cjk_collection = marker == "CM"
                && offset == 0
                && suffix.starts_with('集')
                && suffix.chars().all(|ch| !ch.is_ascii());
            let cjk_prefixed_collection = marker == "CM"
                && !prefix.is_empty()
                && !prefix.is_ascii()
                && suffix.starts_with('集')
                && suffix.chars().all(|ch| !ch.is_ascii());
            if (!prefix.is_ascii() && suffix.chars().all(|ch| !ch.is_ascii()) && offset > 0)
                || cjk_collection
            {
                return Some((
                    category,
                    if cjk_prefixed_collection {
                        start
                    } else {
                        start + offset
                    },
                    None,
                ));
            }
        }
    }
    if text.eq_ignore_ascii_case("information") {
        return Some((ExtraCategory::Other, start, number_after()));
    }
    None
}

fn find_ascii_marker(text: &str, marker: &str) -> Option<usize> {
    text.char_indices().find_map(|(offset, _)| {
        let end = offset.checked_add(marker.len())?;
        text.get(offset..end)
            .filter(|part| part.eq_ignore_ascii_case(marker))
            .map(|_| offset)
    })
}

fn strong_extra_label(tokens: &[Token<'_>], index: usize) -> bool {
    let text = tokens[index].text;
    match text.as_bytes().first() {
        Some(b'C' | b'c' | b'P' | b'p') => {
            ascii_starts_with(text, &["CM", "PV"]) && text.contains('集')
        }
        Some(b'E' | b'e' | b'O' | b'o') => {
            index > 0 && tokens[index - 1].text.starts_with("ノンテロ")
                || index >= 2
                    && tokens[index - 2].text.eq_ignore_ascii_case("non")
                    && tokens[index - 1].text.eq_ignore_ascii_case("credit")
        }
        Some(byte) if !byte.is_ascii() => text.starts_with("ノンテロ"),
        _ => false,
    }
}

fn extra_marker(
    name: &str,
    tokens: &[Token<'_>],
    technical_boundary: Option<usize>,
    expected: Option<ExpectedKind>,
) -> Option<(usize, usize, ExtraInfo)> {
    let end = technical_boundary.map_or(tokens.len(), |boundary| {
        let source_width = if tokens[boundary].text.eq_ignore_ascii_case("bd") {
            1
        } else if tokens[boundary].text.eq_ignore_ascii_case("blu")
            && tokens
                .get(boundary + 1)
                .is_some_and(|token| token.text.eq_ignore_ascii_case("ray"))
        {
            2
        } else {
            0
        };
        if source_width > 0 {
            tokens
                .iter()
                .enumerate()
                .skip(boundary + source_width)
                .find_map(|(index, token)| is_technical(token.text).then_some(index))
                .unwrap_or(tokens.len())
        } else {
            boundary
        }
    });
    for (index, token) in tokens.iter().enumerate() {
        if !token.bracketed {
            if ((index < end && expected == Some(ExpectedKind::Series))
                || strong_extra_label(tokens, index))
                && let Some((category, mut group_start, number)) = unbracketed_extra(tokens, index)
            {
                if index > 0 && tokens[index - 1].text.eq_ignore_ascii_case("bd") {
                    group_start = tokens[index - 1].start;
                } else if index >= 2
                    && tokens[index - 2].text.eq_ignore_ascii_case("blu")
                    && tokens[index - 1].text.eq_ignore_ascii_case("ray")
                {
                    group_start = tokens[index - 2].start;
                }
                return Some((index, group_start, ExtraInfo { category, number }));
            }
            continue;
        }
        let text = token.text;
        let starts = |prefix: &str| ascii_starts_with(text, &[prefix]);
        let eq = |word: &str| text.eq_ignore_ascii_case(word);
        let (prefix, category) = if starts("NCOP") || eq("OP") {
            (
                if starts("NCOP") { "NCOP" } else { "OP" },
                ExtraCategory::Opening,
            )
        } else if starts("NCED") || eq("ED") {
            (
                if starts("NCED") { "NCED" } else { "ED" },
                ExtraCategory::Ending,
            )
        } else if starts("TVSP") || eq("SP") {
            (
                if starts("TVSP") { "TVSP" } else { "SP" },
                ExtraCategory::TelevisionSpecial,
            )
        } else if starts("PV") {
            ("PV", ExtraCategory::PromotionalVideo)
        } else if starts("CM") {
            ("CM", ExtraCategory::Commercial)
        } else if starts("MENU") {
            ("MENU", ExtraCategory::Menu)
        } else if starts("EVENT") {
            ("EVENT", ExtraCategory::Event)
        } else if starts("CHRONICLE") {
            ("CHRONICLE", ExtraCategory::Chronicle)
        } else if starts("LECTURE") {
            ("LECTURE", ExtraCategory::Lecture)
        } else if starts("TALKING") || starts("COMMENT") {
            (
                if starts("TALKING") {
                    "TALKING"
                } else {
                    "COMMENT"
                },
                ExtraCategory::Commentary,
            )
        } else if starts("PRELUDE") {
            ("PRELUDE", ExtraCategory::Prelude)
        } else if starts("TRAILER") {
            ("TRAILER", ExtraCategory::Trailer)
        } else if starts("PREVIEW") {
            ("PREVIEW", ExtraCategory::Preview)
        } else if starts("MAKING") {
            ("MAKING", ExtraCategory::MakingOf)
        } else if starts("MUSIC") {
            ("MUSIC", ExtraCategory::Music)
        } else if starts("LOGO") || starts("PRODUCER") {
            (
                if starts("PRODUCER") {
                    "PRODUCER"
                } else {
                    "LOGO"
                },
                ExtraCategory::Logo,
            )
        } else if eq("MINI")
            && tokens
                .get(index + 1)
                .is_some_and(|t| t.bracketed && t.text.eq_ignore_ascii_case("Anime"))
        {
            ("MINI", ExtraCategory::MiniAnime)
        } else if starts("IV") {
            ("IV", ExtraCategory::Interview)
        } else if ascii_one_of(text, &["OVA", "OAD", "ONA"]) {
            (text, ExtraCategory::Other)
        } else {
            continue;
        };
        let mut digits = text.get(prefix.len()..).unwrap_or_default();
        if category == ExtraCategory::Commercial && digits.eq_ignore_ascii_case("s") {
            digits = "";
        }
        let number = if digits.is_empty() {
            None
        } else if digits.bytes().all(|b| b.is_ascii_digit()) {
            digits.parse().ok()
        } else {
            continue;
        };
        let mut group_start = token_group_start(name, token);
        if category == ExtraCategory::Menu
            && index >= 2
            && ascii_one_of(tokens[index - 2].text, &["vol", "volume"])
            && tokens[index - 1]
                .text
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            group_start = token_group_start(name, &tokens[index - 2]);
        }
        return Some((index, group_start, ExtraInfo { category, number }));
    }
    None
}

fn title_before_offset(name: &str, end: usize) -> Option<String> {
    clean_title(name.get(..end)?)
}

fn token_group_start(name: &str, token: &Token<'_>) -> usize {
    if !token.bracketed {
        return token.start;
    }
    let mut closed = 0usize;
    let mut start = token.start;
    for (index, byte) in name.as_bytes()[..token.start].iter().enumerate().rev() {
        match byte {
            b']' | b')' | b'}' => closed += 1,
            b'[' | b'(' | b'{' if closed > 0 => closed -= 1,
            b'[' | b'(' | b'{' => start = index,
            _ => {}
        }
    }
    start
}

fn spaced_dash_offset(text: &str) -> Option<usize> {
    let content_start = if text.starts_with('[') {
        text.find(']').map_or(0, |end| end + 1)
    } else {
        0
    };
    text.get(content_start..)?
        .char_indices()
        .find_map(|(offset, ch)| {
            if !matches!(ch, '-' | '‐' | '‑' | '‒' | '–' | '—') {
                return None;
            }
            let absolute = content_start + offset;
            let previous = text.get(..absolute)?.chars().next_back()?;
            let next = text.get(absolute + ch.len_utf8()..)?.chars().next()?;
            (previous.is_whitespace() && next.is_whitespace())
                .then(|| text[..absolute].trim_end().len())
        })
}

fn title_before(name: &str, tokens: &[Token<'_>], token_index: usize) -> Option<String> {
    let end = tokens
        .get(token_index)
        .map_or(name.len(), |t| token_group_start(name, t));
    clean_title(&name[..end])
}

fn separate_title_annotations(name: &str, candidate: &mut Candidate) {
    if !matches!(
        candidate.media.kind,
        MediaKind::Movie | MediaKind::Episode | MediaKind::SeasonPack
    ) {
        return;
    }
    let Some(span) = candidate.title_span else {
        return;
    };
    let Some(raw) = name.get(span.start..span.end) else {
        return;
    };
    let Some(open) = raw.find('(') else {
        return;
    };
    let Some(title) = clean_title(&raw[..open]) else {
        return;
    };
    let mut annotations = Vec::new();
    let mut cursor = open;
    while cursor < raw.len() {
        let remaining = &raw[cursor..];
        if !remaining.starts_with('(') {
            return;
        }
        let mut depth = 0usize;
        let mut close = None;
        for (offset, byte) in remaining.bytes().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            return;
        };
        let value = &remaining[1..close];
        let first_word = value
            .split(|ch: char| !ch.is_alphanumeric())
            .find(|word| !word.is_empty());
        if !value.chars().any(char::is_alphabetic)
            || first_word.is_some_and(|word| {
                ascii_one_of(
                    word,
                    &[
                        "part", "chapter", "episode", "season", "volume", "vol", "disc",
                    ],
                )
            })
            || value.bytes().all(|byte| {
                byte.is_ascii_whitespace()
                    || matches!(
                        byte.to_ascii_uppercase(),
                        b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M'
                    )
            })
        {
            return;
        }
        annotations.push(Span {
            start: span.start + cursor + 1,
            end: span.start + cursor + close,
        });
        cursor += close + 1;
        while raw
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'.' | b'_' | b'-'))
        {
            cursor += 1;
        }
    }
    candidate.media.title = Some(title);
    for span in annotations {
        candidate
            .media
            .title_annotations
            .push(clean_title(&name[span.start..span.end]).unwrap_or_default());
    }
}

fn clean_title(value: &str) -> Option<String> {
    let mut text = value.trim();
    // A leading bracketed prefix precedes title text. A sole bracketed block
    // supplies the title itself.
    while text.starts_with('[')
        && let Some(close) = text.find(']')
        && !text[close + 1..].trim().is_empty()
    {
        text = text[close + 1..].trim_start();
    }
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' | '_' | '\t' | '\n' | '\r' => pending_space = true,
            '-' | '‐' | '‑' | '–' | '—' => {
                let spaced = pending_space || chars.peek().is_some_and(|next| next.is_whitespace());
                if spaced && !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push('-');
                pending_space = spaced;
            }
            '[' | ']' | '(' | ')' | '{' | '}' => pending_space = true,
            c if c.is_whitespace() => pending_space = true,
            c => {
                if pending_space && !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                pending_space = false;
                out.push(c);
            }
        }
    }
    let trim = |c: char| c.is_whitespace() || c == '-' || c == '&';
    let start = out.len() - out.trim_start_matches(trim).len();
    if start > 0 {
        out.drain(..start);
    }
    out.truncate(out.trim_end_matches(trim).len());
    (!out.is_empty()).then_some(out)
}

fn episode_title(
    name: &str,
    tokens: &[Token<'_>],
    anchor: usize,
    tech: Option<usize>,
    year: Option<usize>,
) -> Option<String> {
    let start = tokens.get(anchor)?.end;
    let boundary = match (tech.filter(|i| *i > anchor), year.filter(|i| *i > anchor)) {
        (Some(tech), Some(year)) => Some(tech.min(year)),
        (Some(tech), None) | (None, Some(tech)) => Some(tech),
        (None, None) => None,
    };
    let end = boundary
        .and_then(|i| tokens.get(i))
        .map_or(name.len(), |t| token_group_start(name, t));
    clean_title(name.get(start..end)?)
}

fn filename_stem(input: &str) -> (&str, usize) {
    let start = input.rfind(['/', '\\']).map_or(0, |i| i + 1);
    let name = &input[start..];
    let Some(dot) = name.rfind('.') else {
        return (name, name.len());
    };
    let extension = &name[dot + 1..];
    if [
        "mkv", "mp4", "avi", "m4v", "mov", "ts", "webm", "wmv", "m2ts", "mpg", "mpeg", "ogv",
        "flv", "rmvb", "rm", "srt", "ass", "ssa", "vtt", "sub", "sup", "idx",
    ]
    .iter()
    .any(|known| extension.eq_ignore_ascii_case(known))
    {
        (&name[..dot], dot)
    } else {
        (name, name.len())
    }
}

fn expected_score(actual: Option<ExpectedKind>, candidate: ExpectedKind) -> i32 {
    match actual {
        Some(kind) if kind == candidate => 12,
        Some(_) => -12,
        None => 0,
    }
}

fn is_context_directory(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "movies"
            | "movie"
            | "films"
            | "series"
            | "tv"
            | "shows"
            | "season"
            | "specials"
            | "extras"
            | "bonus"
            | "sps"
            | "sp"
    ) || normalized.starts_with("season ")
}

fn empty_candidate(kind: MediaKind, score: i32) -> Candidate {
    Candidate {
        media: ParsedMedia {
            kind,
            title: None,
            title_annotations: Vec::new(),
            year: None,
            season: None,
            episodes: Vec::new(),
            absolute_episodes: Vec::new(),
            episode_numbering: None,
            episode_title: None,
            extra: None,
            resolution: None,
            source: None,
            video_codec: None,
            dynamic_range: Vec::new(),
            audio_codec: Vec::new(),
            audio_channels: None,
            release_group: None,
        },
        score,
        title_span: None,
    }
}

#[cfg(test)]
mod tests;
