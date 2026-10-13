#![forbid(unsafe_code)]

use cinekey::{
    EpisodeNumbering, ExpectedKind, MediaKind, ParseContext, ParsedMedia, parse_with_context,
};
use clap::{Parser, ValueEnum};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{
    fmt::Display,
    fs::File,
    io::{self, BufRead, Write},
    path::PathBuf,
    process::ExitCode,
};

#[derive(Debug, Parser)]
#[command(
    name = "cinekey",
    version,
    about = "Parse movie and TV names into identification and technical metadata",
    arg_required_else_help = true,
    group = clap::ArgGroup::new("input_source").args(["inputs", "from_file"]).required(true).multiple(true),
    after_help = "Examples:\n  cinekey 'Arrival.2016.1080p.BluRay.mkv' 'Example.S01E02.mkv'\n  cinekey --json ./media/*.mkv\n  cinekey --from-file names.txt --json\n  cinekey --from-file - --json | jq -r '.title'"
)]
struct Cli {
    /// Filenames, release names, or path strings to parse, in order.
    #[arg(value_name = "INPUT", num_args = 1..)]
    inputs: Vec<String>,
    /// Read one input per line from a file; use "-" for stdin.
    #[arg(long, value_name = "PATH")]
    from_file: Option<PathBuf>,
    /// Emit one JSON object per input.
    #[arg(long)]
    json: bool,
    /// Supply a movie or series hint for every input.
    #[arg(long, value_enum)]
    kind: Option<KindArg>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum KindArg {
    Movie,
    Series,
}

impl From<KindArg> for ExpectedKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Movie => Self::Movie,
            KindArg::Series => Self::Series,
        }
    }
}

#[derive(Debug)]
struct OutputRecord<'a> {
    input: &'a str,
    media: &'a ParsedMedia,
}

impl Serialize for OutputRecord<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let media = self.media;
        let mut map = serializer.serialize_map(None)?;
        macro_rules! optional {
            ($field:ident) => {
                if let Some(value) = &media.$field {
                    map.serialize_entry(stringify!($field), value)?;
                }
            };
        }
        macro_rules! list {
            ($field:ident) => {
                if !media.$field.is_empty() {
                    map.serialize_entry(stringify!($field), &media.$field)?;
                }
            };
        }
        map.serialize_entry("input", self.input)?;
        map.serialize_entry("kind", &media.kind)?;
        optional!(title);
        list!(title_annotations);
        optional!(year);
        optional!(season);
        list!(episodes);
        list!(absolute_episodes);
        optional!(episode_numbering);
        optional!(episode_title);
        if let Some(extra) = &media.extra {
            #[derive(Serialize)]
            struct ExtraRecord<'a> {
                category: &'a cinekey::ExtraCategory,
                #[serde(skip_serializing_if = "Option::is_none")]
                number: Option<u16>,
            }
            map.serialize_entry(
                "extra",
                &ExtraRecord {
                    category: &extra.category,
                    number: extra.number,
                },
            )?;
        }
        optional!(resolution);
        optional!(source);
        optional!(video_codec);
        list!(dynamic_range);
        list!(audio_codec);
        optional!(audio_channels);
        optional!(release_group);
        map.end()
    }
}

pub fn run() -> ExitCode {
    match run_inner(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        // A closed output pipe completes the command successfully.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cinekey: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(cli: Cli) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut first = true;
    let hint = cli.kind.map(ExpectedKind::from);
    for input in cli.inputs {
        write_input(&mut out, &input, hint, cli.json, &mut first)?;
    }
    if let Some(path) = cli.from_file {
        if path.as_os_str() == "-" {
            write_lines(&mut out, io::stdin().lock(), hint, cli.json, &mut first)?;
        } else {
            write_lines(
                &mut out,
                io::BufReader::new(File::open(path)?),
                hint,
                cli.json,
                &mut first,
            )?;
        }
    }
    out.flush()
}

fn write_lines(
    out: &mut impl Write,
    mut input: impl BufRead,
    hint: Option<ExpectedKind>,
    json: bool,
    first: &mut bool,
) -> io::Result<()> {
    let mut line = String::new();
    while input.read_line(&mut line)? > 0 {
        let name = line.trim_end_matches(['\r', '\n']);
        if !name.trim().is_empty() {
            write_input(out, name, hint, json, first)?;
        }
        line.clear();
    }
    Ok(())
}

fn write_input(
    out: &mut impl Write,
    input: &str,
    hint: Option<ExpectedKind>,
    json: bool,
    first: &mut bool,
) -> io::Result<()> {
    let mut context = context_for_path(input);
    if hint.is_some() {
        context.expected_kind = hint;
    }
    let media = parse_with_context(input, &context);
    if json {
        serde_json::to_writer(
            &mut *out,
            &OutputRecord {
                input,
                media: &media,
            },
        )?;
        out.write_all(b"\n")?;
    } else {
        if !*first {
            out.write_all(b"\n")?;
        }
        write_field(out, "input", Some(input))?;
        write_human(out, &media)?;
    }
    *first = false;
    Ok(())
}

fn context_for_path(input: &str) -> ParseContext<'_> {
    let mut components = input.rsplit(['/', '\\']);
    components.next();
    let ancestors: Vec<_> = components
        .filter(|component| !component.is_empty())
        .collect();
    let expected_kind = ancestors.iter().rev().find_map(|part| {
        if part.eq_ignore_ascii_case("movies") || part.eq_ignore_ascii_case("movie") {
            Some(ExpectedKind::Movie)
        } else if part.eq_ignore_ascii_case("series") || part.eq_ignore_ascii_case("tv") {
            Some(ExpectedKind::Series)
        } else {
            None
        }
    });
    ParseContext {
        expected_kind,
        ancestors,
    }
}

const FIELD_WIDTH: usize = "episode_numbering".len();

fn write_field(out: &mut impl Write, field: &str, value: Option<impl Display>) -> io::Result<()> {
    if let Some(value) = value {
        writeln!(out, "{field:<FIELD_WIDTH$}: {value}")?;
    }
    Ok(())
}

fn write_list(out: &mut impl Write, field: &str, values: &[String]) -> io::Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    write!(out, "{field:<FIELD_WIDTH$}:")?;
    for (index, value) in values.iter().enumerate() {
        write!(out, "{}{value}", if index == 0 { " " } else { ", " })?;
    }
    writeln!(out)
}

fn write_human(out: &mut impl Write, media: &ParsedMedia) -> io::Result<()> {
    let kind = match media.kind {
        MediaKind::Movie => "movie",
        MediaKind::Episode => "episode",
        MediaKind::SeasonPack => "season_pack",
        MediaKind::Extra => "extra",
        MediaKind::Unknown => "unknown",
    };
    write_field(out, "kind", Some(kind))?;
    write_field(out, "title", media.title.as_deref())?;
    write_list(out, "title_annotations", &media.title_annotations)?;
    write_field(out, "year", media.year)?;
    write_field(out, "season", media.season)?;
    write_field(
        out,
        "episodes",
        (!media.episodes.is_empty()).then_some(format_args!("{:?}", media.episodes)),
    )?;
    write_field(
        out,
        "absolute_episodes",
        (!media.absolute_episodes.is_empty())
            .then_some(format_args!("{:?}", media.absolute_episodes)),
    )?;
    write_field(
        out,
        "episode_numbering",
        media.episode_numbering.map(|numbering| match numbering {
            EpisodeNumbering::SeasonEpisode => "season_episode",
            EpisodeNumbering::Absolute => "absolute",
            EpisodeNumbering::SeasonUnspecified => "season_unspecified",
        }),
    )?;
    write_field(out, "episode_title", media.episode_title.as_deref())?;
    if let Some(extra) = &media.extra {
        let category = serde_json::to_value(extra.category)?;
        write!(
            out,
            "{:<FIELD_WIDTH$}: {}",
            "extra",
            category.as_str().unwrap_or_default()
        )?;
        if let Some(value) = extra.number {
            write!(out, " {value}")?;
        }
        writeln!(out)?;
    }
    for (field, value) in [
        ("resolution", &media.resolution),
        ("source", &media.source),
        ("video_codec", &media.video_codec),
    ] {
        write_field(out, field, value.as_deref())?;
    }
    write_list(out, "dynamic_range", &media.dynamic_range)?;
    write_list(out, "audio_codec", &media.audio_codec)?;
    for (field, value) in [
        ("audio_channels", &media.audio_channels),
        ("release_group", &media.release_group),
    ] {
        write_field(out, field, value.as_deref())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_accept_multiple_inputs_and_input_lists() {
        let cli = Cli::try_parse_from(["cinekey", "First.2020.mkv", "Second.S01E02.mkv", "--json"])
            .unwrap();
        assert_eq!(cli.inputs, ["First.2020.mkv", "Second.S01E02.mkv"]);
        assert!(cli.json);
        let cli = Cli::try_parse_from(["cinekey", "--from-file", "-"]).unwrap();
        assert!(cli.inputs.is_empty());
        assert_eq!(cli.from_file, Some(PathBuf::from("-")));
    }

    #[test]
    fn path_context_supplies_kind_and_nearest_ancestor() {
        for input in [
            "/media/series/Example.S01.1080p/Season 1/E02.mkv",
            "C:\\media\\series\\Example.S01.1080p\\Season 1\\E02.mkv",
        ] {
            let context = context_for_path(input);
            assert_eq!(context.expected_kind, Some(ExpectedKind::Series));
            assert_eq!(context.ancestors.first().copied(), Some("Season 1"));
            let media = parse_with_context(input, &context);
            assert_eq!(media.title.as_deref(), Some("Example"));
            assert_eq!(media.season, Some(1));
            assert_eq!(media.episodes, [2]);
        }
    }

    #[test]
    fn human_output_presents_metadata_fields() {
        let media = cinekey::parse("Example (Another Name).2020.1080p.mkv");
        let mut output = Vec::new();
        write_human(&mut output, &media).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "kind             : movie\n",
                "title            : Example\n",
                "title_annotations: Another Name\n",
                "year             : 2020\n",
                "resolution       : 1080p\n",
            )
        );
    }

    #[test]
    fn json_lines_preserve_inputs_order_and_duplicate_records() {
        let input = b"First.2020.mkv\r\n\nSecond.2021.mkv\nFirst.2020.mkv\n";
        let mut output = Vec::new();
        write_lines(&mut output, input.as_slice(), None, true, &mut true).unwrap();
        let text = String::from_utf8(output).unwrap();
        let rows = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["input"], "First.2020.mkv");
        assert_eq!(rows[0]["title"], "First");
        assert_eq!(rows[1]["year"], 2021);
        assert_eq!(rows[2], rows[0]);
    }

    #[test]
    fn batch_human_output_separates_records_and_keeps_title_prefixes() {
        let mut output = Vec::new();
        write_lines(
            &mut output,
            b"#Alive.2020.mkv\nArrival.2016.mkv\n".as_slice(),
            None,
            false,
            &mut true,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("input            : #Alive.2020.mkv\nkind             : movie\ntitle            : #Alive\n"));
        assert!(output.contains("\n\ninput            : Arrival.2016.mkv\n"));
    }

    #[test]
    fn supplied_kind_applies_to_each_input() {
        let mut output = Vec::new();
        write_lines(
            &mut output,
            b"Example.1080p.mkv\nOther.1080p.mkv\n".as_slice(),
            Some(ExpectedKind::Series),
            true,
            &mut true,
        )
        .unwrap();
        for line in String::from_utf8(output).unwrap().lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(row["kind"], "unknown");
        }
    }
}
