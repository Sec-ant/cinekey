use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Default)]
/// Hints and ancestors belonging to the input's own directory path.
pub struct ParseContext<'a> {
    pub expected_kind: Option<ExpectedKind>,
    /// Ancestor directory names, ordered nearest first.
    pub ancestors: Vec<&'a str>,
}
