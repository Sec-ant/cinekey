#![forbid(unsafe_code)]

use cinekey::{ExpectedKind, ParseContext, ParseReport, parse_with_context};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::{
    fs::{self, ReadDir},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Parser)]
#[command(
    name = "cinekey",
    version,
    about = "Parse movie and TV release names into ranked, explainable candidates",
    arg_required_else_help = true,
    subcommand_required = true,
    after_help = "Examples:\n  cinekey parse 'Slow.Horses.S04E01.Identity.Theft.2160p.WEB-DL.DDP5.1.H.265-GROUP.mkv'\n  cinekey parse --json 'Fear.Street.Part.3.1666.2021.1080p.WEB-DL.mkv'\n  cinekey scan ./media/series --jsonl"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse a filename, release name, or path string.
    Parse {
        input: String,
        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
        /// Supply a movie or series hint.
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
    },
    /// Scan media files incrementally; results stream to stdout.
    Scan {
        #[arg(required=true, num_args=1..)]
        paths: Vec<PathBuf>,
        /// Visit only the supplied directory's immediate files.
        #[arg(long)]
        no_recursive: bool,
        /// Emit one JSON object per media file.
        #[arg(long, conflicts_with = "pretty")]
        jsonl: bool,
        /// Emit expanded JSON records.
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum KindArg {
    Movie,
    Series,
}
impl From<KindArg> for ExpectedKind {
    fn from(v: KindArg) -> Self {
        match v {
            KindArg::Movie => Self::Movie,
            KindArg::Series => Self::Series,
        }
    }
}

pub fn run() -> ExitCode {
    match run_inner(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        // A downstream command such as `head` may close a pipeline early.
        // A closed output pipe completes the command successfully.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cinekey: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_inner(cli: Cli) -> io::Result<()> {
    match cli.command {
        Command::Parse { input, json, kind } => {
            let mut context = context_for_path(Path::new(&input));
            if let Some(kind) = kind {
                context.expected_kind = Some(kind.into());
            }
            let report = parse_with_context(&input, &context);
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                serde_json::to_writer_pretty(&mut out, &report)?;
                out.write_all(b"\n")
            } else {
                write_human(&mut out, &report)
            }
        }
        Command::Scan {
            paths,
            no_recursive,
            jsonl,
            pretty,
        } => {
            let show_progress = io::stderr().is_terminal();
            if show_progress {
                eprintln!("Scanning {} input path(s)…", paths.len());
            }
            let stdout = io::stdout();
            let mut out = stdout.lock();
            let mut processed = 0usize;
            for path in &paths {
                walk(
                    path,
                    !no_recursive,
                    &mut out,
                    jsonl,
                    pretty,
                    &mut processed,
                    show_progress,
                )?;
            }
            if show_progress {
                eprintln!("Scanned {processed} video files.");
            }
            Ok(())
        }
    }
}

#[derive(Serialize)]
struct ScanRecord<'a> {
    path: &'a Path,
    report: &'a ParseReport,
}

/// Stream entries through a stack of open directory iterators.
/// Memory follows directory depth, the current path, and one parse result.
fn walk(
    path: &Path,
    recursive: bool,
    out: &mut impl Write,
    jsonl: bool,
    pretty: bool,
    processed: &mut usize,
    show_progress: bool,
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        return write_file(path, out, jsonl, pretty, processed, show_progress);
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut stack: Vec<ReadDir> = vec![fs::read_dir(path)?];
    while let Some(entries) = stack.last_mut() {
        let Some(next) = entries.next() else {
            stack.pop();
            continue;
        };
        let entry = next?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        let child = entry.path();
        if ty.is_file() {
            write_file(&child, out, jsonl, pretty, processed, show_progress)?;
        } else if recursive && ty.is_dir() {
            stack.push(fs::read_dir(child)?);
        }
    }
    Ok(())
}

fn write_file(
    path: &Path,
    out: &mut impl Write,
    jsonl: bool,
    pretty: bool,
    processed: &mut usize,
    show_progress: bool,
) -> io::Result<()> {
    if !is_video_file(path) {
        return Ok(());
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        eprintln!("cinekey: skipping non-UTF-8 filename: {}", path.display());
        return Ok(());
    };
    let context = context_for_path(path);
    let report = parse_with_context(name, &context);
    let record = ScanRecord {
        path,
        report: &report,
    };
    if jsonl {
        serde_json::to_writer(&mut *out, &record)?;
        out.write_all(b"\n")?;
    } else if pretty {
        serde_json::to_writer_pretty(&mut *out, &record)?;
        out.write_all(b"\n\n")?;
    } else {
        writeln!(out, "{}", path.display())?;
        write_human(out, &report)?;
        out.write_all(b"\n")?;
    }
    *processed += 1;
    if show_progress && (*processed).is_multiple_of(250) {
        eprintln!("Scanned {processed} video files…");
    }
    Ok(())
}

fn is_video_file(path: &Path) -> bool {
    const VIDEO_EXTENSIONS: &[&str] = &[
        "mkv", "mp4", "avi", "m4v", "mov", "ts", "webm", "wmv", "m2ts", "mpg", "mpeg", "ogv",
        "flv", "rmvb", "rm",
    ];
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

fn context_for_path(path: &Path) -> ParseContext<'_> {
    let expected_kind = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .find_map(|part| {
            if part.eq_ignore_ascii_case("movies") || part.eq_ignore_ascii_case("movie") {
                Some(ExpectedKind::Movie)
            } else if part.eq_ignore_ascii_case("series") || part.eq_ignore_ascii_case("tv") {
                Some(ExpectedKind::Series)
            } else {
                None
            }
        });
    let ancestors = path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .filter_map(Path::file_name)
        .filter_map(|n| n.to_str())
        .collect();
    ParseContext {
        expected_kind,
        ancestors,
    }
}

fn write_human(out: &mut impl Write, report: &ParseReport) -> io::Result<()> {
    for (index, c) in report.candidates.iter().enumerate() {
        writeln!(
            out,
            "{}. {:?} — {} ({:?} confidence, score {})",
            index + 1,
            c.kind,
            c.title.as_deref().unwrap_or("<title unknown>"),
            c.confidence,
            c.score
        )?;
        for annotation in &c.title_annotations {
            writeln!(out, "   title annotation: {annotation}")?;
        }
        if let Some(v) = c.year {
            writeln!(out, "   year: {v}")?;
        }
        if let Some(v) = c.season {
            writeln!(out, "   season: {v}")?;
        }
        if let Some(numbering) = c.episode_numbering {
            writeln!(out, "   episode numbering: {numbering:?}")?;
        }
        if !c.episodes.is_empty() {
            writeln!(out, "   episodes: {:?}", c.episodes)?;
        }
        if !c.absolute_episodes.is_empty() {
            writeln!(out, "   absolute episodes: {:?}", c.absolute_episodes)?;
        }
        if let Some(v) = &c.episode_title {
            writeln!(out, "   episode title: {v}")?;
        }
        if let Some(extra) = &c.extra {
            writeln!(
                out,
                "   extra: {:?}{}",
                extra.category,
                extra.number.map_or(String::new(), |n| format!(" #{n}"))
            )?;
        }
        if let Some(v) = &c.resolution {
            writeln!(out, "   resolution: {v}")?;
        }
        if let Some(v) = &c.source {
            writeln!(out, "   source: {v}")?;
        }
        if let Some(v) = &c.video_codec {
            writeln!(out, "   video codec: {v}")?;
        }
        if !c.dynamic_range.is_empty() {
            writeln!(out, "   dynamic range: {}", c.dynamic_range.join(", "))?;
        }
        if !c.audio_codec.is_empty() {
            writeln!(out, "   audio: {}", c.audio_codec.join(", "))?;
        }
        if let Some(v) = &c.audio_channels {
            writeln!(out, "   channels: {v}")?;
        }
        if let Some(v) = &c.release_group {
            writeln!(out, "   release group: {v}")?;
        }
        for e in &c.evidence {
            writeln!(
                out,
                "   evidence: {:?} <- {} ({:?}: {:?}, {}..{})",
                e.field, e.rule, e.source, e.text, e.span.start, e.span.end
            )?;
        }
        for note in &c.diagnostics {
            writeln!(out, "   note: {note}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_context_supplies_kind_and_nearest_ancestor() {
        let p = Path::new("/media/series/Show/Season 1/E01.mkv");
        let c = context_for_path(p);
        assert_eq!(c.expected_kind, Some(ExpectedKind::Series));
        assert_eq!(c.ancestors.first().copied(), Some("Season 1"));
    }
    #[test]
    fn scan_selects_video_extensions() {
        assert!(is_video_file(Path::new("Episode.MKV")));
        assert!(!is_video_file(Path::new("Episode.srt")));
    }

    #[test]
    fn human_output_presents_title_annotations() {
        let report = cinekey::parse("Example (Another Name).2020.1080p.mkv");
        let mut output = Vec::new();
        write_human(&mut output, &report).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Movie — Example"));
        assert!(output.contains("title annotation: Another Name"));
    }
}
