"""Prepare shared identification expectations from pinned upstream fixtures."""

import argparse
import json
from pathlib import Path


def strip_quotes(value):
    if not value.startswith(('"', "'")):
        value = value.split("#", 1)[0].strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        return value[1:-1]
    return value


def hunch_groups(path):
    """Read the multi-key fixture format using Hunch's loader conventions."""
    groups = []
    keys, props, items = [], {}, []
    in_value, list_key = False, None

    def flush_list():
        nonlocal list_key, items
        if list_key is not None:
            props[list_key] = (
                items[0] if len(items) == 1 else f"[{', '.join(items)}]" if items else ""
            )
        list_key, items = None, []

    def prop_line(line):
        nonlocal list_key
        if ":" in line:
            key, value = line.split(":", 1)
            key, value = key.strip(), strip_quotes(value.strip())
            if key:
                if value:
                    props[key] = value
                else:
                    list_key = key

    for line in path.read_text().splitlines():
        text = line.strip()
        if not text or text.startswith("#"):
            continue
        if text.startswith("? "):
            if in_value:
                flush_list()
                groups.append((keys, props))
                keys, props, in_value = [], {}, False
            keys.append(strip_quotes(text[2:].strip()))
        elif text.startswith(": "):
            in_value = True
            flush_list()
            prop_line(text[2:])
        elif in_value and line.startswith((" ", "\t")):
            if text.startswith("- "):
                items.append(strip_quotes(text[2:].strip()))
            else:
                flush_list()
                prop_line(text)
    flush_list()
    if keys:
        groups.append((keys, props))
    return groups


def fixture_value(value, numeric=False):
    if value.strip().lower() in ("!!null", "null"):
        return None
    values = (
        [strip_quotes(x.strip()) for x in value[1:-1].split(",") if x.strip()]
        if value.startswith("[") and value.endswith("]")
        else [value]
    )
    values = [int(x) if numeric else x for x in values]
    return values[0] if len(values) == 1 else values


def hunch_rows(root):
    files = ["movies.yml", "episodes.yml", "various.yml", "rules/title.yml", "rules/episodes.yml", "community.yml"]
    for filename in files:
        defaults = {}
        for keys, props in hunch_groups(root / filename):
            if keys == ["__default__"]:
                defaults = props.copy()
                continue
            expected = defaults.copy()
            for key, value in props.items():
                if key.startswith("-"):
                    expected.pop(key[1:], None)
                else:
                    expected[key] = value
            if "options" in expected:
                continue
            shared = {}
            for source, target in [("title", "title"), ("year", "year"), ("season", "season"), ("episode", "episodes"), ("type", "kind")]:
                if source in expected:
                    shared[target] = fixture_value(expected[source], source in ("year", "season", "episode"))
            for name in keys:
                if name.startswith(("+", "-")):
                    continue
                yield {"name": name, "expected_kind": None, "ancestors": [], "expected": shared, "fixture": filename}


def oleoo_rows(path):
    for name, expected in json.loads(path.read_text()).items():
        yield {
            "name": name, "expected_kind": None, "ancestors": [],
            "expected": {
                "title": expected["title"],
                "year": expected["year"],
                "season": expected["season"],
                "episodes": expected["episodes"] or None,
                "kind": "episode" if expected["type"] == "tvshow" else expected["type"],
            },
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", choices=["hunch", "oleoo"])
    parser.add_argument("fixtures", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    rows = list(hunch_rows(args.fixtures) if args.source == "hunch" else oleoo_rows(args.fixtures))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows))
    print(f"{args.source}: {len(rows)} cases; {sum(bool(row['expected']) for row in rows)} with shared expectations")


if __name__ == "__main__":
    main()
