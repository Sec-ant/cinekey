# cinekey

A Rust library and CLI for parsing movie and television release names, with a focus on Scene/P2P releases from PT trackers. Each input produces one record of identification fields and technical metadata.

Rust 1.88 or newer is required. Licensed under MIT.

## Install

```sh
cargo install --git https://github.com/Sec-ant/cinekey --locked --profile dist cinekey
```

From a local checkout:

```sh
just install
```

For a library dependency:

```toml
[dependencies]
cinekey = { git = "https://github.com/Sec-ant/cinekey", default-features = false }
```

## CLI

Pass one or more filenames, release names, or path strings directly. Inputs are processed in order, and path components supply local movie/series, title, and season hints. Shell globs and file discovery tools provide file lists.

```sh
cinekey 'Arrival.2016.1080p.BluRay.mkv' 'Example.S01E02.1080p.WEB-DL.mkv'
cinekey ./media/*.mkv
cinekey --kind series 'Episode.03.mkv'
```

The default output presents parsed fields over multiple lines, ordered by identification, video metadata, audio metadata, and release group. A blank line separates records:

```text
input: Arrival.2016.1080p.BluRay.mkv
kind: movie
title: Arrival
year: 2016
resolution: 1080p
source: Blu-ray
```

`--json` emits one compact JSON object per input, with the original `input` and parsed fields at the top level. Both single-input and batch output compose directly with `jq`:

```sh
cinekey --json 'Arrival.2016.1080p.BluRay.mkv' | jq '.title, .year'
cinekey --json ./media/*.mkv | jq -r '[.input, .title, .year] | @tsv'
```

`--from-file` reads one input per line from a list file; `-` reads stdin. Blank lines are skipped, and repeated names retain their input order. Positional inputs are processed first, followed by the list. List input streams through a reusable line buffer and one result.

```sh
cinekey --from-file names.txt --json
find ./media -type f -name '*.mkv' -print | cinekey --from-file - --json | jq -r '.title'
find ./media -type f -name '*.mkv' -exec cinekey --json {} +
```

`--kind movie` or `--kind series` supplies a hint for every input. Results go to stdout, errors go to stderr, and failures return a nonzero exit status. Closed output pipes complete successfully. The CLI follows the composable command and stream conventions in [clig.dev](https://clig.dev/).

## Library API

```rust
use cinekey::{ExpectedKind, MediaKind, ParseContext, parse, parse_with_context};

let media = parse("Arrival.2016.1080p.BluRay.mkv");
assert_eq!(media.kind, MediaKind::Movie);
assert_eq!(media.title.as_deref(), Some("Arrival"));
assert_eq!(media.year, Some(2016));

let context = ParseContext {
    expected_kind: Some(ExpectedKind::Series),
    ancestors: vec!["Season 1", "Example.Show.S01.1080p"],
};
let media = parse_with_context("E02.mkv", &context);
assert_eq!(media.title.as_deref(), Some("Example Show"));
assert_eq!(media.season, Some(1));
assert_eq!(media.episodes, [2]);
```

`parse` accepts `&str` and parses its final filename component. `parse_with_context` also accepts kind and ancestor hints supplied by the caller. Both return `ParsedMedia` directly.

### Output

| Fields | Meaning |
| --- | --- |
| `kind`, `title`, `year` | Movie, episode, season pack, extra, or unknown; primary title and release year |
| `season`, `episodes`, `absolute_episodes`, `episode_numbering` | Season-relative, absolute, or season-unspecified numbering |
| `episode_title`, `extra` | Episode subtitle or bonus-content category and ordinal |
| `title_annotations` | Trailing parenthesized names and qualifiers, in input order |
| `resolution`, `source`, `video_codec`, `dynamic_range` | Normalized video and release metadata |
| `audio_codec`, `audio_channels`, `release_group` | Audio metadata and release group |

Optional scalar fields use `Option`; repeated metadata values use vectors. Empty annotation lists are omitted from JSON and deserialize as empty lists. `source` describes release media such as Blu-ray or WEB-DL. Structural and release anchors select the best interpretation, including the release year when a title contains another year.

## WebAssembly

The Rust library supports `wasm32-unknown-unknown` with `default-features = false`. The `cinekey-wasm` workspace package provides browser and Node bindings over the same parser. `parse` and `parseWithContext` return plain JavaScript objects with the fields above, nullable scalar values, and native arrays. Generated TypeScript declarations describe both inputs and results.

Install the Wasm Rust target and `wasm-bindgen-cli` 0.2.129:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
just wasm
just wasm nodejs
```

`just wasm` writes a browser ES module, Wasm binary, and TypeScript declarations to `pkg/web/`. Initialize the module once, then call the synchronous parser:

```js
import init, { parse, parseWithContext } from "./pkg/web/cinekey.js";

await init();
const movie = parse("Arrival.2016.1080p.BluRay.mkv");
const episode = parseWithContext("E02.mkv", {
  expected_kind: "series",
  ancestors: ["Season 1", "Example.Show.S01.1080p"],
});
```

`just wasm nodejs` writes a CommonJS package to `pkg/nodejs/` with synchronous initialization:

```js
const { parse } = require("./pkg/nodejs/cinekey.js");
const movie = parse("Arrival.2016.1080p.BluRay.mkv");
```

Ancestor names are ordered nearest first. Context fields are optional; invalid field types and kind values produce JavaScript errors. Bindings serialize metadata directly into JavaScript values. The adapter's dependencies belong to its own crate; the Rust library retains its parser and Serde dependencies.

## Parsing and resource use

- A Unicode-aware lexer borrows token slices and records separators, bracket state, and byte offsets. Structural recognizers combine these tokens with a static technical vocabulary.
- Episode grammar handles compact and spaced season/episode markers, explicit `E`/`EP` sequences, inclusive ranges, lists, revisions, season packs, and anchored anime absolute numbers. Season words support numeric and canonical Roman values.
- Title boundaries use structural, year, and technical anchors. Titles retain accents, installment numbers, inline parenthetical text, and complete movie subtitles. Trailing parenthesized qualifiers populate `title_annotations`.
- Technical recognizers normalize source and codec aliases, compound release phrases, split codec spellings, HDR markers, and audio layouts. Bonus-content recognizers identify common supplement categories and ordinals.
- Release-group recognition uses the dash anchor and structural release markers. A group ends at the next dot, so other filename segments can follow it across file extensions.
- Path context supplies missing titles and seasons from structurally recognizable ancestors on the input's own path. Explicit filename structure participates in selection alongside the supplied hints.
- The library and CLI forbid `unsafe`. Parsing uses borrowed tokens, bounded stack keys, static dispatch, and one returned metadata record. The CLI streams input lists and output records; memory follows the argument list, current input, ancestor names, and current parse result.

The `cli` feature supplies the binary, argument parsing, and JSON output. Library consumers can use `default-features = false` for the parser and its Serde data model. Modules separate the public model, lexer, structural selection, episode grammar, and technical vocabulary. Release builds use Thin LTO, one code-generation unit, and symbol stripping with optimization level 3. The `dist` profile uses full LTO and aborting panics for distribution executables. Wasm packaging removes debugging names and producer metadata. Both profiles keep the parser at optimization level 3.

The grammar targets naming conventions shared across releases. Bare compact digits have several plausible meanings; explicit markers and surrounding structure supply episode numbering. Broad title-boundary and multi-season conventions in other collections remain areas for greater compatibility.

## Accuracy and performance

On a saved local collection of 2,855 PT filenames, the parsed result matches all five identification fields for **96.95%** of basenames and **99.72%** with directory context. The contextual result matches all 2,312 episode records.

[Dataset definitions, full accuracy tables, throughput, memory measurements, and reproduction commands](tools/eval/DATASETS.md) cover the local collection and both public fixture sets. Public results differ by naming grammar and reference title conventions. The measurements describe the listed inputs and host.

## Development

[Just](https://just.systems/) provides a thin command layer over Cargo. Run `just` to list the available tasks:

```sh
just check
just build
just dist
just install
just wasm
just wasm-check
just wasm-test
just bench
just eval --help
just clean
```

`just fmt`, `just clippy`, `just test`, and `just doc` run individual checks; `just fmt-fix` formats sources. `just install` installs the distribution profile. `just wasm-check` checks the Wasm target, and `just wasm-test` runs the bindings tests in Node through `wasm-bindgen-test-runner`. The commands share the root `target/` directory, and `just clean` reclaims its build artifacts. Generated Wasm packages stay in `pkg/`. Cargo commands can also be run directly.

The dependency-free release benchmark parses six representative names one million times, alternating plain parsing and ancestor context. The separate Rust package at `tools/eval` prepares public fixtures, scores identification results, exports parser output, and measures corpus throughput with pinned Hunch and Oleoo versions. Saved local inputs and references stay in ignored development directories.

The Rust API follows [Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/) through explicit safety policy, typed interfaces, documented examples, formatting, and static lint checks.
