#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::lexer::{Separator, Token, tokenize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
/// An optional movie or series hint supplied by the caller.
pub enum ExpectedKind {
    Movie,
    Series,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
/// The structural interpretation of a media name.
pub enum MediaKind {
    Movie,
    Episode,
    SeasonPack,
    Extra,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
/// The numbering system used by a parsed episode.
pub enum EpisodeNumbering {
    SeasonEpisode,
    Absolute,
    SeasonUnspecified,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraCategory {
    Opening,
    Ending,
    PromotionalVideo,
    Commercial,
    Menu,
    TelevisionSpecial,
    Interview,
    Commentary,
    Chronicle,
    Lecture,
    Event,
    Trailer,
    Preview,
    MakingOf,
    Music,
    Logo,
    MiniAnime,
    Prelude,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExtraInfo {
    pub category: ExtraCategory,
    pub number: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// Identification and technical metadata parsed from one media name.
pub struct ParsedMedia {
    pub kind: MediaKind,
    pub title: Option<String>,
    /// Parenthesized names and qualifiers accompanying the primary title.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title_annotations: Vec<String>,
    pub year: Option<u16>,
    pub season: Option<u16>,
    pub episodes: Vec<u16>,
    pub absolute_episodes: Vec<u16>,
    pub episode_numbering: Option<EpisodeNumbering>,
    pub episode_title: Option<String>,
    pub extra: Option<ExtraInfo>,
    pub resolution: Option<String>,
    pub source: Option<String>,
    pub video_codec: Option<String>,
    pub dynamic_range: Vec<String>,
    pub audio_codec: Vec<String>,
    pub audio_channels: Option<String>,
    pub release_group: Option<String>,
}

#[derive(Clone, Debug)]
struct Candidate {
    media: ParsedMedia,
    score: i32,
    title_span: Option<Span>,
}

impl std::ops::Deref for Candidate {
    type Target = ParsedMedia;
    fn deref(&self) -> &Self::Target {
        &self.media
    }
}

impl std::ops::DerefMut for Candidate {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.media
    }
}

#[derive(Clone, Debug, Default)]
/// Hints and ancestors belonging to the input's own directory path.
pub struct ParseContext<'a> {
    pub expected_kind: Option<ExpectedKind>,
    /// Ancestor directory names, ordered nearest first.
    pub ancestors: Vec<&'a str>,
}

#[derive(Clone, Debug)]
struct EpisodeMarker {
    token_index: usize,
    last_token_index: usize,
    season: Option<u16>,
    episodes: Vec<u16>,
    is_pack: bool,
    numbering: EpisodeNumbering,
    rule: &'static str,
}

impl EpisodeMarker {
    fn new(
        token_range: (usize, usize),
        season: Option<u16>,
        episodes: Vec<u16>,
        is_pack: bool,
        numbering: EpisodeNumbering,
        rule: &'static str,
    ) -> Self {
        Self {
            token_index: token_range.0,
            last_token_index: token_range.1,
            season,
            episodes,
            is_pack,
            numbering,
            rule,
        }
    }
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
            if candidate.kind == MediaKind::Episode
                && candidate.season.is_none()
                && candidate.episode_numbering == Some(EpisodeNumbering::SeasonUnspecified)
            {
                candidate.season = Some(season);
                candidate.episode_numbering = Some(EpisodeNumbering::SeasonEpisode);
                candidate.score += 8;
            }
        }
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    }
    if !candidates.iter().any(|candidate| candidate.title.is_some()) {
        for ancestor in &context.ancestors {
            if is_context_directory(ancestor) {
                continue;
            }
            let Some(parent) = parse_internal(ancestor, context.expected_kind, None)
                .into_iter()
                .find(|candidate| {
                    candidate.title.is_some() && candidate.kind != MediaKind::Unknown
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
            for candidate in &mut candidates {
                candidate.title = Some(title.clone());
                candidate.title_annotations.clone_from(&title_annotations);
            }
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

    if let Some(marker) = markers.iter().find(|m| !m.is_pack) {
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
        candidate.title = title;
        candidate.season = marker.season;
        candidate.episode_numbering = Some(marker.numbering);
        match marker.numbering {
            EpisodeNumbering::SeasonEpisode | EpisodeNumbering::SeasonUnspecified => {
                candidate.episodes = marker.episodes.clone();
            }
            EpisodeNumbering::Absolute => candidate.absolute_episodes = marker.episodes.clone(),
        }
        candidate.release_group = group.clone();
        if let Some(year_index) = title_year {
            candidate.year = tokens[year_index].text.parse::<u16>().ok();
        }

        if candidate.title.is_some() {
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
            candidate.year = tokens[index].text.parse::<u16>().ok();
        }
        if let Some(value) = episode_title(
            name,
            &tokens,
            marker.last_token_index,
            tech_index,
            year_index,
        ) {
            candidate.episode_title = Some(value.clone());
        }
        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate, name, &tokens, tech_index, group.as_deref());

        candidates.push(candidate);
    }

    if let Some(marker) = markers.iter().find(|m| m.is_pack) {
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
        candidate.title = title_before(name, &tokens, boundary);
        if candidate.title.is_some() {
            candidate.title_span = Some(Span {
                start: 0,
                end: tokens
                    .get(boundary)
                    .map_or(name.len(), |token| token_group_start(name, token)),
            });
        }
        if let Some(index) = year_index {
            candidate.year = tokens[index].text.parse().ok();
        }
        candidate.season = marker.season;
        candidate.release_group = group.clone();

        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate, name, &tokens, tech_index, group.as_deref());

        candidates.push(candidate);
    }

    if markers.is_empty() && candidates.is_empty() && extra.is_none() {
        let movie_marker = explicit_movie_marker(&tokens);
        let content_start = leading_content_token(&tokens);
        let numeric_title = tech_index.unwrap_or(tokens.len()) == content_start + 1;
        let year_indexes: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, token)| {
                (is_release_year(token.text) && !(numeric_title && i == content_start)).then_some(i)
            })
            .collect();
        let chosen_years = if year_indexes.is_empty() {
            vec![None]
        } else {
            year_indexes.iter().copied().map(Some).collect()
        };
        for year_index in chosen_years {
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
            candidate.title = title_before(name, &tokens, boundary);
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
                candidate.title = name.get(next.start..end).and_then(clean_title);
                title_start = next.start;
                title_end = end;
            }
            candidate.release_group = group.clone();
            if let Some(index) = year_index {
                let year = tokens[index].text.parse::<u16>().ok();
                candidate.year = year;
            }
            if candidate.title.is_some() {
                candidate.title_span = Some(Span {
                    start: title_start,
                    end: title_end,
                });
            }
            candidate.score += expected_score(expected, ExpectedKind::Movie);
            if let Some(index) = year_index {
                if candidate.title.is_some() {
                    candidate.score += 10;
                } else {
                    candidate.score -= 24;
                }
                if let Some(tech) = tech_index {
                    candidate.score +=
                        12i32.saturating_sub((tech.saturating_sub(index) as i32) * 2);
                }
            }
            fill_technical(&mut candidate, name, &tokens, tech_index, group.as_deref());

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
        candidate.title = title_before_offset(name, title_end);
        if let Some(index) = year_index {
            candidate.year = tokens[index].text.parse().ok();
        }
        candidate.extra = Some(info);
        candidate.release_group = group.clone();

        if candidate.title.is_some() {
            candidate.title_span = Some(Span {
                start: 0,
                end: title_end,
            });
        }
        candidate.score += expected_score(expected, ExpectedKind::Series);
        fill_technical(&mut candidate, name, &tokens, tech_index, group.as_deref());

        candidates.push(candidate);
    }

    if candidates.is_empty() {
        let mut candidate = empty_candidate(MediaKind::Unknown, 0);
        candidate.title = title_before(name, &tokens, tech_index.unwrap_or(tokens.len()));
        candidate.release_group = group.clone();
        fill_technical(&mut candidate, name, &tokens, tech_index, group.as_deref());

        candidates.push(candidate);
    }

    for candidate in &mut candidates {
        separate_title_annotations(name, candidate);
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    candidates.dedup_by(|a, b| {
        a.kind == b.kind
            && a.title == b.title
            && a.year == b.year
            && a.season == b.season
            && a.episodes == b.episodes
    });
    candidates
}

fn episode_markers(
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
        && tokens
            .first()
            .is_some_and(|token| !is_technical(token.text));
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
                "ordinal_season_episode_marker",
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
                "season_word_marker",
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
                "season_x_episode_marker",
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
                "sxxeyy_marker",
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
                    "compact_season_episode_marker",
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
                "season_x_episode_marker",
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
                "compact_episode_from_season_context",
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
                "episode_word_marker",
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
                    "episode_only_marker",
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
                    "season_then_e_episode",
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
                    "season_then_dash_episode",
                ));
            } else {
                markers.push(EpisodeMarker::new(
                    (i, i),
                    Some(season),
                    Vec::new(),
                    true,
                    EpisodeNumbering::SeasonUnspecified,
                    "season_pack_marker",
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
                if token.bracketed {
                    "bracketed_absolute_episode"
                } else {
                    "anime_release_absolute_episode"
                },
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
                if season.is_some() {
                    "series_dash_episode_from_season_context"
                } else {
                    "series_dash_episode_number"
                },
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
                && marker.rule == "episode_only_marker";
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

fn episode_number(text: &str) -> Option<u16> {
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

fn roman_season(text: &str) -> Option<u16> {
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

fn separator_gap_is_dash(name: &str, left: &Token<'_>, right: &Token<'_>) -> bool {
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

fn parse_s_token(text: &str) -> Option<u16> {
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

fn parse_sxe(text: &str) -> Option<(u16, Vec<u16>)> {
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

fn parse_x_format(text: &str) -> Option<(u16, u16)> {
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

fn season_directory_number(name: &str) -> Option<u16> {
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
        candidate.kind,
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
    candidate.title = Some(title);
    for span in annotations {
        candidate
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

fn leading_content_token(tokens: &[Token<'_>]) -> usize {
    tokens
        .iter()
        .position(|token| !token.bracketed)
        .unwrap_or(0)
}

fn first_technical_token(tokens: &[Token<'_>]) -> Option<usize> {
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

fn is_technical(token: &str) -> bool {
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

fn is_release_year(token: &str) -> bool {
    token.len() == 4
        && token.bytes().all(|b| b.is_ascii_digit())
        && token
            .parse::<u16>()
            .is_ok_and(|year| (1870..=2035).contains(&year))
}

fn fill_technical(
    candidate: &mut Candidate,
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
            candidate.resolution.get_or_insert(resolution);
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

fn ascii_one_of(value: &str, choices: &[&str]) -> bool {
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

fn ascii_starts_with(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        value
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    })
}

fn dimension(text: &str) -> Option<(u16, u16)> {
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

fn release_group(name: &str, tokens: &[Token<'_>], stem_end: usize) -> Option<String> {
    let stem = name.get(..stem_end)?;
    let dash = stem.rfind('-')?;
    let suffix = stem[dash + 1..].trim();
    if suffix.is_empty() || suffix.len() > 32 || suffix.contains([' ', '.', '_']) {
        return None;
    }
    if !suffix.chars().any(char::is_alphanumeric) {
        return None;
    }
    // The final token represents the complete release-group suffix.
    let final_token = tokens.last()?;
    if final_token.text != suffix {
        return None;
    }
    // WEB-DL forms one source marker across its hyphen.
    let suffix_index = tokens.len().checked_sub(1)?;
    if suffix.eq_ignore_ascii_case("DL")
        && suffix_index > 0
        && tokens[suffix_index - 1].text.eq_ignore_ascii_case("WEB")
    {
        return None;
    }
    Some(suffix.to_owned())
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
mod tests {
    use super::*;

    #[test]
    fn every_input_returns_one_metadata_record() {
        for input in ["", "   ", "未知标题.mkv", "[Unclosed", "???", "folder/"] {
            let media = parse(input);
            let encoded = serde_json::to_value(&media).unwrap();
            assert!(encoded.is_object());
            assert_eq!(encoded["kind"], serde_json::to_value(media.kind).unwrap());
            assert_eq!(
                serde_json::from_value::<ParsedMedia>(encoded).unwrap(),
                media
            );
        }
    }

    fn first(input: &str) -> ParsedMedia {
        parse(input)
    }

    #[test]
    fn parses_scene_episode_metadata() {
        let result = first(
            "Slow.Horses.S04E01.Identity.Theft.2160p.ATVP.WEB-DL.DDP5.1.DV.HDR.H.265-NTb.mkv",
        );
        assert_eq!(result.title.as_deref(), Some("Slow Horses"));
        assert_eq!(result.season, Some(4));
        assert_eq!(result.episodes, vec![1]);
        assert_eq!(result.episode_title.as_deref(), Some("Identity Theft"));
        assert_eq!(result.resolution.as_deref(), Some("2160p"));
        assert_eq!(result.audio_codec, vec!["E-AC-3"]);
        assert_eq!(result.audio_channels.as_deref(), Some("5.1"));
    }

    #[test]
    fn attached_audio_codec_tokens_supply_channel_layouts() {
        let dolby = first("Example.S01E01.1080p.BluRay.DD5.1.x264-GROUP.mkv");
        assert_eq!(dolby.audio_codec, vec!["AC-3"]);
        assert_eq!(dolby.audio_channels.as_deref(), Some("5.1"));

        let dolby_plus = first("Example.S01E03.1080p.BluRay.DD+5.1.x264-GROUP.mkv");
        assert_eq!(dolby_plus.audio_codec, vec!["E-AC-3"]);
        assert_eq!(dolby_plus.audio_channels.as_deref(), Some("5.1"));

        let aac = first("Example.S01E02.1080p.WEB-DL.AAC2.0.H.264-GROUP.mkv");
        assert_eq!(aac.audio_codec, vec!["AAC"]);
        assert_eq!(aac.audio_channels.as_deref(), Some("2.0"));
    }

    #[test]
    fn normalizes_spaces_around_title_dashes() {
        assert_eq!(
            clean_title("Node.js - The Documentary - An origin story").as_deref(),
            Some("Node js - The Documentary - An origin story")
        );
    }

    #[test]
    fn movie_year_anchors_preserve_numbered_titles() {
        let result = first("Dead.Mail.2024.1080p.WEBRip.AVC.AAC.404.mp4");
        assert_eq!(result.kind, MediaKind::Movie);
        assert_eq!(result.title.as_deref(), Some("Dead Mail"));
        assert_eq!(result.year, Some(2024));
        assert!(result.episodes.is_empty());
    }

    #[test]
    fn movie_installments_share_year_and_title_boundaries() {
        for (input, title, year) in [
            (
                "Scary Movie 1 (2000) - 1080p FR EN x264 ac3 mHDgz",
                "Scary Movie 1",
                Some(2000),
            ),
            (
                "The.Lego.Movie.2.The.Second.Part.2019.MULTi.1080p.HDLight.x264.AC3-TOXIC.mkv",
                "The Lego Movie 2 The Second Part",
                Some(2019),
            ),
            (
                "The.Angry.Birds.Movie.2.2019.MULTi.TRUEFRENCH.1080p.HDLight.x264.AC3-TOXIC.mkv",
                "The Angry Birds Movie 2",
                Some(2019),
            ),
            (
                "Example.Movie.2.[1080p].MULTi.2019.BluRay.mkv",
                "Example Movie 2",
                Some(2019),
            ),
            (
                "Example.Movie.2.[MKV.1920x800].mkv",
                "Example Movie 2",
                None,
            ),
        ] {
            let result = first(input);
            assert_eq!(result.kind, MediaKind::Movie, "{input}");
            assert_eq!(result.title.as_deref(), Some(title), "{input}");
            assert_eq!(result.year, year, "{input}");
            assert!(result.episodes.is_empty(), "{input}");
        }
        let report = parse("Example.Movie.2.1994.2021.1080p.mkv");
        assert_eq!(report.title.as_deref(), Some("Example Movie 2 1994"));
        assert_eq!(report.year, Some(2021));

        let report = parse_with_context(
            "Example Movie 2 1080p.mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Series),
                ancestors: vec![],
            },
        );
        assert_eq!(report.kind, MediaKind::Movie);
    }

    #[test]
    fn parenthesized_title_annotations_preserve_names() {
        for (input, title, annotations) in [
            (
                "Two.Night.Stand.(Aventure.d'un.soir).2014.1080p.mkv",
                "Two Night Stand",
                vec!["Aventure d'un soir"],
            ),
            (
                "Example (Another Name) (Qualifiers (Restored)).2020.1080p.mkv",
                "Example",
                vec!["Another Name", "Qualifiers Restored"],
            ),
            (
                "[Group] Example (Otro Nombre).2020.BluRay.mkv",
                "Example",
                vec!["Otro Nombre"],
            ),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some(title), "{input}");
            assert_eq!(result.title_annotations, annotations, "{input}");
        }
        for (input, title) in [
            ("Example (Part 2).2020.1080p.mkv", "Example Part 2"),
            ("Example (II).2020.1080p.mkv", "Example II"),
            ("(500) Days of Summer.2009.1080p.mkv", "500 Days of Summer"),
            (
                "Example (Annotation) Continued.2020.1080p.mkv",
                "Example Annotation Continued",
            ),
            ("Example (Unclosed.2020.1080p.mkv", "Example"),
            (
                "Mission - Impossible - The Final Reckoning (2025).1080p.mkv",
                "Mission - Impossible - The Final Reckoning",
            ),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some(title), "{input}");
            assert!(result.title_annotations.is_empty(), "{input}");
        }
    }

    #[test]
    fn title_annotations_round_trip_and_follow_ancestors() {
        let ancestor = "catalog/Example (Another Name).2020.1080p";
        let report = parse_with_context(
            "S01E02.mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Series),
                ancestors: vec![ancestor],
            },
        );
        let candidate = &report;
        assert_eq!(candidate.title.as_deref(), Some("Example"));
        assert_eq!(candidate.title_annotations, ["Another Name"]);

        let encoded = serde_json::to_string(&report).unwrap();
        assert_eq!(
            serde_json::from_str::<ParsedMedia>(&encoded).unwrap(),
            report
        );

        let plain = parse("Example.2020.mkv");
        let encoded = serde_json::to_string(&plain).unwrap();
        assert!(!encoded.contains("title_annotations"));
        assert_eq!(
            serde_json::from_str::<ParsedMedia>(&encoded).unwrap(),
            plain
        );
    }

    #[test]
    fn source_and_codec_aliases_share_normalized_technical_fields() {
        for (marker, source) in [
            ("Blu-Ray", "Blu-ray"),
            ("BDMux", "Blu-ray"),
            ("DVDScr", "DVD"),
            ("SDTV", "SDTV"),
            ("HDTVRip", "HDTV"),
            ("HDcam", "CAM"),
            ("HDRip", "HD Rip"),
            ("VHS", "VHS"),
        ] {
            let input = format!("Example.2020.{marker}.1080p.VP9.Opus.mkv");
            let candidate = first(&input);
            assert_eq!(candidate.title.as_deref(), Some("Example"), "{input}");
            assert_eq!(candidate.source.as_deref(), Some(source), "{input}");
            assert_eq!(candidate.video_codec.as_deref(), Some("VP9"));
            assert_eq!(candidate.audio_codec, ["Opus"]);
        }
        assert_eq!(first("Example.2020.DivX.AC3D.mkv").audio_codec, ["AC-3"]);
        let movie = first("VHS.2020.1080p.WEB-DL.mkv");
        assert_eq!(movie.title.as_deref(), Some("VHS"));
        assert_eq!(movie.source.as_deref(), Some("WEB-DL"));
        let movie = first("[BD] Example.2020.1080p.mkv");
        assert_eq!(movie.source.as_deref(), Some("Blu-ray"));
    }

    #[test]
    fn composite_technical_tokens_produce_normalized_values() {
        for (marker, expected) in [("H.264", "H.264"), ("h.265", "H.265"), ("H_265", "H.265")] {
            let input = format!("Example.2020.1080p.{marker}.mkv");
            let candidate = first(&input);
            assert_eq!(candidate.video_codec.as_deref(), Some(expected));

            assert_eq!(
                first(&format!("Example.{marker}.mkv")).title.as_deref(),
                Some("Example")
            );
        }
        let input = "Example.2020.BR1080P.HDR10+.mkv";
        let candidate = first(input);
        assert_eq!(candidate.source.as_deref(), Some("Blu-ray"));
        assert_eq!(candidate.resolution.as_deref(), Some("1080p"));
        assert_eq!(candidate.dynamic_range, ["HDR10+"]);

        assert_eq!(
            first("Example.2020.HDR10Plus.mkv").dynamic_range,
            ["HDR10+"]
        );
    }

    #[test]
    fn release_phrases_bound_titles_across_year_placements() {
        for marker in [
            "HD Light",
            "HDrip",
            "BR1080P",
            "Blu-Ray",
            "BDMux",
            "Director's Cut",
            "Director s Cut",
            "Theatrical Edition",
            "Ultimate Collector's Edition",
            "UNCUT",
            "LIMITED",
        ] {
            let input = format!("Example {marker} MULTI 2020 1080p.mkv");
            let result = first(&input);
            assert_eq!(result.title.as_deref(), Some("Example"), "{marker}");
            assert_eq!(result.year, Some(2020), "{marker}");
        }
        for input in [
            "Example.3D.HOU.1080p.mkv",
            "Example.3D.2020.1080p.BluRay.Half-SBS.mkv",
            "[Group] [BD][1080p] Example.2020.mkv",
        ] {
            assert_eq!(first(input).title.as_deref(), Some("Example"), "{input}");
        }
        for (input, title) in [
            ("Jackass 3D 720p.mkv", "Jackass 3D"),
            (
                "The Complete Unknown.2020.1080p.mkv",
                "The Complete Unknown",
            ),
            ("Limited Edition.2020.1080p.mkv", "Limited Edition"),
        ] {
            assert_eq!(first(input).title.as_deref(), Some(title), "{input}");
        }
    }

    #[test]
    fn leading_years_and_season_pack_years_identify_title_and_release_year() {
        for (input, year, season) in [
            ("2020 - Example.1080p.mkv", 2020, None),
            ("2020-Example.1080p.mkv", 2020, None),
            ("Example.2020.S03.1080p.mkv", 2020, Some(3)),
            ("Example-Season 1-[2020].x264.mkv", 2020, Some(1)),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some("Example"), "{input}");
            assert_eq!(result.year, Some(year), "{input}");
            assert_eq!(result.season, season, "{input}");
        }
    }

    #[test]
    fn sole_numeric_movie_titles_have_independent_release_years() {
        for title in ["1900", "1917", "1984", "2001", "2012", "2025"] {
            for prefix in ["", "[Group] "] {
                let input = format!("{prefix}{title}.1080p.BluRay.mkv");
                let candidate = first(&input);
                assert_eq!(candidate.title.as_deref(), Some(title), "{input}");
                assert_eq!(candidate.year, None, "{input}");
                let input = format!("{prefix}{title}.2026.1080p.BluRay.mkv");
                let candidate = first(&input);
                assert_eq!(candidate.title.as_deref(), Some(title), "{input}");
                assert_eq!(candidate.year, Some(2026), "{input}");
            }
        }
    }

    #[test]
    fn season_words_accept_canonical_roman_numbers() {
        for (roman, season) in [
            ("I", 1),
            ("IV", 4),
            ("vii", 7),
            ("IX", 9),
            ("XXX", 30),
            ("XLII", 42),
            ("LXXIX", 79),
            ("LXXXVIII", 88),
            ("XCIX", 99),
        ] {
            let pack = first(&format!("Example.Saison.{roman}.1080p.mkv"));
            assert_eq!(pack.title.as_deref(), Some("Example"));
            assert_eq!(pack.kind, MediaKind::SeasonPack);
            assert_eq!(pack.season, Some(season));
            let episode = first(&format!("Example.Season.{roman}.Episode.2.1080p.mkv"));
            assert_eq!(episode.season, Some(season));
            assert_eq!(episode.episodes, [2]);
        }
        for value in ["", "IIII", "VV", "IC", "IIV", "XXXX", "C", "Spring"] {
            assert_eq!(roman_season(value), None, "{value}");
        }
        let report = parse_with_context(
            "E03.mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Series),
                ancestors: vec!["Season IV", "Example.S04.1080p"],
            },
        );
        assert_eq!(report.season, Some(4));
    }

    #[test]
    fn explicit_episode_sequences_and_word_ranges_preserve_numbering() {
        for (input, season, episodes) in [
            ("Example.S02EP01.1080p.mkv", Some(2), vec![1]),
            ("Example.s2EP5.1080p.mkv", Some(2), vec![5]),
            ("Example.S03-x02.1080p.mkv", Some(3), vec![2]),
            ("Example.E01E02E04.1080p.mkv", None, vec![1, 2, 4]),
            ("Example.S01E01-S01E04.1080p.mkv", Some(1), vec![1, 2, 3, 4]),
            ("Example.S01E01.and.S01E03.1080p.mkv", Some(1), vec![1, 3]),
            (
                "Example.Season.4.Episodes.1-3.1080p.mkv",
                Some(4),
                vec![1, 2, 3],
            ),
            (
                "[Group] Example_-_16-20_[720p].mkv",
                None,
                vec![16, 17, 18, 19, 20],
            ),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some("Example"), "{input}");
            assert_eq!(result.season, season, "{input}");
            let actual = if season.is_none() && input.starts_with('[') {
                &result.absolute_episodes
            } else {
                &result.episodes
            };
            assert_eq!(actual, &episodes, "{input}");
        }
    }

    #[test]
    fn episode_titles_follow_structural_boundaries() {
        for input in [
            "Example.S03E16.Episode.Title.2020.720p.mkv",
            "Example.720p.S03E16.Episode.Title.mkv",
            "HD 720p: Example.S03E16.Episode.Title.mkv",
            "Example.S03E16.Episode.Title.[2020].720p.mkv",
        ] {
            let candidate = first(input);
            assert_eq!(candidate.episode_title.as_deref(), Some("Episode Title"));
        }
    }

    #[test]
    fn bracketed_titles_and_ratings_keep_movie_numbers_in_context() {
        for input in [
            "Example (2014) [R, 6.3] - 1080p.mkv",
            "Example (2010) (127 Hours) 1080p.mkv",
            "Example (Starter for 10).2006.1080p.mkv",
            "Example (Part 2).2006.1080p.mkv",
        ] {
            let result = first(input);
            assert_eq!(result.kind, MediaKind::Movie, "{input}");
            assert!(result.absolute_episodes.is_empty(), "{input}");
        }
        assert_eq!(
            first("[Group] Example [03v2] [720p].mkv").absolute_episodes,
            vec![3]
        );
    }

    #[test]
    fn identification_grammar_composes_across_titles_and_separators() {
        for (title, expected_title) in [
            ("Invented Chronicle", "Invented Chronicle"),
            ("新しい物語", "新しい物語"),
            ("Y Tu Mamá También", "Y Tu Mamá También"),
            ("Mini-hero Adventures", "Mini-hero Adventures"),
            ("The Darjeeling Limited", "The Darjeeling Limited"),
            ("Pal", "Pal"),
            ("VHS", "VHS"),
            ("1983", "1983"),
            (
                "War and Peace - The Final Chapter",
                "War and Peace - The Final Chapter",
            ),
        ] {
            for separator in [".", "_", " "] {
                for prefix in ["", "[ReleaseGroup] ", "[ReleaseGroup] [BD][1080p] "] {
                    let name = title.replace(' ', separator);
                    for marker in ["S02E03", "s02EP03", "S02-x03", "Season 2 Episode 03"] {
                        let input =
                            format!("{prefix}{name}{separator}{marker}{separator}1080p.mkv");
                        let result = first(&input);
                        assert_eq!(result.kind, MediaKind::Episode, "{input}");
                        assert_eq!(result.title.as_deref(), Some(expected_title), "{input}");
                        assert_eq!(result.season, Some(2), "{input}");
                        assert_eq!(result.episodes, vec![3], "{input}");
                    }
                    let input = format!("{prefix}{name}{separator}2020{separator}1080p.mkv");
                    let result = first(&input);
                    assert_eq!(result.kind, MediaKind::Movie, "{input}");
                    assert_eq!(result.title.as_deref(), Some(expected_title), "{input}");
                    assert_eq!(result.year, Some(2020), "{input}");
                }
            }
        }
        for (ordinal, season) in [
            ("1st", 1),
            ("2nd", 2),
            ("3rd", 3),
            ("4th", 4),
            ("11th", 11),
            ("12th", 12),
            ("21st", 21),
        ] {
            let input = format!("[Group][Example][{ordinal} - 03][1080p].mkv");
            let result = first(&input);
            assert_eq!(result.title.as_deref(), Some("Example"));
            assert_eq!(result.season, Some(season));
            assert_eq!(result.episodes, vec![3]);
        }
        for prefix in ["[h265 - HEVC]", "[BD][1080p]"] {
            let result = first(&format!("{prefix} Example 2 1080p.mkv"));
            assert_eq!(result.kind, MediaKind::Movie);
            assert_eq!(result.title.as_deref(), Some("Example 2"));
        }
        let numeric_title = first("1983 - Season 1 - 1080p.mkv");
        assert_eq!(numeric_title.title.as_deref(), Some("1983"));
        assert_eq!(numeric_title.year, None);
    }

    #[test]
    fn movie_titles_end_at_the_first_release_anchor() {
        for (input, title, year) in [
            (
                "Example.[1080p].MULTi.2013.BluRay.x264-GROUP.mkv",
                "Example",
                2013,
            ),
            (
                "Example.UNRATED.2010.MULTi.1080p.BluRay.mkv",
                "Example",
                2010,
            ),
            (
                "The.French.Connection.1971.1080p.BluRay.mkv",
                "The French Connection",
                1971,
            ),
            (
                "The.English.Patient.1996.1080p.BluRay.mkv",
                "The English Patient",
                1996,
            ),
            (
                "The.Girl.in.the.Spiders.Web.2019.1080p.WEB-DL.mkv",
                "The Girl in the Spiders Web",
                2019,
            ),
            ("[Group] Example (2020) [1080p].mkv", "Example", 2020),
            ("Extended.Family.2020.1080p.mkv", "Extended Family", 2020),
            ("Dual.2022.1080p.BluRay.mkv", "Dual", 2022),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some(title), "{input}");
            assert_eq!(result.year, Some(year), "{input}");
            assert_eq!(result.kind, MediaKind::Movie, "{input}");
        }
    }

    #[test]
    fn language_tags_form_contextual_release_boundaries() {
        for input in [
            "Example.FRENCH.DVDRip.XviD.mkv",
            "Example.German.720p.BluRay.mkv",
            "Example.TRUEFRENCH.BluRay.1080p.mkv",
            "Example.MULTi.x265.AC3.mkv",
            "Example.VOSTFR.HDLight.x264.mkv",
            "Example.VF2.mHD.x264.mkv",
            "Example [FR-EN] (2020).mkv",
            "Example [EN-JA] (2020).mkv",
            "Example [ENG+JPN] (2020).mkv",
        ] {
            assert_eq!(first(input).title.as_deref(), Some("Example"), "{input}");
        }
        assert_eq!(
            first("French.Exit.mkv").title.as_deref(),
            Some("French Exit")
        );
        assert_eq!(
            first("The.German.Doctor.mkv").title.as_deref(),
            Some("The German Doctor")
        );
        assert_eq!(
            first("Example [It] (2020).mkv").title.as_deref(),
            Some("Example It")
        );
    }

    #[test]
    fn explicit_episode_anchors_expand_ranges_and_list_members() {
        for (input, expected, season) in [
            ("Example.S01E01+03.1080p.mkv", vec![1, 3], Some(1)),
            ("Example.S01E02-04.1080p.mkv", vec![2, 3, 4], Some(1)),
            ("Example.1x02-04.1080p.mkv", vec![2, 3, 4], Some(1)),
            ("Example.E02-04.1080p.mkv", vec![2, 3, 4], None),
            ("Example.E02&04.1080p.mkv", vec![2, 4], None),
        ] {
            let result = first(input);
            assert_eq!(result.kind, MediaKind::Episode, "{input}");
            assert_eq!(result.episodes, expected, "{input}");
            assert_eq!(result.season, season, "{input}");
        }
        assert_eq!(first("Example.S01E01.DD5.1.1080p.mkv").episodes, vec![1]);
    }

    #[test]
    fn episode_ranges_follow_numbering_block_boundaries() {
        for (input, episodes) in [
            ("Example - S16E03-04 - 313-314.mkv", vec![3, 4]),
            ("Example.S01E01-03+05-07.1080p.mkv", vec![1, 2, 3, 5, 6, 7]),
            ("Example.S01E01 - E03.1080p.mkv", vec![1, 2, 3]),
            ("Example.S01E01 - 123 - Episode Title.mkv", vec![1]),
        ] {
            assert_eq!(first(input).episodes, episodes, "{input}");
        }
    }

    #[test]
    fn numeric_season_words_identify_packs_and_episodes() {
        for word in ["Season", "Seasons", "Saison", "Saisons", "Temporada"] {
            let pack = first(&format!("Example - {word} 2 - 1080p.BluRay.mkv"));
            assert_eq!(pack.kind, MediaKind::SeasonPack, "{word}");
            assert_eq!(pack.title.as_deref(), Some("Example"), "{word}");
            assert_eq!(pack.season, Some(2), "{word}");
            let episode = first(&format!("Example {word} 2 Episode 03v2 1080p.mkv"));
            assert_eq!(episode.kind, MediaKind::Episode, "{word}");
            assert_eq!(episode.title.as_deref(), Some("Example"), "{word}");
            assert_eq!(episode.season, Some(2), "{word}");
            assert_eq!(episode.episodes, vec![3], "{word}");
        }
        assert_eq!(first("Example Season 2 E03-05.mkv").episodes, vec![3, 4, 5]);
        assert_eq!(
            first("The.Fifth.Season.2012.mkv").title.as_deref(),
            Some("The Fifth Season")
        );
    }

    #[test]
    fn calendar_seasons_preserve_the_explicit_season_number() {
        for marker in ["S2014E18", "S2014.E18", "Season.2014.Episode.18"] {
            let episode = first(&format!("Example.{marker}.1080p.mkv"));
            assert_eq!(episode.kind, MediaKind::Episode, "{marker}");
            assert_eq!(episode.title.as_deref(), Some("Example"), "{marker}");
            assert_eq!(episode.season, Some(2014), "{marker}");
            assert_eq!(episode.episodes, vec![18], "{marker}");
            assert_eq!(episode.year, None, "{marker}");
        }
        let separate_year = first("Example.2008.S2014E18.1080p.mkv");
        assert_eq!(separate_year.year, Some(2008));
        assert_eq!(separate_year.season, Some(2014));
        assert!(parse_sxe("S100E18").is_none());
        assert!(parse_sxe("S9999E18").is_none());
    }

    #[test]
    fn episode_revision_suffixes_preserve_episode_identity() {
        for (input, season, episodes, absolute) in [
            ("Example Episode 366v2 VOSTFR.mkv", None, vec![366], vec![]),
            ("Example.S01E03v2.1080p.mkv", Some(1), vec![3], vec![]),
            ("Example.E03V12.1080p.mkv", None, vec![3], vec![]),
            ("Example.1x03v2.1080p.mkv", Some(1), vec![3], vec![]),
            ("[Group] Example - 03v2 [720p].mkv", None, vec![], vec![3]),
            ("[Group] Example 03v2 [720p].mkv", None, vec![], vec![3]),
        ] {
            let result = first(input);
            assert_eq!(result.kind, MediaKind::Episode, "{input}");
            assert_eq!(result.title.as_deref(), Some("Example"), "{input}");
            assert_eq!(result.season, season, "{input}");
            assert_eq!(result.episodes, episodes, "{input}");
            assert_eq!(result.absolute_episodes, absolute, "{input}");
        }
        for invalid in ["03v", "03version2", "03v2extra", "1000v2"] {
            assert!(episode_number(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn title_text_preserves_accents_bilingual_names_and_bare_numbers() {
        for (input, title) in [
            ("Y Tu Mamá También (2001).mkv", "Y Tu Mamá También"),
            (
                "War and Peace - Guerre et Paix.1956.Multi.1080p.mkv",
                "War and Peace - Guerre et Paix",
            ),
            ("new.girl.117.hdtv-lol.mp4", "new girl 117"),
            ("Example.0307.PDTV.mkv", "Example 0307"),
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some(title), "{input}");
            assert!(result.episodes.is_empty(), "{input}");
        }
    }

    #[test]
    fn extra_labels_bound_titles_years_and_version_numbers() {
        for (label, category) in [
            ("ノンテロップED (Ver.3)", ExtraCategory::Ending),
            ("ノンテロップED (Ver.3)", ExtraCategory::Ending),
            ("Non-Credit ED 2", ExtraCategory::Ending),
            ("BD CM集", ExtraCategory::Commercial),
            ("Blu-ray CM集", ExtraCategory::Commercial),
        ] {
            let input = format!("Example 2014 - {label} [BD 1080p].mkv");
            let result = first(&input);
            assert_eq!(result.kind, MediaKind::Extra, "{label}");
            assert_eq!(result.title.as_deref(), Some("Example"), "{label}");
            assert_eq!(result.year, Some(2014), "{label}");
            assert_eq!(result.extra.as_ref().unwrap().category, category, "{label}");
            assert!(result.episodes.is_empty(), "{label}");
            assert!(result.absolute_episodes.is_empty(), "{label}");
        }
        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec!["Example.S01", "series"],
        };
        let report = parse_with_context("ノンテロップED (Ver.3) [BD 1080p].mkv", &context);
        assert_eq!(report.title.as_deref(), Some("Example"));
        assert_eq!(report.kind, MediaKind::Extra);
        assert!(report.extra.as_ref().unwrap().number.is_none());
    }

    #[test]
    fn anime_release_prefixes_support_absolute_episode_numbers() {
        for input in [
            "[Group] Example - 04 [720p].mkv",
            "[Group] Example 04 (720p 8bit AAC).mkv",
            "[Group] Example [04] {720p}.mkv",
            "[t.3.3.d] Example - 04 [720p].mkv",
        ] {
            let result = first(input);
            assert_eq!(result.title.as_deref(), Some("Example"), "{input}");
            assert_eq!(result.absolute_episodes, vec![4], "{input}");
            assert_eq!(result.kind, MediaKind::Episode, "{input}");
        }
        let movie = parse_with_context(
            "[Group] Example 04 [720p].mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Movie),
                ancestors: vec![],
            },
        );
        assert_eq!(movie.kind, MediaKind::Movie);
    }

    #[test]
    fn nested_release_blocks_supply_their_outermost_boundary() {
        let result = first("[Group] Example [notes (1080p)] (2020).mkv");
        assert_eq!(result.title.as_deref(), Some("Example"));
        assert_eq!(result.year, Some(2020));
    }

    #[test]
    fn movie_title_years_select_the_release_year() {
        let result = parse("2001.A.Space.Odyssey.1968.1080p.BluRay.x264-GROUP.mkv");

        assert_eq!(result.year, Some(1968));

        let movie_title_year = parse("1922.2017.1080p.WEBRip.DD+5.1.x264-playHD.mkv");
        assert_eq!(movie_title_year.title.as_deref(), Some("1922"));
        assert_eq!(movie_title_year.year, Some(2017));

        let installment_year = parse("Fear.Street.Part.1.1994.2021.1080p.NF.WEB-DL.mkv");
        assert_eq!(
            installment_year.title.as_deref(),
            Some("Fear Street Part 1 1994")
        );
        assert_eq!(installment_year.year, Some(2021));

        let numbered_installment = parse(
            "Fear.Street.Part.3.1666.2021.1080p.NF.WEB-DL.DDP5.1.Atmos.DV.HDR.H.265-CRFW.mkv",
        );
        assert_eq!(numbered_installment.kind, MediaKind::Movie);
        assert_eq!(
            numbered_installment.title.as_deref(),
            Some("Fear Street Part 3 1666")
        );
        assert_eq!(numbered_installment.year, Some(2021));
        assert!(numbered_installment.season.is_none());
        assert!(numbered_installment.episodes.is_empty());
    }

    #[test]
    fn parses_season_pack_and_bare_episode_using_local_parent_context() {
        let pack = first("Slow.Horses.S04.2160p.WEB-DL.mkv");
        assert_eq!(pack.kind, MediaKind::SeasonPack);
        assert_eq!(pack.season, Some(4));

        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec!["A.Miss.Mallard.Mystery.2000.1080p.WEBRip", "series"],
        };
        let episode = parse_with_context("S01E01.Lost.in.the.Amazon.mp4", &context);
        let candidate = &episode;
        assert_eq!(candidate.title.as_deref(), Some("A Miss Mallard Mystery"));
        assert_eq!(candidate.episodes, vec![1]);
    }

    #[test]
    fn season_directory_context_resolves_episode_and_compact_numbers() {
        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec!["The Office", "Season 4", "series"],
        };
        let episode = parse_with_context("Episode 4 - Kim Kelly Is My Friend.mkv", &context);
        assert_eq!(episode.kind, MediaKind::Episode);
        assert_eq!(episode.season, Some(4));
        assert_eq!(episode.episodes, vec![4]);

        let compact = parse_with_context("The Office [401] Fun Run.avi", &context);
        assert_eq!(compact.season, Some(4));
        assert_eq!(compact.episodes, vec![1]);
        assert_eq!(compact.episode_title.as_deref(), Some("Fun Run"));
    }

    #[test]
    fn spaced_dash_episode_inherits_season_from_release_directory() {
        let filename = first("Dorohedoro S2 - 07 (1080p).mkv");
        assert_eq!(filename.kind, MediaKind::Episode);
        assert_eq!(filename.title.as_deref(), Some("Dorohedoro"));
        assert_eq!(filename.season, Some(2));
        assert_eq!(filename.episodes, vec![7]);

        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec!["Dorohedoro.S02.1080p.CR.WEB-DL", "series"],
        };
        let inherited = parse_with_context("Dorohedoro - 07 (1080p).mkv", &context);
        assert_eq!(inherited.kind, MediaKind::Episode);
        assert_eq!(inherited.season, Some(2));
        assert_eq!(inherited.episodes, vec![7]);
        assert_eq!(
            inherited.episode_numbering,
            Some(EpisodeNumbering::SeasonEpisode)
        );
    }

    #[test]
    fn parses_adjacent_season_episode_markers() {
        let result = first("The.Wire.S0404.1080p.BluRay.DD5.1.x264-CtrlHD.mkv");
        assert_eq!(result.kind, MediaKind::Episode);
        assert_eq!(result.title.as_deref(), Some("The Wire"));
        assert_eq!(result.season, Some(4));
        assert_eq!(result.episodes, vec![4]);
    }

    #[test]
    fn classifies_unbracketed_anime_menu_opening_and_promo_markers() {
        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec![],
        };
        let cases = [
            (
                "JoJo no Kimyou na Bouken 2012 - Vol.3 Menu [BD 1920x1080 AVC FLAC].mkv",
                ExtraCategory::Menu,
                Some("JoJo no Kimyou na Bouken"),
            ),
            (
                "JoJo no Kimyou na Bouken 2012 - Non-Credit ED 2 [BD 1920x1080 AVC FLAC].mkv",
                ExtraCategory::Ending,
                Some("JoJo no Kimyou na Bouken"),
            ),
            (
                "JoJo no Kimyou na Bouken 2012 - PV第3弾 [BD 1920x1080 AVC FLAC].mkv",
                ExtraCategory::PromotionalVideo,
                Some("JoJo no Kimyou na Bouken"),
            ),
            (
                "JoJo no Kimyou na Bouken 2012 - 番宣CM集 [BD 1920x1080 AVC FLAC].mkv",
                ExtraCategory::Commercial,
                Some("JoJo no Kimyou na Bouken"),
            ),
            (
                "ノンテロップOP2 [BD 1920x1080 HEVC FLAC].mkv",
                ExtraCategory::Opening,
                None,
            ),
            (
                "ノンテロップED3 [BD 1920x1080 HEVC FLAC].mkv",
                ExtraCategory::Ending,
                None,
            ),
        ];
        for (name, category, title) in cases {
            let report = parse_with_context(name, &context);
            let result = &report;
            assert_eq!(result.kind, MediaKind::Extra, "{name}");
            assert_eq!(result.title.as_deref(), title, "{name}");
            assert_eq!(result.year, name.contains("2012").then_some(2012), "{name}");
            assert_eq!(
                result.extra.as_ref().map(|info| info.category),
                Some(category),
                "{name}"
            );
        }
        let numbered = parse_with_context("ノンテロップOP2 [BD 1920x1080 HEVC FLAC].mkv", &context);
        assert_eq!(numbered.extra.as_ref().unwrap().number, Some(2));
    }

    #[test]
    fn parses_episode_ranges_split_across_dash_tokens() {
        let result = first("Show.S01E01-E02-E03.1080p.WEB-DL.mkv");
        assert_eq!(result.season, Some(1));
        assert_eq!(result.episodes, vec![1, 2, 3]);
        assert_eq!(result.episode_title, None);
        assert_eq!(
            result.episode_numbering,
            Some(EpisodeNumbering::SeasonEpisode)
        );

        for (name, expected) in [
            ("Show.S04E29-31.1080p.WEB-DL.mkv", vec![29, 30, 31]),
            ("Show.S04E32-35.1080p.WEB-DL.mkv", vec![32, 33, 34, 35]),
        ] {
            assert_eq!(first(name).episodes, expected, "{name}");
        }
    }

    #[test]
    fn combines_adjacent_episode_anchors() {
        let cases = [
            (
                "Show.Name.S01E02.S01E03.HDTV.XViD.Etc-Group.mkv",
                Some(1),
                vec![2, 3],
                None,
            ),
            (
                "Show Name - S01.E02 - S01.E03 - S01.E04 - Episode Title HDTV.mkv",
                Some(1),
                vec![2, 3, 4],
                Some("Episode Title"),
            ),
            (
                "Phineas and Ferb S01E00 & S01E01 & S01E02.mkv",
                Some(1),
                vec![0, 1, 2],
                None,
            ),
            ("Show.Name.S01.E02.E03.1080p.mkv", Some(1), vec![2, 3], None),
        ];

        for (name, season, episodes, episode_title) in cases {
            let candidate = &parse(name);
            assert_eq!(candidate.kind, MediaKind::Episode, "{name}");
            assert_eq!(candidate.season, season, "{name}");
            assert_eq!(candidate.episodes, episodes, "{name}");
            assert_eq!(candidate.episode_title.as_deref(), episode_title, "{name}");
        }
    }

    #[test]
    fn spaced_season_episode_markers_identify_an_episode() {
        let report =
            parse("SUPERNATURAL - S08 E07 - A LITTLE SLICE OF KEVIN 720P DD5.1 x264 MMI.mkv");

        assert_eq!(report.kind, MediaKind::Episode);
        assert_eq!(report.title.as_deref(), Some("SUPERNATURAL"));
        assert_eq!(report.season, Some(8));
        assert_eq!(report.episodes, vec![7]);
    }

    #[test]
    fn episode_only_markers_use_season_unspecified_numbering() {
        let result =
            first("Watashi.wo.Hanasanaide.E03.2016.Amazon.WEB-DL.1080p.H264.DDP-HDCTV.mkv");
        assert_eq!(result.kind, MediaKind::Episode);
        assert_eq!(result.title.as_deref(), Some("Watashi wo Hanasanaide"));
        assert_eq!(result.episodes, vec![3]);
        assert!(result.absolute_episodes.is_empty());
        assert_eq!(
            result.episode_numbering,
            Some(EpisodeNumbering::SeasonUnspecified)
        );
        assert_eq!(result.year, Some(2016));
        assert_eq!(result.episode_title, None);
    }

    #[test]
    fn distinguishes_dimensions_from_episode_cross_and_parses_ep_style() {
        let result = first("JoJo no Kimyou na Bouken 2012 - EP09 [BD 1920x1080 AVC FLAC].mkv");
        assert_eq!(result.kind, MediaKind::Episode);
        assert_eq!(result.title.as_deref(), Some("JoJo no Kimyou na Bouken"));
        assert_eq!(result.year, Some(2012));
        assert_eq!(result.season, None);
        assert_eq!(result.episodes, vec![9]);
        assert_eq!(result.resolution.as_deref(), Some("1080p"));
        assert_eq!(result.source.as_deref(), Some("Blu-ray"));
    }

    #[test]
    fn episode_markers_and_technical_numbers_have_distinct_roles() {
        let context = ParseContext {
            expected_kind: Some(ExpectedKind::Series),
            ancestors: vec![],
        };
        let compact = parse_with_context("The.Office.S06xE01.avi", &context);
        assert_eq!(compact.kind, MediaKind::Episode);
        assert_eq!(compact.season, Some(6));
        assert_eq!(compact.episodes, vec![1]);

        let word_marker = parse_with_context("Naruto Shippuden Episode 366 VOSTFR.avi", &context);
        assert_eq!(word_marker.kind, MediaKind::Episode);
        assert_eq!(word_marker.title.as_deref(), Some("Naruto Shippuden"));
        assert_eq!(word_marker.episodes, vec![366]);

        let anime = parse_with_context(
            "[Kaylith] Zankyou no Terror - 04 [480p][B4D4514E].mp4",
            &context,
        );
        assert_eq!(anime.kind, MediaKind::Episode);
        assert_eq!(anime.title.as_deref(), Some("Zankyou no Terror"));
        assert_eq!(anime.absolute_episodes, vec![4]);

        let bracketed = parse("[Group] Zankyou no Terror [04][480p][B4D4514E].mp4");
        assert_eq!(bracketed.kind, MediaKind::Episode);
        assert_eq!(bracketed.absolute_episodes, vec![4]);

        let x_range = parse_with_context("Show_Name.1x02x03x04.HDTV_XViD_Etc-Group.mkv", &context);
        assert_eq!(x_range.season, Some(1));
        assert_eq!(x_range.episodes, vec![2, 3, 4]);

        let season_x_range = parse_with_context("Kaamelott - 5x44x45x46.avi", &context);
        assert_eq!(season_x_range.season, Some(5));
        assert_eq!(season_x_range.episodes, vec![44, 45, 46]);

        let movie = parse_with_context(
            "Persepolis (2007) [H264+Aac-128(Fr-Eng)+ST(Fr-Eng)+Ind].mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Movie),
                ancestors: vec![],
            },
        );
        assert_eq!(movie.kind, MediaKind::Movie);
        assert!(movie.episodes.is_empty());
    }

    #[test]
    fn recognizes_anime_bracket_episode() {
        let result = first(
            "[SweetSub&VCB-Studio] Made in Abyss Retsujitsu no Ougonkyou [09][Ma10p_1080p][x265_flac].mkv",
        );
        assert_eq!(result.kind, MediaKind::Episode);
        assert_eq!(result.absolute_episodes, vec![9]);
        assert_eq!(
            result.title.as_deref(),
            Some("Made in Abyss Retsujitsu no Ougonkyou")
        );
    }

    #[test]
    fn preserves_attached_title_hyphens_and_bracketed_titles() {
        for (name, title, episodes) in [
            (
                "[Team] Mini-hero Adventures [03][1080p].mkv",
                "Mini-hero Adventures",
                vec![3],
            ),
            ("[Cinder Lake][08][1080p].mkv", "Cinder Lake", vec![8]),
        ] {
            let candidate = first(name);
            assert_eq!(candidate.title.as_deref(), Some(title));
            assert_eq!(candidate.absolute_episodes, episodes);
        }
        assert_eq!(
            first("Spider-Man.2026.1080p.mkv").title.as_deref(),
            Some("Spider-Man")
        );
        assert_eq!(
            first("X-Men.97.S01E03.1080p.mkv").title.as_deref(),
            Some("X-Men 97")
        );
    }

    #[test]
    fn separates_absolute_series_subtitles_from_the_main_title() {
        for name in [
            "[Group] Series Name - Part Title [08][1080p].mkv",
            "[Group] Series Name — Part Title [08][1080p].mkv",
        ] {
            let candidate = first(name);
            assert_eq!(candidate.kind, MediaKind::Episode);
            assert_eq!(candidate.title.as_deref(), Some("Series Name"));
            assert_eq!(candidate.absolute_episodes, vec![8]);
        }
    }

    #[test]
    fn bracketed_anime_bonus_markers_separate_title_and_category() {
        let samples = [
            (
                "[Group] Made in Abyss [NCED02][Ma10p_1080p][x265_flac].mkv",
                ExtraCategory::Ending,
                Some(2),
            ),
            (
                "[Group] Made in Abyss [PV01][Ma10p_1080p][x265_flac].mkv",
                ExtraCategory::PromotionalVideo,
                Some(1),
            ),
            (
                "[Group] Made in Abyss [Menu01][Ma10p_1080p][x265_flac].mkv",
                ExtraCategory::Menu,
                Some(1),
            ),
            (
                "[Group] Made in Abyss [TVSP][Ma10p_1080p][x265_aac].mkv",
                ExtraCategory::TelevisionSpecial,
                None,
            ),
        ];
        for (name, category, number) in samples {
            let candidate = first(name);
            assert_eq!(candidate.kind, MediaKind::Extra, "{name}");
            assert_eq!(candidate.title.as_deref(), Some("Made in Abyss"), "{name}");
            assert_eq!(
                candidate.extra.as_ref().map(|x| x.category),
                Some(category),
                "{name}"
            );
            assert_eq!(
                candidate.extra.as_ref().and_then(|x| x.number),
                number,
                "{name}"
            );
        }
    }

    #[test]
    fn bracketed_extra_group_takes_precedence_over_episode_like_suffixes() {
        for (name, category) in [
            (
                "[UHA-WINGS] JoJo's Bizarre Adventure [NCED_EP38][Ma10p_1080p].mkv",
                ExtraCategory::Ending,
            ),
            (
                "[UHA-WINGS] JoJo's Bizarre Adventure [CM Collection 02][Ma10p_1080p].mkv",
                ExtraCategory::Commercial,
            ),
        ] {
            let report = parse_with_context(
                name,
                &ParseContext {
                    expected_kind: Some(ExpectedKind::Series),
                    ancestors: vec![],
                },
            );
            let result = &report;
            assert_eq!(result.kind, MediaKind::Extra);
            assert_eq!(
                result.extra.as_ref().map(|extra| extra.category),
                Some(category)
            );
            assert!(result.episodes.is_empty());
            assert!(result.absolute_episodes.is_empty());
        }
    }

    #[test]
    fn recognizes_unbracketed_japanese_cm_collections() {
        for (name, number) in [
            (
                "JoJo no Kimyou na Bouken 2012 - Blu-ray CM集 #10~#19 [BD 1080p].mkv",
                None,
            ),
            (
                "JoJo no Kimyou na Bouken 2012 - Blu-ray cm集② [BD 1080p].mkv",
                None,
            ),
        ] {
            let report = parse_with_context(
                name,
                &ParseContext {
                    expected_kind: Some(ExpectedKind::Series),
                    ancestors: vec![],
                },
            );
            let result = &report;
            assert_eq!(result.kind, MediaKind::Extra);
            assert_eq!(
                result.extra,
                Some(ExtraInfo {
                    category: ExtraCategory::Commercial,
                    number,
                })
            );
        }
    }

    #[test]
    fn anime_extras_and_explicit_movies_follow_series_context() {
        for (marker, category) in [
            ("CMs", ExtraCategory::Commercial),
            ("Mini Anime", ExtraCategory::MiniAnime),
            ("Prelude04", ExtraCategory::Prelude),
            ("Making02", ExtraCategory::MakingOf),
            ("Preview", ExtraCategory::Preview),
            ("Producer Logo", ExtraCategory::Logo),
        ] {
            let name = format!("[Fansub] Example Show [{marker}][Ma10p_1080p][x265_flac].mkv");
            let result = first(&name);
            assert_eq!(result.kind, MediaKind::Extra, "{name}");
            assert_eq!(result.title.as_deref(), Some("Example Show"), "{name}");
            assert_eq!(
                result.extra.as_ref().map(|x| x.category),
                Some(category),
                "{name}"
            );
        }

        let movie = parse_with_context(
            "Made.in.Abyss.Movie.3.Fukaki.Tamashii.no.Reimei.[Ma10p_1080p][x265_flac].mkv",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Series),
                ancestors: vec![],
            },
        );
        assert_eq!(movie.kind, MediaKind::Movie);
        assert_eq!(
            movie.title.as_deref(),
            Some("Made in Abyss Movie 3 Fukaki Tamashii no Reimei")
        );
    }

    #[test]
    fn generic_context_roots_yield_an_empty_title() {
        let report = parse_with_context(
            "S01E01.Lost.in.the.Amazon.mp4",
            &ParseContext {
                expected_kind: Some(ExpectedKind::Series),
                ancestors: vec!["series"],
            },
        );
        assert_eq!(report.title, None);
    }
}
