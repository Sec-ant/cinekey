use super::episodes::{episode_number, parse_sxe, roman_season};
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

#[test]
fn release_groups_follow_release_anchors_across_file_names() {
    for input in [
        "Movie.2024.1080p.x264-GROUP.mkv",
        "Movie.2024.1080p.x264-GROUP.chs.ass",
        "Movie.2024.1080p.x264-GROUP.chs&eng.srt",
        "Movie.2024.1080p.x264-GROUP.eng.forced.srt",
        "Movie.2024.1080p.x264-GROUP.chs.mkv",
        "Show.S01E02-GROUP.chs.srt",
    ] {
        assert_eq!(
            parse(input).release_group.as_deref(),
            Some("GROUP"),
            "{input}"
        );
    }
    assert_eq!(
        parse("Watchmen.S01E06.This.Extraordinary.Being.2160p.MAX.WEB-DL.DTS-HD.MA.5.1.HDR.DoVi.H.265-NTb.mkv")
            .release_group
            .as_deref(),
        Some("NTb")
    );
    for input in [
        "chs.ass",
        "chs&eng.srt",
        "Mission - Impossible - The Final Reckoning (2025).mkv",
        "Movie - Impossible.chs.srt",
        "Movie.2024.WEB-DL.chs.srt",
        "Movie.2024.DTS-HD.MA.5.1.mkv",
        "-GROUP.chs.srt",
    ] {
        assert_eq!(parse(input).release_group, None, "{input}");
    }
}

fn first(input: &str) -> ParsedMedia {
    parse(input)
}

#[test]
fn parses_scene_episode_metadata() {
    let result =
        first("Slow.Horses.S04E01.Identity.Theft.2160p.ATVP.WEB-DL.DDP5.1.DV.HDR.H.265-NTb.mkv");
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
                    let input = format!("{prefix}{name}{separator}{marker}{separator}1080p.mkv");
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

    let numbered_installment =
        parse("Fear.Street.Part.3.1666.2021.1080p.NF.WEB-DL.DDP5.1.Atmos.DV.HDR.H.265-CRFW.mkv");
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
    let report = parse("SUPERNATURAL - S08 E07 - A LITTLE SLICE OF KEVIN 720P DD5.1 x264 MMI.mkv");

    assert_eq!(report.kind, MediaKind::Episode);
    assert_eq!(report.title.as_deref(), Some("SUPERNATURAL"));
    assert_eq!(report.season, Some(8));
    assert_eq!(report.episodes, vec![7]);
}

#[test]
fn episode_only_markers_use_season_unspecified_numbering() {
    let result = first("Watashi.wo.Hanasanaide.E03.2016.Amazon.WEB-DL.1080p.H264.DDP-HDCTV.mkv");
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
