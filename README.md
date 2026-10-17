# cinekey

A Rust library and CLI for parsing movie and television release names, with a focus on Scene/P2P releases from PT trackers. Each input produces one record of identification fields and technical metadata.

[crates.io](https://crates.io/crates/cinekey) · [npm](https://www.npmjs.com/package/cinekey) · [API documentation](https://docs.rs/cinekey)

Rust 1.88 or newer is required. Licensed under MIT.

## Install

Prebuilt CLI archives for Linux, macOS, and Windows are available from [GitHub Releases](https://github.com/Sec-ant/cinekey/releases/latest). Each archive includes `cinekey` and `ck` (`.exe` on Windows).

```sh
cargo install cinekey --locked --profile dist
```

From a local checkout, `just install` also installs `ck`, a hard link to the same executable:

```sh
just install
ck --help
ck --json 'Arrival.2016.1080p.BluRay.mkv'
```

For a library dependency:

```toml
[dependencies]
cinekey = { version = "0.4", default-features = false }
```

## CLI

Pass one or more filenames, release names, or path strings directly. Inputs are processed in order, and path components supply local movie/series, title, and season hints. Shell globs and file discovery tools provide file lists.

```sh
cinekey 'Arrival.2016.1080p.BluRay.mkv' 'Example.S01E02.1080p.WEB-DL.mkv'
cinekey ./media/*.mkv
cinekey --kind series 'Episode.03.mkv'
```

The default output shows populated fields:

```text
input            : Arrival.2016.1080p.BluRay.mkv
kind             : movie
title            : Arrival
year             : 2016
resolution       : 1080p
source           : Blu-ray
```

`--json` emits one compact JSON object per input, with the original `input` and populated parsed fields at the top level. Absent scalar values and empty lists are omitted. Both single-input and batch output compose directly with `jq`:

```sh
cinekey --json 'Arrival.2016.1080p.BluRay.mkv' | jq '.title, .year'
cinekey --json ./media/*.mkv | jq -r '[.input, .title, .year] | @tsv'
```

`--from-file` reads one input per line from a list file; `-` reads stdin. Blank lines are skipped. Positional inputs are processed first, followed by the list in its original order.

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
| `edition` | Edition qualifiers, including Limited, Unrated, Extended, and Director's Cut |
| `resolution`, `source`, `streaming_service`, `video_codec`, `dynamic_range` | Resolution, release medium, originating service, video codec, and dynamic range |
| `video_transcoded` | Whether release markers indicate that the publisher transcoded the video |
| `audio_codec`, `audio_profile`, `audio_channels` | Audio codecs, their profiles, and channel layout |
| `release_flags`, `release_group` | Release status and packaging markers, followed by the release group |

Optional scalar fields use `Option`. Editions, release flags, and audio profiles use typed `EnumSet` bitsets, with stable, duplicate-free iteration and readable JSON arrays. Other repeated fields use vectors. Empty annotation lists and bitsets are omitted from JSON and deserialize as empty collections. `source` describes release media such as Blu-ray or WEB-DL; `streaming_service` identifies platforms such as Netflix (`NF`), Amazon (`AMZN`), Movies Anywhere (`MA`), and iTunes (`iT`). Audio context distinguishes DTS-HD Master Audio (`DTS-HD.MA`) from the Movies Anywhere service marker. Structural and release anchors select the best interpretation, including the release year when a title contains another year.

`video_transcoded` is an optional boolean inferred from release naming conventions. Encoder and rip markers indicate `true`; Remux, WEB-DL, Scene-style WEB with a codec, and Complete Blu-ray with AVC/H.264 or HEVC/H.265 indicate `false`. Explicit transcoding markers take priority over source-only hints. Insufficient or conflicting evidence leaves the field absent. It describes video processing relative to the release's source; audio conversion and container changes are independent.

## WebAssembly

The [npm package](https://www.npmjs.com/package/cinekey) includes browser and Node bindings, Wasm, and TypeScript declarations:

```sh
npm install cinekey
```

The Rust library supports `wasm32-unknown-unknown` with `default-features = false`. The `cinekey-wasm` workspace package provides browser and Node bindings over the same parser. `parse` and `parseWithContext` return plain JavaScript objects with the fields above, nullable scalar values, and native arrays. Generated TypeScript declarations describe both inputs and results.

In browsers, initialize the ES module once, then call the synchronous parser:

```js
import init, { parse, parseWithContext } from "cinekey/web";

await init();
const movie = parse("Arrival.2016.1080p.BluRay.mkv");
const episode = parseWithContext("E02.mkv", {
  expected_kind: "series",
  ancestors: ["Season 1", "Example.Show.S01.1080p"],
});
```

In Node.js, both ESM and CommonJS load the parser synchronously:

```js
import { parse } from "cinekey";
const movie = parse("Arrival.2016.1080p.BluRay.mkv");
```

```js
const { parse } = require("cinekey");
const movie = parse("Arrival.2016.1080p.BluRay.mkv");
```

Ancestor names are ordered nearest first. Context fields are optional; invalid field types and kind values produce JavaScript errors. Bindings serialize metadata directly into JavaScript values. The adapter's dependencies belong to its own crate; the Rust library uses Serde, compact enum sets, and byte search.

The root entry selects Node bindings in Node.js and browser bindings in other environments. `cinekey/web` and `cinekey/nodejs` select an explicit target. The Wasm files are exported as `cinekey/web/cinekey_bg.wasm` and `cinekey/nodejs/cinekey_bg.wasm` for asset handling and custom initialization. [GitHub Releases](https://github.com/Sec-ant/cinekey/releases/latest) also provide separate `wasm-web` and `wasm-nodejs` archives and the npm tarball.

For local builds, install the Wasm Rust target and `wasm-bindgen-cli` 0.2.129:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
just wasm
just wasm nodejs
just npm-pack
```

`just wasm` writes browser bindings to `pkg/web/`; `just wasm nodejs` writes Node bindings to `pkg/nodejs/`. `just npm-pack` builds both targets, derives the npm version from Cargo, and writes `dist/cinekey-<version>.tgz`.

## Parsing and resource use

- A Unicode-aware lexer borrows token slices and records separators, bracket state, and byte offsets. Internal release tokens retain compact word classifications for reuse by structural and technical recognizers. These classifications fit within the token layout on native and Wasm targets, and tokenization uses one shared scanner.
- Episode grammar handles compact and spaced season/episode markers, explicit `E`/`EP` sequences, inclusive ranges, lists, revisions, season packs, and anchored anime absolute numbers. Season words support numeric and canonical Roman values.
- Title boundaries use structural, year, and technical anchors. Titles retain accents, installment numbers, inline parenthetical text, and complete movie subtitles. Trailing parenthesized qualifiers populate `title_annotations`.
- Technical recognizers share a word and phrase vocabulary for boundaries, services, editions, release flags, and audio profiles. They normalize source and codec aliases, compound release phrases, split codec spellings, HDR markers, and audio layouts. Bonus-content recognizers identify common supplement categories and ordinals.
- Release-group recognition uses structural release markers and dot or dash boundaries. Group names retain internal hyphens and end at the next dot, so other filename segments can follow them across file extensions.
- Path context supplies missing titles and seasons from structurally recognizable ancestors on the input's own path. Explicit filename structure participates in selection alongside the supplied hints.
- The library and CLI forbid `unsafe`. Parsing uses borrowed tokens, bounded stack keys, static dispatch, and one returned metadata record. Service identifiers and metadata bitsets occupy inline storage and serialize through static labels. Byte search locates path separators while preserving UTF-8 filename boundaries. The CLI streams input lists and output records; memory follows the argument list, current input, ancestor names, and current parse result.

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
just npm-pack
just wasm-check
just wasm-test
just bench
just eval --help
just clean
```

`just fmt`, `just clippy`, `just test`, and `just doc` run individual checks; `just fmt-fix` formats sources. `just install` installs the distribution profile. `just wasm-check` checks the Wasm target, and `just wasm-test` runs the bindings tests in Node through `wasm-bindgen-test-runner`. The commands share the root `target/` directory, and `just clean` reclaims its build artifacts. Generated Wasm packages stay in `pkg/`. Cargo commands can also be run directly.

The dependency-free release benchmark parses six representative names one million times, alternating plain parsing and ancestor context. The separate Rust package at `tools/eval` prepares public fixtures, scores identification results, exports parser output, and measures corpus throughput with pinned Hunch and Oleoo versions. Saved local inputs and references stay in ignored development directories.

The Rust API follows [Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/) through explicit safety policy, typed interfaces, documented examples, formatting, and static lint checks.

## Releasing

Releases use [Knope](https://knope.tech/) 0.23.0 and Changesets-style Markdown files. Describe a user-facing change and select its release level:

```sh
just change
```

Commit the generated `.changeset/*.md` file with the implementation. Use `patch` for fixes, `minor` for features, and `major` for breaking changes. For `0.x` releases, breaking changes advance the minor number; features and fixes advance the patch number. [Contributing](CONTRIBUTING.md) covers installation and the change-file format.

When changes reach `main`, CI creates or updates the `knope/release` branch and its Release PR. The PR combines pending changes, synchronizes Cargo versions and lockfiles, updates `CHANGELOG.md`, and runs CI. Rust, CLI, and npm share one release version; the npm manifest derives its version from Cargo during packaging.

Merge the Release PR to publish. CI creates an annotated `v<version>` tag and starts the release workflow at that tag. The workflow checks the version, runs CI, and builds CLI archives for x86-64 and ARM64 Linux (GNU and musl), macOS, and Windows, plus browser and Node Wasm packages. Builds use the `dist` profile.

After all checks and builds succeed, the workflow uploads the archives, npm tarball, and `SHA256SUMS` to a draft GitHub Release, publishes the crate and npm package through Trusted Publishing, and publishes the GitHub Release with the changelog entry as its release notes. Both registries bind `Sec-ant/cinekey` to `release.yml`. npm uses GitHub OIDC and generates provenance automatically. Prerelease npm versions use the `next` dist-tag.

The release workflow also supports manual runs with an optional version tag. An empty tag builds branch artifacts; a version tag builds and publishes that release. Build artifacts remain available for seven days. All jobs use the same source commit. Failed runs can be rerun for the same tag; existing registry versions are checked against the tag's commit. `just release` provides a manual tag-push entry point for a committed release version.
