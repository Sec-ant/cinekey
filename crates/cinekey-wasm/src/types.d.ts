export type ExpectedKind = "movie" | "series";
export type MediaKind = "movie" | "episode" | "season_pack" | "extra" | "unknown";
export type EpisodeNumbering = "season_episode" | "absolute" | "season_unspecified";
export type ExtraCategory =
  | "opening" | "ending" | "promotional_video" | "commercial" | "menu"
  | "television_special" | "interview" | "commentary" | "chronicle" | "lecture"
  | "event" | "trailer" | "preview" | "making_of" | "music" | "logo"
  | "mini_anime" | "prelude" | "other";

export interface ParseContext {
  expected_kind?: ExpectedKind | null;
  /** Ancestor directory names, ordered nearest first. */
  ancestors?: string[];
}

export interface ParsedMedia {
  kind: MediaKind;
  title: string | null;
  title_annotations?: string[];
  year: number | null;
  season: number | null;
  episodes: number[];
  absolute_episodes: number[];
  episode_numbering: EpisodeNumbering | null;
  episode_title: string | null;
  extra: { category: ExtraCategory; number: number | null } | null;
  resolution: string | null;
  source: string | null;
  video_codec: string | null;
  dynamic_range: string[];
  audio_codec: string[];
  audio_channels: string | null;
  release_group: string | null;
}
