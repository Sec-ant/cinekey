"""Score primary parser results against a complete local identification reference."""

import argparse
import json
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path

from scoring import fold_title_accents

FIELDS = ("title", "year", "season", "episodes", "kind")
PARSERS = ("cinekey", "contextual", "hunch", "oleoo")


def norm(value, field=None):
    if isinstance(value, str):
        value = (
            fold_title_accents(value)
            if field == "title"
            else unicodedata.normalize("NFC", value)
        )
        return " ".join(value.casefold().split())
    return value


def candidate_fields(candidate):
    return {
        **{field: candidate.get(field) for field in FIELDS},
        "episodes": sorted(set(
            candidate.get("episodes", []) + candidate.get("absolute_episodes", [])
        )) or None,
        "kind": (
            "episode" if candidate.get("kind") == "season_pack"
            else candidate.get("kind")
        ),
    }


def external_fields(row, parser):
    data = row.get(parser) or {}
    if parser == "hunch":
        episodes = data.get("episode")
        if not isinstance(episodes, list):
            episodes = [episodes] if episodes is not None else None
        return {
            "title": data.get("title"),
            "year": data.get("year"),
            "season": data.get("season"),
            "episodes": episodes,
            "kind": data.get("type"),
        }
    return {
        "title": data.get("title"),
        "year": int(data["year"]) if data.get("year") is not None else None,
        "season": data.get("season"),
        "episodes": data.get("episodes") or None,
        "kind": {"tvshow": "episode"}.get(data.get("type"), data.get("type")),
    }


def match(actual, expected, field, aliases):
    actual, expected = norm(actual, field), norm(expected, field)
    if field == "episodes":
        actual, expected = sorted(set(actual or [])), sorted(set(expected or []))
    return actual == expected or (
        field == "title" and actual in [norm(alias, field) for alias in aliases]
    )


def score(labels, outputs):
    stats, mismatches = {}, []
    for part in ("all", "series", "movie"):
        subset = [
            label for label in labels
            if not label["unresolved"]
            and (part == "all" or label["expected_kind"] == part)
        ]
        by_parser = {}
        for parser in PARSERS:
            counts, presence, positive, negative = (
                Counter(), Counter(), Counter(), Counter()
            )
            full = 0
            for label in subset:
                row, expected = outputs[label["name"]], label["expected"]
                aliases = label["acceptable"]["title"]
                actual = (
                    candidate_fields(row[parser]["candidates"][0])
                    if parser in ("cinekey", "contextual")
                    else external_fields(row, parser)
                )
                correct = {
                    field: match(actual.get(field), expected[field], field, aliases)
                    for field in FIELDS
                }
                counts.update(field for field in FIELDS if correct[field])
                full += all(correct.values())
                for field in FIELDS:
                    is_present = expected[field] is not None and expected[field] != []
                    presence[field] += is_present
                    if correct[field]:
                        (positive if is_present else negative)[field] += 1
                if part == "all" and not all(correct.values()):
                    mismatches.append({
                        "name": label["name"], "parser": parser,
                        "expected": expected, "actual": actual,
                        "wrong": [field for field in FIELDS if not correct[field]],
                        "source": label["review_source"],
                    })
            by_parser[parser] = {
                "count": len(subset), "full": full, "fields": dict(counts),
                "expected_present": dict(presence),
                "present_matches": dict(positive), "empty_matches": dict(negative),
            }
        stats[part] = by_parser
    return stats, mismatches


def print_context_groups(mismatches):
    print("Contextual disagreement groups:")
    groups = defaultdict(list)
    for row in mismatches:
        if row["parser"] == "contextual":
            key = (
                tuple(row["wrong"]),
                tuple((field, json.dumps(row["expected"][field], ensure_ascii=False))
                      for field in row["wrong"]),
                tuple((field, json.dumps(row["actual"][field], ensure_ascii=False))
                      for field in row["wrong"]),
            )
            groups[key].append(row["name"])
    for key, names in sorted(groups.items(), key=lambda item: -len(item[1])):
        print(len(names), key, "EXAMPLE", names[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expectations", type=Path)
    parser.add_argument("results", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    labels = [json.loads(line) for line in args.expectations.read_text().splitlines()]
    raw_outputs = [json.loads(line) for line in args.results.read_text().splitlines()]
    outputs = {row["name"]: row for row in raw_outputs}
    assert len(outputs) == len(raw_outputs), "duplicate result names"
    assert len({row["name"] for row in labels}) == len(labels), "duplicate expectation names"
    assert {row["name"] for row in labels} == set(outputs), "reference/result names differ"
    stats, mismatches = score(labels, outputs)
    report = {
        "total": len(labels),
        "unresolved": [
            {"name": label["name"], "reason": label["unresolved"]}
            for label in labels if label["unresolved"]
        ],
        "scores": stats,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "summary.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    )
    (args.output_dir / "mismatches.jsonl").write_text(
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in mismatches)
    )
    for part, parsers in stats.items():
        print(part)
        for name, result in parsers.items():
            print(name, "full", result["full"], "/", result["count"],
                  "fields", result["fields"])
    print_context_groups(mismatches)


if __name__ == "__main__":
    main()
