use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
};

use caseless::Caseless;
use clap::ValueEnum;
use serde_json::{Value, json};
use unicode_normalization::{UnicodeNormalization, char::canonical_combining_class};
use unicode_script::{Script, UnicodeScript};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FIELDS: [&str; 5] = ["title", "year", "season", "episodes", "kind"];
const PARSERS: [&str; 4] = ["cinekey", "contextual", "hunch", "oleoo"];

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Scope {
    Local,
    Public,
}

#[derive(Default)]
struct Counts {
    count: usize,
    full: usize,
    fields: [usize; 5],
    tested: [usize; 5],
    present: [usize; 5],
    positive: [usize; 5],
    empty: [usize; 5],
}

impl Counts {
    fn record(&mut self, expected: &Value, correct: &[bool; 5]) {
        self.count += 1;
        self.full += usize::from(correct.iter().all(|matched| *matched));
        for (index, field) in FIELDS.iter().enumerate() {
            if expected.get(field).is_none() {
                continue;
            }
            let present = !values(&expected[field]).is_empty();
            self.tested[index] += 1;
            self.fields[index] += usize::from(correct[index]);
            self.present[index] += usize::from(present);
            if correct[index] {
                if present {
                    self.positive[index] += 1;
                } else {
                    self.empty[index] += 1;
                }
            }
        }
    }
    fn local(&self) -> Value {
        json!({"count": self.count, "full": self.full, "fields": field_map(&self.fields, false), "expected_present": field_map(&self.present, self.count > 0), "present_matches": field_map(&self.positive, false), "empty_matches": field_map(&self.empty, false)})
    }
    fn public(&self, parse_errors: usize) -> Value {
        json!({"count": self.count, "full": self.full, "fields": field_map(&self.fields, false), "tested": field_map(&self.tested, false), "expected_present": field_map(&self.present, false), "present_matches": field_map(&self.positive, false), "parse_errors": parse_errors})
    }
}

fn field_map(counts: &[usize; 5], include_zero: bool) -> Value {
    FIELDS
        .iter()
        .zip(counts)
        .filter(|(_, count)| include_zero || **count > 0)
        .map(|(field, count)| ((*field).to_owned(), json!(count)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn values(value: &Value) -> Vec<&Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().collect(),
        value => vec![value],
    }
}

fn normalized_text(text: &str, title: bool) -> String {
    let text = if title {
        let mut latin = false;
        text.nfd()
            .filter(|character| {
                if canonical_combining_class(*character) != 0 {
                    !latin
                } else {
                    latin = character.script() == Script::Latin;
                    true
                }
            })
            .nfc()
            .collect::<String>()
    } else {
        text.nfc().collect()
    };
    text.chars()
        .default_case_fold()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normal(value: &Value, field: &str) -> Vec<String> {
    let mut result = values(value)
        .into_iter()
        .map(|item| {
            normalized_text(
                &item
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| item.to_string()),
                field == "title",
            )
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn local_match(actual: &Value, expected: &Value, field: &str, aliases: &Value) -> bool {
    let equivalent = if field == "episodes" {
        let mut actual = values(actual);
        let mut expected = values(expected);
        actual.sort_by_cached_key(|value| value.to_string());
        expected.sort_by_cached_key(|value| value.to_string());
        actual.dedup();
        expected.dedup();
        actual == expected
    } else if let (Some(actual), Some(expected)) = (actual.as_str(), expected.as_str()) {
        normalized_text(actual, field == "title") == normalized_text(expected, field == "title")
    } else {
        actual == expected
    };
    equivalent
        || (field == "title"
            && aliases.as_array().is_some_and(|aliases| {
                aliases
                    .iter()
                    .any(|alias| match (actual.as_str(), alias.as_str()) {
                        (Some(actual), Some(alias)) => {
                            normalized_text(actual, true) == normalized_text(alias, true)
                        }
                        _ => false,
                    })
            }))
}

fn actual_fields(row: &Value, parser: &str, scope: Scope) -> Result<Value> {
    let data = &row[parser];
    if matches!(parser, "cinekey" | "contextual") {
        let candidate = data;
        let episodes = match scope {
            Scope::Local => {
                let mut episodes = candidate["episodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(
                        candidate["absolute_episodes"]
                            .as_array()
                            .into_iter()
                            .flatten(),
                    )
                    .map(|number| number.as_u64().ok_or("expected unsigned episode number"))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                episodes.sort_unstable();
                episodes.dedup();
                if episodes.is_empty() {
                    Value::Null
                } else {
                    json!(episodes)
                }
            }
            Scope::Public => {
                if truthy(&candidate["episodes"]) {
                    candidate["episodes"].clone()
                } else if truthy(&candidate["absolute_episodes"]) {
                    candidate["absolute_episodes"].clone()
                } else {
                    Value::Null
                }
            }
        };
        return Ok(
            json!({"title": candidate["title"], "year": candidate["year"], "season": candidate["season"], "episodes": episodes, "kind": if candidate["kind"] == "season_pack" { json!("episode") } else { candidate["kind"].clone() }}),
        );
    }
    let mut episodes = data[if parser == "hunch" {
        "episode"
    } else {
        "episodes"
    }]
    .clone();
    if matches!(scope, Scope::Local) {
        if parser == "hunch" && !episodes.is_null() && !episodes.is_array() {
            episodes = json!([episodes]);
        } else if parser == "oleoo" && !truthy(&episodes) {
            episodes = Value::Null;
        }
    }
    let year = if matches!(scope, Scope::Local) && parser == "oleoo" {
        match &data["year"] {
            Value::String(value) => json!(value.parse::<i64>()?),
            value => value.clone(),
        }
    } else {
        data["year"].clone()
    };
    let kind = if data["type"] == "tvshow" && (parser == "oleoo" || matches!(scope, Scope::Public))
    {
        json!("episode")
    } else {
        data["type"].clone()
    };
    Ok(
        json!({"title": data["title"], "year": year, "season": data["season"], "episodes": episodes, "kind": kind}),
    )
}

fn rows(path: &Path) -> Result<impl Iterator<Item = Result<Value>>> {
    let path = path.to_owned();
    Ok(BufReader::new(fs::File::open(&path)?)
        .lines()
        .enumerate()
        .map(move |(index, line)| {
            serde_json::from_str(&line?)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1).into())
        }))
}

pub fn run(scope: Scope, expectations: &Path, results: &Path, output_dir: &Path) -> Result<()> {
    let labels = rows(expectations)?.collect::<Result<Vec<_>>>()?;
    let (summary, mismatches) = match scope {
        Scope::Local => score_local(&labels, rows(results)?)?,
        Scope::Public => score_public(&labels, rows(results)?)?,
    };
    fs::create_dir_all(output_dir)?;
    let mut output = std::io::BufWriter::new(fs::File::create(output_dir.join("summary.json"))?);
    serde_json::to_writer_pretty(&mut output, &summary)?;
    output.write_all(b"\n")?;
    output.flush()?;
    let mut output =
        std::io::BufWriter::new(fs::File::create(output_dir.join("mismatches.jsonl"))?);
    for mismatch in mismatches {
        serde_json::to_writer(&mut output, &mismatch)?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    let scores = if matches!(scope, Scope::Local) {
        &summary["scores"]["all"]
    } else {
        &summary["scores"]
    };
    for parser in PARSERS {
        if let Some(score) = scores.get(parser) {
            let full = score["full"].as_u64().unwrap_or(0);
            let count = score["count"].as_u64().unwrap_or(0);
            println!(
                "{parser}: {full}/{count} ({:.2}%)",
                if count == 0 {
                    0.0
                } else {
                    100.0 * full as f64 / count as f64
                }
            );
        }
    }
    Ok(())
}

fn score_local(
    labels: &[Value],
    outputs: impl Iterator<Item = Result<Value>>,
) -> Result<(Value, Vec<Value>)> {
    let mut names = HashMap::new();
    let mut unresolved = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        let name = label["name"].as_str().ok_or("expected string name")?;
        if names.insert(name, index).is_some() {
            return Err("duplicate expectation names".into());
        }
        if truthy(&label["unresolved"]) {
            unresolved.push(json!({"name": name, "reason": label["unresolved"]}));
        } else if FIELDS
            .iter()
            .any(|field| label["expected"].get(*field).is_none())
        {
            return Err(
                format!("{name}: local reference requires all five identification fields").into(),
            );
        }
    }
    let mut counts: [[Counts; 4]; 3] =
        std::array::from_fn(|_| std::array::from_fn(|_| Counts::default()));
    let mut seen = HashSet::new();
    let mut mismatches = Vec::new();
    for row in outputs {
        let row = row?;
        let name = row["name"].as_str().ok_or("expected string result name")?;
        let index = *names.get(name).ok_or("reference/result names differ")?;
        if !seen.insert(index) {
            return Err("duplicate result names".into());
        }
        let label = &labels[index];
        if truthy(&label["unresolved"]) {
            continue;
        }
        let expected = &label["expected"];
        for (parser_index, parser) in PARSERS.iter().enumerate() {
            let actual = actual_fields(&row, parser, Scope::Local)?;
            let correct = std::array::from_fn(|field| {
                local_match(
                    &actual[FIELDS[field]],
                    &expected[FIELDS[field]],
                    FIELDS[field],
                    &label["acceptable"]["title"],
                )
            });
            counts[0][parser_index].record(expected, &correct);
            match label["expected_kind"].as_str() {
                Some("series") => counts[1][parser_index].record(expected, &correct),
                Some("movie") => counts[2][parser_index].record(expected, &correct),
                _ => {}
            }
            if !correct.iter().all(|matched| *matched) {
                mismatches.push((parser_index, index, json!({"name": name, "parser": parser, "expected": expected, "actual": actual, "wrong": FIELDS.iter().zip(correct).filter_map(|(field, correct)| (!correct).then_some(field)).collect::<Vec<_>>(), "source": label["review_source"]})));
            }
        }
    }
    if seen.len() != labels.len() {
        return Err("reference/result names differ".into());
    }
    mismatches.sort_unstable_by_key(|(parser, label, _)| (*parser, *label));
    let scores: serde_json::Map<_, _> = ["all", "series", "movie"]
        .into_iter()
        .zip(counts)
        .map(|(part, parsers)| {
            (
                part.to_owned(),
                PARSERS
                    .into_iter()
                    .zip(parsers)
                    .map(|(parser, counts)| (parser.to_owned(), counts.local()))
                    .collect::<serde_json::Map<_, _>>()
                    .into(),
            )
        })
        .collect();
    Ok((
        json!({"total": labels.len(), "unresolved": unresolved, "scores": scores}),
        mismatches.into_iter().map(|(_, _, value)| value).collect(),
    ))
}

fn score_public(
    labels: &[Value],
    outputs: impl Iterator<Item = Result<Value>>,
) -> Result<(Value, Vec<Value>)> {
    let mut counts: [Counts; 3] = std::array::from_fn(|_| Counts::default());
    let parsers = ["cinekey", "hunch", "oleoo"];
    let mut parse_errors = 0;
    let mut mismatches = Vec::new();
    let mut outputs = outputs;
    for (index, label) in labels.iter().enumerate() {
        let row = outputs
            .next()
            .ok_or("reference/result row counts differ")??;
        if label["name"] != row["name"] {
            return Err("reference/result row order differs".into());
        }
        parse_errors += usize::from(truthy(&row["oleoo_error"]));
        let expected = &label["expected"];
        if !truthy(expected) {
            continue;
        }
        for (parser_index, parser) in parsers.iter().enumerate() {
            let actual = actual_fields(&row, parser, Scope::Public)?;
            let correct = std::array::from_fn(|field| {
                expected.get(FIELDS[field]).is_none_or(|value| {
                    normal(&actual[FIELDS[field]], FIELDS[field]) == normal(value, FIELDS[field])
                })
            });
            counts[parser_index].record(expected, &correct);
            if !correct.iter().all(|matched| *matched) {
                mismatches.push((parser_index, index, json!({"name": label["name"], "parser": parser, "fixture": label["fixture"], "expected": expected, "actual": actual, "wrong": FIELDS.iter().zip(correct).filter_map(|(field, correct)| (!correct).then_some(field)).collect::<Vec<_>>()})));
            }
        }
    }
    if outputs.next().transpose()?.is_some() {
        return Err("reference/result row counts differ".into());
    }
    mismatches.sort_unstable_by_key(|(parser, label, _)| (*parser, *label));
    let scores: serde_json::Map<_, _> = parsers
        .into_iter()
        .zip(counts)
        .map(|(parser, counts)| {
            (
                parser.to_owned(),
                counts.public(if parser == "oleoo" { parse_errors } else { 0 }),
            )
        })
        .collect();
    Ok((
        json!({"input_rows": labels.len(), "scores": scores}),
        mismatches.into_iter().map(|(_, _, value)| value).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_aliases_apply_to_title() {
        let aliases = json!(["bilingual title"]);
        assert!(local_match(
            &json!("English Title"),
            &json!("english title"),
            "title",
            &aliases
        ));
        assert!(local_match(
            &json!("bilingual title"),
            &json!("english title"),
            "title",
            &aliases
        ));
        assert!(!local_match(
            &json!("wrong title"),
            &json!("english title"),
            "title",
            &aliases
        ));
        assert!(!local_match(
            &json!("bilingual title"),
            &json!(2024),
            "year",
            &aliases
        ));
    }

    #[test]
    fn empty_expectations_are_scored() {
        assert!(local_match(&Value::Null, &Value::Null, "year", &json!([])));
        assert!(local_match(
            &json!([]),
            &Value::Null,
            "episodes",
            &json!([])
        ));
        assert!(!local_match(&json!(2024), &Value::Null, "year", &json!([])));
        assert!(!local_match(
            &Value::Null,
            &json!("Title"),
            "title",
            &json!([])
        ));
    }

    #[test]
    fn episode_lists_compare_complete_values() {
        assert!(!local_match(
            &json!(["2"]),
            &json!([2]),
            "episodes",
            &json!([])
        ));
        assert!(local_match(
            &json!([3, 2, 2]),
            &json!([2, 3]),
            "episodes",
            &json!([])
        ));
        assert!(!local_match(
            &json!([2]),
            &json!([2, 3]),
            "episodes",
            &json!([])
        ));
        assert_eq!(
            normal(&json!([2, 3]), "episodes"),
            normal(&json!([3, 2]), "episodes")
        );
        assert_ne!(
            normal(&json!([2, 2]), "episodes"),
            normal(&json!([2]), "episodes")
        );
    }

    #[test]
    fn title_equivalence_preserves_unicode_script_and_punctuation() {
        for (left, right) in [
            ("Amélie", "Amelie"),
            ("Ame\u{301}lie", "Amelie"),
            ("Straße", "STRASSE"),
            ("ΟΣ", "οσ"),
        ] {
            assert_eq!(
                normal(&json!(left), "title"),
                normal(&json!(right), "title")
            );
        }
        assert_eq!(normalized_text("が", true), "が");
        assert_ne!(normal(&json!("が"), "title"), normal(&json!("か"), "title"));
        assert_ne!(
            normal(&json!("L'homme"), "title"),
            normal(&json!("L homme"), "title")
        );
    }

    #[test]
    fn shared_projection_preserves_season_pack_and_absolute_numbering() {
        let row =
            json!({"cinekey": {"kind": "season_pack", "episodes": [], "absolute_episodes": []}});
        let actual = actual_fields(&row, "cinekey", Scope::Local).unwrap();
        assert_eq!(actual["kind"], "episode");
        assert_eq!(actual["episodes"], Value::Null);
        let row =
            json!({"cinekey": {"kind": "episode", "episodes": [], "absolute_episodes": [12]}});
        assert_eq!(
            actual_fields(&row, "cinekey", Scope::Public).unwrap()["episodes"],
            json!([12])
        );
    }

    #[test]
    fn public_scoring_uses_supplied_fields_and_counts_parse_errors() {
        let labels = vec![
            json!({"name": "A", "expected": {"title": "A"}}),
            json!({"name": "B", "expected": {}}),
        ];
        let outputs = vec![
            Ok(json!({"name": "A", "cinekey": {"title": "A", "year": 2024}})),
            Ok(json!({"name": "B", "oleoo_error": "parse error"})),
        ];
        let (summary, _) = score_public(&labels, outputs.into_iter()).unwrap();
        assert_eq!(summary["scores"]["cinekey"]["full"], 1);
        assert_eq!(summary["scores"]["cinekey"]["tested"], json!({"title": 1}));
        assert_eq!(summary["scores"]["oleoo"]["parse_errors"], 1);
    }

    #[test]
    fn public_scoring_validates_row_alignment() {
        let labels = vec![json!({"name": "A", "expected": {}})];
        assert!(score_public(&labels, std::iter::empty()).is_err());
        assert!(score_public(&labels, [Ok(json!({"name":"B"}))].into_iter()).is_err());
        assert!(
            score_public(
                &labels,
                [Ok(json!({"name":"A"})), Ok(json!({"name":"B"}))].into_iter()
            )
            .is_err()
        );
    }

    #[test]
    fn local_scoring_validates_unique_names_and_complete_references() {
        let label = json!({"name": "A", "expected_kind": "series", "expected": {"title": null,"year": null,"season": null,"episodes": null,"kind": null}});
        let row = json!({"name": "A"});
        let (summary, _) =
            score_local(std::slice::from_ref(&label), [Ok(row.clone())].into_iter()).unwrap();
        assert_eq!(summary["scores"]["all"]["cinekey"]["full"], 1);
        assert_eq!(summary["scores"]["movie"]["cinekey"]["count"], 0);
        assert!(score_local(&[label.clone(), label.clone()], std::iter::empty()).is_err());
        assert!(
            score_local(
                std::slice::from_ref(&label),
                [Ok(row.clone()), Ok(row)].into_iter()
            )
            .is_err()
        );
        assert!(score_local(&[label], std::iter::empty()).is_err());
        assert!(score_local(&[json!({"name":"A", "expected":{}})], std::iter::empty()).is_err());
    }
}
