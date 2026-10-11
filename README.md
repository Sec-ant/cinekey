# cinekey

A Rust library and CLI for parsing movie and television release names, with a focus on Scene/P2P releases from PT trackers. It returns ranked interpretations with identification fields, technical metadata, and evidence for each field.

Rust 1.88 or newer is required. Licensed under MIT.

## Install

```sh
cargo install --git https://github.com/Sec-ant/cinekey --locked cinekey
```

For a library dependency:

```toml
[dependencies]
cinekey = { git = "https://github.com/Sec-ant/cinekey" }
```

## CLI

`parse` accepts a filename, release name, or path-like **string**. It parses the final filename component and preserves the original input in the report. Path strings also supply local ancestor hints.

```sh
cinekey parse 'Slow.Horses.S04E01.Identity.Theft.2160p.WEB-DL.DDP5.1.H.265-GROUP.mkv'
cinekey parse --json 'Fear.Street.Part.3.1666.2021.1080p.WEB-DL.mkv'
cinekey parse --kind series --json 'Episode.03.mkv'
```

`scan` accepts file or directory **paths**, traverses directories, and supplies each filename with local ancestor context. Directories named `movies`/`movie` or `series`/`tv` provide the corresponding kind hint.

```sh
cinekey scan ./media/series ./media/movies --jsonl > scan.jsonl
cinekey scan ./media/movies --no-recursive --pretty
```

`--jsonl` emits one JSON object per video file. `--pretty` emits expanded JSON. The default output is human-readable. `cinekey --help` and each command's `--help` describe the options.

Results go to stdout, errors go to stderr, and failures return a nonzero exit status. Interactive scans report progress to stderr. Closed output pipes complete successfully. The CLI follows the composable command and stream conventions in [clig.dev](https://clig.dev/).

## Library API

```rust
use cinekey::{ExpectedKind, MediaKind, ParseContext, parse, parse_with_context};

let report = parse("Arrival.2016.1080p.BluRay.mkv");
let candidate = &report.candidates[0];
assert_eq!(candidate.kind, MediaKind::Movie);
assert_eq!(candidate.title.as_deref(), Some("Arrival"));
assert_eq!(candidate.year, Some(2016));

let context = ParseContext {
    expected_kind: Some(ExpectedKind::Series),
    ancestors: vec!["Season 1", "Example.Show.S01.1080p"],
};
let report = parse_with_context("E02.mkv", &context);
let candidate = &report.candidates[0];
assert_eq!(candidate.title.as_deref(), Some("Example Show"));
assert_eq!(candidate.season, Some(1));
assert_eq!(candidate.episodes, [2]);
```

### Output

`ParseReport` contains the original `input` and a `candidates` array ordered by descending score. Each candidate is a complete interpretation:

| Fields | Meaning |
| --- | --- |
| `kind`, `title`, `year` | Movie, episode, season pack, extra, or unknown; primary title and release year |
| `season`, `episodes`, `absolute_episodes`, `episode_numbering` | Season-relative, absolute, or season-unspecified numbering |
| `episode_title`, `extra` | Episode subtitle or bonus-content category and ordinal |
| `title_annotations` | Trailing parenthesized names and qualifiers, in input order |
| `resolution`, `source`, `video_codec`, `dynamic_range` | Normalized video and release metadata |
| `audio_codec`, `audio_channels`, `release_group` | Audio metadata and release group |
| `score`, `confidence` | Heuristic ranking and evidence-strength tier |
| `evidence`, `diagnostics` | Recognizer, source text, byte span, and interpretation notes |

Optional scalar fields use `Option`; repeated values use vectors. Empty annotation lists are omitted from JSON and deserialize as empty lists. Scores order hypotheses, and confidence tiers summarize heuristic evidence strength. Accuracy measurements use the first candidate.

Filename evidence uses half-open UTF-8 byte ranges into the final filename component, with recognized media or subtitle extensions stripped for parsing. Directory evidence uses offsets into the contributing ancestor name. `EvidenceSource` identifies the source. `input` retains the caller's complete original string.

## Parsing and resource use

- A Unicode-aware lexer borrows token slices and records separators, bracket state, and byte offsets. Structural recognizers combine these tokens with a static technical vocabulary.
- Episode grammar handles compact and spaced season/episode markers, explicit `E`/`EP` sequences, inclusive ranges, lists, revisions, season packs, and anchored anime absolute numbers. Season words support numeric and canonical Roman values.
- Title boundaries use structural, year, and technical anchors. Titles retain accents, installment numbers, inline parenthetical text, and complete movie subtitles. Trailing parenthesized qualifiers have their own annotation fields and evidence.
- Technical recognizers normalize source and codec aliases, compound release phrases, split codec spellings, HDR markers, and audio layouts. Bonus-content recognizers identify common supplement categories and ordinals.
- Path context supplies missing titles and seasons from structurally recognizable ancestors on the input's own path. Explicit filename structure participates in ranking alongside the supplied hints. Ambiguous structures can produce multiple candidates.
- The library and CLI forbid `unsafe`. Production parsing uses borrowed tokens, bounded stack keys, and static dispatch. The scanner streams records through an iterative directory traversal; memory follows directory depth, the current path, and one parse result. It skips symlinks and reads filesystem metadata and names.

The grammar targets naming conventions shared across releases. Bare compact digits have several plausible meanings; explicit markers and surrounding structure supply episode numbering. Broad title-boundary and multi-season conventions in other collections remain areas for greater compatibility.

## Accuracy and performance

On a saved local collection of 2,855 PT filenames, the primary result matches all five identification fields for **96.95%** of basenames and **99.72%** with directory context. The contextual result matches all 2,312 episode records. Basename throughput is approximately **665,000 calls/s**, compared with 10,100 for Hunch and 2,520 for Oleoo on the same strings and machine.

[Dataset definitions, full accuracy tables, throughput, memory measurements, and reproduction commands](tools/eval/DATASETS.md) cover the local collection and both public fixture sets. Public results differ by naming grammar and reference title conventions. These measurements describe the listed inputs and host.

## Development

[Just](https://just.systems/) provides a thin command layer over Cargo. Run `just` to list the available tasks:

```sh
just check
just build
just bench
just eval --help
just clean
```

`just fmt`, `just clippy`, `just test`, and `just doc` run individual checks; `just fmt-fix` formats sources. `just install` installs the local CLI. The commands share the root `target/` directory, and `just clean` reclaims its build artifacts. Cargo commands can also be run directly.

The dependency-free release benchmark parses six representative names one million times, alternating plain parsing and ancestor context. The separate Rust package at `tools/eval` prepares public fixtures, scores identification results, exports parser output, and measures corpus throughput with pinned Hunch and Oleoo versions. Saved local inputs and references stay in ignored development directories.

The Rust API follows [Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/) through explicit safety policy, typed interfaces, documented examples, formatting, and static lint checks.
