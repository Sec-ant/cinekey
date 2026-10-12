#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fs,
    hint::black_box,
    io::{self, BufRead, Write},
    path::Path,
    time::{Duration, Instant},
};

use cinekey::{ExpectedKind, ParseContext};
use clap::{Parser, Subcommand};
use serde_json::Value;

mod prepare;
mod scoring;

const LOCAL_BATCH_SIZE: usize = 128;
const LOCAL_PASSES: usize = 20;

struct LocalCorpusRow {
    name: String,
    expected_kind: ExpectedKind,
    ancestors: Vec<String>,
}

#[derive(Debug, Parser)]
#[command(
    about = "Prepare, score, and benchmark saved media-name corpora",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export all parser results as JSONL.
    Results { corpus: std::path::PathBuf },
    /// Compare native throughput of all three parsers.
    Benchmark { corpus: std::path::PathBuf },
    /// Measure cinekey throughput in three rounds.
    BenchmarkCinekey { corpus: std::path::PathBuf },
    /// Replay basenames and ancestor context.
    Replay { corpus: std::path::PathBuf },
    /// Convert pinned public fixtures into shared JSONL expectations.
    Prepare {
        #[arg(value_enum)]
        source: prepare::Source,
        fixtures: std::path::PathBuf,
        output: std::path::PathBuf,
    },
    /// Score primary results against shared identification expectations.
    Score {
        #[arg(value_enum)]
        scope: scoring::Scope,
        expectations: std::path::PathBuf,
        results: std::path::PathBuf,
        #[arg(long)]
        output_dir: std::path::PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cinekey-eval: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Results { corpus } => export_local_results(&corpus),
        Command::Benchmark { corpus } => benchmark_corpus(&corpus),
        Command::BenchmarkCinekey { corpus } => benchmark_cinekey_corpus(&corpus),
        Command::Replay { corpus } => replay_local_corpus(&corpus),
        Command::Prepare {
            source,
            fixtures,
            output,
        } => prepare::run(source, &fixtures, &output),
        Command::Score {
            scope,
            expectations,
            results,
            output_dir,
        } => scoring::run(scope, &expectations, &results, &output_dir),
    }
}

fn context_from_row(row: &Value) -> Result<ParseContext<'_>, Box<dyn std::error::Error>> {
    let expected_kind = serde_json::from_value(row["expected_kind"].clone())?;
    let ancestors = row["ancestors"]
        .as_array()
        .ok_or("expected ancestors array")?
        .iter()
        .map(|ancestor| ancestor.as_str().ok_or("expected string ancestor"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ParseContext {
        expected_kind,
        ancestors,
    })
}

fn export_local_results(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let pipeline = hunch::Pipeline::new();
    let options = oleoo::Options {
        current_year: Some(2026),
        ..oleoo::Options::default()
    };
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for (index, line) in io::BufReader::new(fs::File::open(path)?)
        .lines()
        .enumerate()
    {
        let row: Value = serde_json::from_str(&line?)?;
        let name = row["name"]
            .as_str()
            .ok_or_else(|| format!("{}:{}: missing string name", path.display(), index + 1))?;
        let context = context_from_row(&row)?;
        let oleoo = oleoo::parse(name, &options);
        let result = serde_json::json!({
            "name": name,
            "cinekey": cinekey::parse(name),
            "contextual": cinekey::parse_with_context(name, &context),
            "hunch": pipeline.run(name).to_flat_map(),
            "oleoo": oleoo.as_ref().ok(),
            "oleoo_error": oleoo.as_ref().err().map(ToString::to_string),
        });
        serde_json::to_writer(&mut out, &result)?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

fn benchmark_corpus(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut names = Vec::new();
    for line in io::BufReader::new(fs::File::open(path)?).lines() {
        let row: Value = serde_json::from_str(&line?)?;
        names.push(
            row["name"]
                .as_str()
                .ok_or("missing string name")?
                .to_owned(),
        );
    }
    if names.is_empty() {
        return Err("benchmark corpus must contain at least one name".into());
    }
    let pipeline = hunch::Pipeline::new();
    let options = oleoo::Options {
        current_year: Some(2026),
        ..oleoo::Options::default()
    };
    for name in &names {
        black_box(cinekey::parse(black_box(name.as_str())));
        let _ = black_box(pipeline.run(black_box(name.as_str())));
        let _ = black_box(oleoo::parse(black_box(name.as_str()), &options));
    }
    let mut totals = LocalTotals::default();
    for chunk in names.chunks(LOCAL_BATCH_SIZE) {
        process_batch(&mut chunk.to_vec(), &pipeline, &options, &mut totals);
    }
    let calls = totals.files * LOCAL_PASSES;
    let measurement = serde_json::json!({
        "files": totals.files,
        "passes": LOCAL_PASSES,
        "warmup_passes": 1,
        "calls_per_parser": calls,
        "batch_size": LOCAL_BATCH_SIZE,
        "parsers": {
            "cinekey": {"seconds": totals.cinekey.as_secs_f64(), "calls_per_second": calls as f64 / totals.cinekey.as_secs_f64()},
            "hunch": {"seconds": totals.hunch.as_secs_f64(), "calls_per_second": calls as f64 / totals.hunch.as_secs_f64()},
            "oleoo": {"seconds": totals.oleoo.as_secs_f64(), "calls_per_second": calls as f64 / totals.oleoo.as_secs_f64()},
        },
    });
    println!("{}", serde_json::to_string_pretty(&measurement)?);
    Ok(())
}

fn benchmark_cinekey_corpus(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    const PASSES: usize = 256;
    let names = io::BufReader::new(fs::File::open(path)?)
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(&line?)?;
            Ok(row["name"]
                .as_str()
                .ok_or("missing string name")?
                .to_owned())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    if names.is_empty() {
        return Err("benchmark corpus must contain at least one name".into());
    }
    for name in &names {
        black_box(cinekey::parse(black_box(name.as_str())));
    }
    let mut seconds = [0.0; 3];
    for elapsed in &mut seconds {
        let start = Instant::now();
        for _ in 0..PASSES {
            for name in &names {
                black_box(cinekey::parse(black_box(name.as_str())));
            }
        }
        *elapsed = start.elapsed().as_secs_f64();
    }
    let mut sorted = seconds;
    sorted.sort_by(f64::total_cmp);
    let calls = names.len() * PASSES;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "files": names.len(), "passes": PASSES, "rounds": 3,
            "seconds": seconds, "median_seconds": sorted[1],
            "calls_per_second": calls as f64 / sorted[1],
        }))?
    );
    Ok(())
}

fn replay_local_corpus(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read_to_string(path)?;
    let mut rows = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let value: Value = serde_json::from_str(line)
            .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{}:{}: missing string `name`", path.display(), index + 1))?;
        let expected_kind = match value.get("expected_kind").and_then(Value::as_str) {
            Some("movie") => ExpectedKind::Movie,
            Some("series") => ExpectedKind::Series,
            _ => {
                return Err(format!(
                    "{}:{}: expected_kind must be `movie` or `series`",
                    path.display(),
                    index + 1
                )
                .into());
            }
        };
        let ancestors = value
            .get("ancestors")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                format!(
                    "{}:{}: missing `ancestors` array",
                    path.display(),
                    index + 1
                )
            })?
            .iter()
            .map(|ancestor| {
                ancestor.as_str().map(str::to_owned).ok_or_else(|| {
                    format!(
                        "{}:{}: ancestor must be a string",
                        path.display(),
                        index + 1
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        rows.push(LocalCorpusRow {
            name: name.to_owned(),
            expected_kind,
            ancestors,
        });
    }

    let mut missing_title = 0usize;
    let mut kinds = BTreeMap::<String, usize>::new();

    for row in &rows {
        std::hint::black_box(cinekey::parse(std::hint::black_box(row.name.as_str())));
    }
    let basename_elapsed = Instant::now();
    for _pass in 0..LOCAL_PASSES {
        for row in &rows {
            std::hint::black_box(cinekey::parse(std::hint::black_box(row.name.as_str())));
        }
    }
    let basename_elapsed = basename_elapsed.elapsed();

    let contextual_elapsed = Instant::now();
    for pass in 0..LOCAL_PASSES {
        for row in &rows {
            let context = ParseContext {
                expected_kind: Some(row.expected_kind),
                ancestors: row.ancestors.iter().map(String::as_str).collect(),
            };
            let report = cinekey::parse_with_context(&row.name, &context);
            if pass == 0 {
                *kinds.entry(format!("{:?}", report.kind)).or_default() += 1;
                missing_title += usize::from(report.title.is_none());
            }
            std::hint::black_box(report);
        }
    }
    let contextual_elapsed = contextual_elapsed.elapsed();
    let parses = rows.len() * LOCAL_PASSES;
    println!(
        "Replayed {} local names x {LOCAL_PASSES} passes per mode",
        rows.len()
    );
    println!("Parsed records without a title: {missing_title}");
    println!("Media kinds: {kinds:?}");
    if parses > 0 {
        println!(
            "Basename parser throughput: {:.0} calls/s",
            parses as f64 / basename_elapsed.as_secs_f64()
        );
        println!(
            "Path-context parser throughput: {:.0} calls/s",
            parses as f64 / contextual_elapsed.as_secs_f64()
        );
    }
    Ok(())
}

#[derive(Default)]
struct LocalTotals {
    files: usize,
    batches: usize,
    cinekey: Duration,
    hunch: Duration,
    oleoo: Duration,
}

fn process_batch(
    batch: &mut Vec<String>,
    pipeline: &hunch::Pipeline,
    options: &oleoo::Options,
    totals: &mut LocalTotals,
) {
    if batch.is_empty() {
        return;
    }
    let mut durations = [Duration::ZERO; 3];
    let first = totals.batches % 3;
    for offset in 0..3 {
        let parser = (first + offset) % 3;
        let start = Instant::now();
        match parser {
            0 => {
                for _ in 0..LOCAL_PASSES {
                    for name in batch.iter() {
                        black_box(cinekey::parse(black_box(name.as_str())));
                    }
                }
            }
            1 => {
                for _ in 0..LOCAL_PASSES {
                    for name in batch.iter() {
                        let _ = black_box(pipeline.run(black_box(name.as_str())));
                    }
                }
            }
            _ => {
                for _ in 0..LOCAL_PASSES {
                    for name in batch.iter() {
                        let _ = black_box(oleoo::parse(black_box(name.as_str()), options));
                    }
                }
            }
        }
        durations[parser] = start.elapsed();
    }
    totals.cinekey += durations[0];
    totals.hunch += durations[1];
    totals.oleoo += durations[2];
    totals.files += batch.len();
    totals.batches += 1;
    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_context_preserves_ancestor_order_and_kind() {
        let row = serde_json::json!({"expected_kind": "series", "ancestors": ["Season 1", "Example Show"]});
        let context = context_from_row(&row).unwrap();
        assert_eq!(context.expected_kind, Some(ExpectedKind::Series));
        assert_eq!(context.ancestors, ["Season 1", "Example Show"]);
    }

    #[test]
    fn public_corpus_context_accepts_optional_kind_and_empty_ancestors() {
        let row = serde_json::json!({"ancestors": []});
        let context = context_from_row(&row).unwrap();
        assert_eq!(context.expected_kind, None);
        assert!(context.ancestors.is_empty());
    }

    #[test]
    fn corpus_context_requires_string_ancestors() {
        assert!(context_from_row(&serde_json::json!({"ancestors": [1]})).is_err());
        assert!(context_from_row(&serde_json::json!({})).is_err());
    }
}
