use std::{collections::BTreeMap, fs, io::Write, path::Path};

use clap::ValueEnum;
use serde::{
    Deserializer,
    de::{DeserializeSeed, MapAccess, Visitor},
};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Source {
    Hunch,
    Oleoo,
}

pub fn run(source: Source, fixtures: &Path, output: &Path) -> Result<()> {
    if fixtures
        .canonicalize()
        .ok()
        .zip(output.canonicalize().ok())
        .is_some_and(|(input, output)| input == output)
    {
        return Err("fixture source and output must have distinct paths".into());
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut out = std::io::BufWriter::new(fs::File::create(output)?);
    let (count, scored) = match source {
        Source::Hunch => prepare_hunch(fixtures, &mut out)?,
        Source::Oleoo => {
            let mut input = serde_json::Deserializer::from_reader(std::io::BufReader::new(
                fs::File::open(fixtures)?,
            ));
            let count = OleooWriter(&mut out).deserialize(&mut input)?;
            input.end()?;
            (count, count)
        }
    };
    out.flush()?;
    println!("{source:?}: {count} cases; {scored} with shared expectations");
    Ok(())
}

fn write_row(out: &mut impl Write, row: &Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, row)?;
    out.write_all(b"\n")
}

fn strip_quotes(value: &str) -> String {
    let quoted = value.starts_with(['\'', '"']);
    let value = if quoted {
        value
    } else {
        value.split('#').next().unwrap_or("").trim()
    };
    if value.len() >= 2 && quoted && value.as_bytes().first() == value.as_bytes().last() {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

#[derive(Default)]
struct Group {
    keys: Vec<String>,
    props: BTreeMap<String, String>,
}

#[derive(Default)]
struct FixtureReader {
    group: Group,
    groups: Vec<Group>,
    items: Vec<String>,
    list_key: Option<String>,
    in_value: bool,
}

impl FixtureReader {
    fn flush_list(&mut self) {
        if let Some(key) = self.list_key.take() {
            let value = match self.items.as_slice() {
                [] => String::new(),
                [item] => item.clone(),
                _ => format!("[{}]", self.items.join(", ")),
            };
            self.group.props.insert(key, value);
        }
        self.items.clear();
    }

    fn property(&mut self, line: &str) {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            if !key.is_empty() {
                let value = strip_quotes(value.trim());
                if value.is_empty() {
                    self.list_key = Some(key.to_owned());
                } else {
                    self.group.props.insert(key.to_owned(), value);
                }
            }
        }
    }

    fn read(input: &str) -> Vec<Group> {
        let mut reader = Self::default();
        for line in input.lines() {
            let text = line.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            if let Some(key) = text.strip_prefix("? ") {
                if reader.in_value {
                    reader.flush_list();
                    reader.groups.push(std::mem::take(&mut reader.group));
                    reader.in_value = false;
                }
                reader.group.keys.push(strip_quotes(key.trim()));
            } else if let Some(property) = text.strip_prefix(": ") {
                reader.in_value = true;
                reader.flush_list();
                reader.property(property);
            } else if reader.in_value && line.starts_with([' ', '\t']) {
                if let Some(item) = text.strip_prefix("- ") {
                    reader.items.push(strip_quotes(item.trim()));
                } else {
                    reader.flush_list();
                    reader.property(text);
                }
            }
        }
        reader.flush_list();
        if !reader.group.keys.is_empty() {
            reader.groups.push(reader.group);
        }
        reader.groups
    }
}

fn fixture_value(value: &str, numeric: bool) -> Result<Value> {
    if matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "!!null" | "null"
    ) {
        return Ok(Value::Null);
    }
    let items = if value.starts_with('[') && value.ends_with(']') {
        value[1..value.len() - 1]
            .split(',')
            .filter(|item| !item.trim().is_empty())
            .map(|item| strip_quotes(item.trim()))
            .collect::<Vec<_>>()
    } else {
        vec![value.to_owned()]
    };
    let mut values = items
        .into_iter()
        .map(|item| {
            if numeric {
                Ok(Value::from(item.parse::<i64>()?))
            } else {
                Ok(Value::String(item))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(if values.len() == 1 {
        values.pop().unwrap()
    } else {
        Value::Array(values)
    })
}

fn prepare_hunch(root: &Path, out: &mut impl Write) -> Result<(usize, usize)> {
    let mut count = 0;
    let mut scored = 0;
    for filename in [
        "movies.yml",
        "episodes.yml",
        "various.yml",
        "rules/title.yml",
        "rules/episodes.yml",
        "community.yml",
    ] {
        let mut defaults = BTreeMap::new();
        for group in FixtureReader::read(&fs::read_to_string(root.join(filename))?) {
            if group.keys == ["__default__"] {
                defaults = group.props;
                continue;
            }
            let mut expected = defaults.clone();
            for (key, value) in group.props {
                if let Some(key) = key.strip_prefix('-') {
                    expected.remove(key);
                } else {
                    expected.insert(key, value);
                }
            }
            if expected.contains_key("options") {
                continue;
            }
            let mut shared = serde_json::Map::new();
            for (source, target) in [
                ("title", "title"),
                ("year", "year"),
                ("season", "season"),
                ("episode", "episodes"),
                ("type", "kind"),
            ] {
                if let Some(value) = expected.get(source) {
                    shared.insert(
                        target.to_owned(),
                        fixture_value(value, matches!(source, "year" | "season" | "episode"))?,
                    );
                }
            }
            for name in group.keys {
                if name.starts_with(['+', '-']) {
                    continue;
                }
                write_row(
                    out,
                    &json!({"name": name, "expected_kind": null, "ancestors": [], "expected": shared, "fixture": filename}),
                )?;
                count += 1;
                scored += usize::from(!shared.is_empty());
            }
        }
    }
    Ok((count, scored))
}

/// Visit accepted releases in source order and emit each row incrementally.
struct OleooWriter<'a, W>(&'a mut W);

impl<'de, W: Write> DeserializeSeed<'de> for OleooWriter<'_, W> {
    type Value = usize;
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<usize, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de, W: Write> Visitor<'de> for OleooWriter<'_, W> {
    type Value = usize;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an ordered map of accepted release names and properties")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut input: M) -> std::result::Result<usize, M::Error> {
        let mut count = 0;
        while let Some((name, expected)) = input.next_entry::<String, Value>()? {
            let episodes = match &expected["episodes"] {
                Value::Array(values) if values.is_empty() => Value::Null,
                value => value.clone(),
            };
            let kind = if expected["type"] == "tvshow" {
                json!("episode")
            } else {
                expected["type"].clone()
            };
            write_row(self.0, &json!({"name": name, "expected_kind": null, "ancestors": [], "expected": {"title": expected["title"], "year": expected["year"], "season": expected["season"], "episodes": episodes, "kind": kind}})).map_err(serde::de::Error::custom)?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_key_fixture_groups_preserve_lists_and_defaults() {
        let groups = FixtureReader::read(
            "? __default__\n: type: episode\n? 'First Name'\n? Second Name\n: episode:\n  - 2\n  - 3\n  -type: !!null\n",
        );
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].props["type"], "episode");
        assert_eq!(groups[1].keys, ["First Name", "Second Name"]);
        assert_eq!(groups[1].props["episode"], "[2, 3]");
        assert_eq!(groups[1].props["-type"], "!!null");
    }

    #[test]
    fn fixture_values_preserve_scalar_list_null_and_quoted_comment_semantics() {
        assert_eq!(fixture_value("[2, 3]", true).unwrap(), json!([2, 3]));
        assert_eq!(fixture_value("[2]", true).unwrap(), json!(2));
        assert_eq!(fixture_value("[]", true).unwrap(), json!([]));
        assert_eq!(fixture_value("!!null", false).unwrap(), Value::Null);
        assert_eq!(strip_quotes("Name # comment"), "Name");
        assert_eq!(strip_quotes("'Name # Title'"), "Name # Title");
    }

    #[test]
    fn accepted_release_export_preserves_source_order() {
        let input = r#"{"Z Title": {"title":"Z","year":"2020","season":null,"episodes":[],"type":"movie"},"A Title":{"title":"A","year":null,"season":1,"episodes":[2],"type":"tvshow"}}"#;
        let mut output = Vec::new();
        let mut reader = serde_json::Deserializer::from_str(input);
        assert_eq!(
            OleooWriter(&mut output).deserialize(&mut reader).unwrap(),
            2
        );
        let rows = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows[0]["name"], "Z Title");
        assert_eq!(rows[0]["expected"]["episodes"], Value::Null);
        assert_eq!(rows[1]["name"], "A Title");
        assert_eq!(rows[1]["expected"]["kind"], "episode");
    }
}
