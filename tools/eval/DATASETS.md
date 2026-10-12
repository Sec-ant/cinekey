# Parser accuracy and performance

## Summary

Measured on 2026-10-12 on an Apple M3 Pro, `aarch64-apple-darwin`, with Rust 1.98.1 and Cargo release builds. The target corpus is the saved local Scene/P2P media collection from PT trackers. Public fixtures measure compatibility with broader naming conventions and their reference output policies.

Accuracy means the parsed result matches every scored identification field: title, year, season, complete episode list, and media kind. Each local and Oleoo record scores all five fields, including explicit empty values. Hunch records score the shared fields supplied by their fixtures. Strings compare case-insensitively with Unicode NFC and normalized whitespace. Latin title accents are equivalent across all parsers; punctuation retains its meaning. Lists compare complete values with order ignored.

## Three-parser comparison

All parsers receive identical input strings. Local inputs are saved basenames; public inputs retain the original fixture strings, including any directory components. Cinekey parses the final filename component, while Hunch can also interpret directory components. Throughput measures creation and destruction of each parser’s native result.

| Dataset | Parser | Correct records | Accuracy | Calls/s | μs/call |
| --- | --- | ---: | ---: | ---: | ---: |
| Local PT basenames | cinekey 0.2.0 | 2,768/2,855 | 96.95% | 1,018,633 | 0.98 |
| Local PT basenames | Hunch 2.0.2 | 2,329/2,855 | 81.58% | 11,261 | 88.81 |
| Local PT basenames | Oleoo 3.1.1 | 2,253/2,855 | 78.91% | 2,845 | 351.55 |
| Hunch public syntax | cinekey 0.2.0 | 611/909 | 67.22% | 1,305,216 | 0.77 |
| Hunch public syntax | Hunch 2.0.2 | 787/909 | 86.58% | 15,231 | 65.65 |
| Hunch public syntax | Oleoo 3.1.1 | 465/909 | 51.16% | 3,183 | 314.17 |
| Oleoo accepted releases | cinekey 0.2.0 | 6,185/6,672 | 92.70% | 1,240,380 | 0.81 |
| Oleoo accepted releases | Hunch 2.0.2 | 6,120/6,672 | 91.73% | 13,932 | 71.78 |
| Oleoo accepted releases | Oleoo 3.1.1 | 6,672/6,672 | 100.00% | 3,657 | 273.43 |

The Hunch input set contains 911 cases; 909 provide at least one shared identification expectation. All 911 inputs participate in timing. Oleoo’s 6,672 accepted records follow its own output conventions; 42 upstream known-error records have separate refused outputs. Its 100% here measures agreement with the accepted reference set. Public title scores also reflect punctuation and alternative-title policies.

Cinekey has the highest complete-record accuracy on the local collection. Hunch has the highest agreement on the selected public syntax fixtures. Oleoo has the highest agreement on its accepted release set. On local basenames, cinekey’s measured throughput is approximately 90 times Hunch’s and 358 times Oleoo’s.

Cinekey recognizes technical anchors that precede years, contextual release-language markers, bracketed language pairs, explicit episode sequences and ranges, movie installment years, numeric and Roman season words, calendar seasons, episode revision suffixes, and anime release prefixes. Parenthesized title annotations preserve accompanying names and qualifiers. Sole numeric movie titles retain their identity. Compound source and edition phrases delimit release metadata, and normalized technical fields recognize split codec spellings. Bonus-content boundaries separate title, year, and ordinal fields. Technical vocabulary uses bounded ASCII keys on the stack and static pattern dispatch. Tokens borrow the input and store UTF-8 spans, separators, and bracket state.

## Local collection

The local snapshot contains 2,855 basenames and their saved ancestor names: 2,579 files from `series` and 276 from `movies`. The reference answers cover every file and were reviewed from filename and directory semantics, including release groups, individual movie names, episode markers, and supplement labels. Explicit aliases and reviewed title variants apply equally to all parsers.

| Input tree | Files | cinekey basename | cinekey with directory context | Hunch | Oleoo |
| --- | ---: | ---: | ---: | ---: | ---: |
| Series, including supplements | 2,579 | 2,494 (96.70%) | 2,573 (99.77%) | 2,067 (80.15%) | 1,987 (77.05%) |
| Movies | 276 | 274 (99.28%) | 274 (99.28%) | 262 (94.93%) | 266 (96.38%) |
| Total | 2,855 | 2,768 (96.95%) | 2,847 (99.72%) | 2,329 (81.58%) | 2,253 (78.91%) |

Directory context supplies the saved ancestor names and movie/series input classification. Cinekey 0.2.1 reaches 2,847/2,855 complete records (99.72%) with this input and measures approximately 713,841 calls/s as the median of three separate 20-pass context replays. Hunch and Oleoo receive basename strings in the local comparison.

Cinekey’s contextual results match 2,312/2,312 episode records, 279/281 movie records, and 256/262 supplement records. Five movie records live in the series tree. The eight remaining mismatches consist of six supplements and two movie titles. Four supplement errors concern descriptive bonus titles; two concern sample-video classification. The movie errors concern two uploader-prefixed translated titles. Basename parsing has 26 episode titles that require ancestor context.

| Local field | cinekey basename | cinekey with context | Hunch | Oleoo |
| --- | ---: | ---: | ---: | ---: |
| title | 2,793/2,855 | 2,849/2,855 | 2,513/2,855 | 2,346/2,855 |
| year | 2,855/2,855 | 2,855/2,855 | 2,730/2,855 | 2,694/2,855 |
| season | 2,855/2,855 | 2,855/2,855 | 2,840/2,855 | 2,816/2,855 |
| episodes | 2,854/2,855 | 2,854/2,855 | 2,669/2,855 | 2,615/2,855 |
| kind | 2,807/2,855 | 2,853/2,855 | 2,499/2,855 | 2,360/2,855 |

## Public fixture results

### Hunch syntax fixtures

The pinned Hunch 2.0.2 package supplies `movies.yml` (200 cases), `episodes.yml` (489), `various.yml` (124), `rules/title.yml` (18), `rules/episodes.yml` (79), and `community.yml` (1). These six files exercise identification syntax; technical-property-only rule files are outside this comparison. The converter follows Hunch’s multi-key fixture loader, default-property merges, excluded option cases, and special-prefixed case rules. Duplicate input strings remain distinct fixture cases. The 911 converted cases match the upstream loader exactly.

The GuessIt-derived files carry [LGPL-3.0 attribution](https://github.com/lijunzh/hunch/blob/v2.0.2/tests/fixtures/ATTRIBUTION.md). Their varied title boundaries, compact numbering, episode ranges, legacy naming, and directory-derived values make them useful syntax checks. The corpus has 909 scored cases, of which 34 supply all five shared fields.

| Field | Scored cases | cinekey | Hunch | Oleoo |
| --- | ---: | ---: | ---: | ---: |
| title | 886 | 666 (75.17%) | 802 (90.52%) | 593 (66.93%) |
| year | 226 | 202 (89.38%) | 219 (96.90%) | 219 (96.90%) |
| season | 475 | 371 (78.11%) | 446 (93.89%) | 362 (76.21%) |
| episodes | 557 | 445 (79.89%) | 505 (90.66%) | 336 (60.32%) |
| kind | 824 | 694 (84.22%) | 772 (93.69%) | 650 (78.88%) |

Cinekey’s largest gaps in this set concern title boundaries, media kind, and episode/season syntax. Its 298 complete-record mismatches include 102 inputs containing directory components and 21 references containing multiple seasons; these groups overlap. Hunch’s broad syntax support transfers more successfully across these naming conventions. Current cinekey results also reflect its complete-title and season-identity policies. Oleoo returns one parse error on this set, included as a failed record.

### Oleoo release fixtures

The [accepted fixture snapshot](https://github.com/thcolin/oleoo/blob/7f03450/tests/fixtures/accepted.json), pinned to upstream commit `7f03450`, contains 6,672 records: 6,562 movies and 110 TV releases, with 69 nonempty episode lists. The surrounding fixture set has 6,818 input rows, 6,714 unique names, and 42 known-error records. The repository license is MIT.

The accepted set is especially useful for Scene/P2P movie syntax, French release vocabulary, language markers, and alternate titles. Its TV sample is small relative to the target local collection. Its primary title convention can remove accents, alter apostrophes, and select one title from a bilingual name. The shared scoring rule accepts Latin accent variations. Punctuation and alternative-title conventions participate in fixture matching.

| Field | Expected present | cinekey present match | Hunch present match | Oleoo present match | Expected empty | cinekey empty match | Hunch empty match | Oleoo empty match |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| title | 6,672 | 6,192 | 6,188 | 6,672 | 0 | 0 | 0 | 0 |
| year | 6,064 | 6,010 | 6,013 | 6,064 | 608 | 608 | 606 | 608 |
| season | 99 | 99 | 86 | 99 | 6,573 | 6,573 | 6,536 | 6,573 |
| episodes | 69 | 65 | 65 | 69 | 6,603 | 6,595 | 6,538 | 6,603 |
| kind | 6,672 | 6,657 | 6,586 | 6,672 | 0 | 0 | 0 | 0 |

Cinekey has 487 complete-record mismatches on this set; 480 involve the title field. Remaining title differences include punctuation conventions, complete spaced-dash titles, alternative-title choices, release vocabulary, and boundary errors. Cinekey and Hunch agree with 6,192 and 6,188 expected titles respectively. Oleoo defines the accepted reference convention.

## Measurement and reproduction

Each throughput measurement loads the input strings before timing, initializes and reuses Hunch’s `Pipeline` and Oleoo’s options, warms all three parsers with one full pass, and measures 20 passes in batches of 128. Parser order rotates between batches. Parser calls are sequential. File reading, directory traversal, parser initialization, output serialization, and result export are outside the timed region. Result allocations and destruction are included. Oleoo uses `current_year: Some(2026)`. The rates are single-run measurements and vary with machine load and compiler settings.

The cinekey-only replay measures three rounds of 256 passes after a warm-up. Two runs provide six timed rounds for each dataset. Cinekey 0.2.1 median throughputs are 950,190 calls/s on local basenames, 1,133,661 calls/s on Hunch syntax inputs, and 1,013,207 calls/s on Oleoo inputs. Native result construction and destruction are included. Peak resident memory is 4.02–4.69 MiB across these preloaded-basename runs and at most 5.17 MiB for the ancestor-context replays, measured with macOS `time -l`; this includes saved input strings and process overhead.

The evaluation package operates on saved JSONL corpora. Each input row has a string `name`, an optional `expected_kind` (`movie`, `series`, or `null`), and an `ancestors` array ordered nearest first. Context replay uses a movie or series kind for every row. The local inputs and references are private; upstream fixture sources supply the public inputs. Hunch and Oleoo belong to this separate development package.

```sh
just eval-build

# Local accuracy and speed from saved filenames
EVAL=target/release/cinekey-eval
REVIEW=tools/eval/local-review
mkdir -p "$REVIEW"
$EVAL results tools/eval/local-corpus.jsonl > "$REVIEW/outputs.jsonl"
$EVAL score local "$REVIEW/expectations.jsonl" \
  "$REVIEW/outputs.jsonl" --output-dir "$REVIEW"
$EVAL benchmark tools/eval/local-corpus.jsonl
$EVAL benchmark-cinekey tools/eval/local-corpus.jsonl
$EVAL replay tools/eval/local-corpus.jsonl

# Public fixture preparation: these paths point to the pinned upstream fixture sources
$EVAL prepare hunch "$HUNCH_FIXTURES" "$REVIEW/comparison/hunch.jsonl"
$EVAL prepare oleoo "$OLEOO_FIXTURES/accepted.json" "$REVIEW/comparison/oleoo.jsonl"

# Repeat for DATASET=hunch and DATASET=oleoo
DATASET=hunch
$EVAL results "$REVIEW/comparison/$DATASET.jsonl" \
  > "$REVIEW/comparison/$DATASET-results.jsonl"
$EVAL score public "$REVIEW/comparison/$DATASET.jsonl" \
  "$REVIEW/comparison/$DATASET-results.jsonl" \
  --output-dir "$REVIEW/comparison/$DATASET-score"
$EVAL benchmark "$REVIEW/comparison/$DATASET.jsonl"
```

The evaluation package uses Rust for fixture preparation, Unicode-aware scoring, and measurement. Result scoring streams parser output and reports `summary.json` and `mismatches.jsonl`. Local reference answers, result JSONL, per-file mismatches, timing JSON, input fingerprints, and independent score checks remain in `tools/eval/local-review/`. Public fixture data stays in this ignored directory. `just clean` reclaims the shared Rust build directory.

## Interpretation

The measured local collection is cinekey’s strongest domain: the contextual parser gets every episode identification record correct, has two movie mismatches, and has six supplement mismatches. The public measurements show substantial room for broader syntax compatibility, especially on the Hunch set. The local reference evaluates the collection used during development; held-out collections would provide additional evidence about transfer to other PT libraries.

The public sets are useful complementary references. Hunch emphasizes varied identification grammar, while Oleoo emphasizes movie release conventions. The target remains accurate, fast, memory-conscious parsing of the local PT naming mix. The API returns one identification and technical-metadata record for each input.

The comparison scope is the three measured native APIs and the listed identification fields. Their outputs carry different technical-property sets. The broader ecosystem includes [GuessIt](https://github.com/guessit-io/guessit), which covers movies and television episodes, and [Anitomy](https://github.com/erengy/anitomy), which focuses on anime. Industry-wide accuracy and speed rankings require comparable measurements across those libraries and independent inputs.
