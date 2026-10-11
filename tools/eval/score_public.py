"""Score each parser's primary result against provided shared fixture fields."""

import argparse
import json
import unicodedata
from collections import Counter
from pathlib import Path

from scoring import fold_title_accents

FIELDS = ("title", "year", "season", "episodes", "kind")
PARSERS = ("cinekey", "hunch", "oleoo")


def values(value):
    if value is None or value == []:
        return []
    return value if isinstance(value, list) else [value]


def normal(value, field=None):
    items = (str(item) for item in values(value))
    if field == "title":
        items = (fold_title_accents(item) for item in items)
    return sorted(
        " ".join(unicodedata.normalize("NFC", item).casefold().split())
        for item in items
    )


def actual_fields(row, parser):
    data = row.get(parser) or {}
    if parser == "cinekey":
        candidates = data.get("candidates", [])
        data = candidates[0] if candidates else {}
        return {
            **{field: data.get(field) for field in FIELDS},
            "episodes": data.get("episodes") or data.get("absolute_episodes") or None,
            "kind": "episode" if data.get("kind") == "season_pack" else data.get("kind"),
        }
    return {
        "title": data.get("title"), "year": data.get("year"), "season": data.get("season"),
        "episodes": data.get("episode") if parser == "hunch" else data.get("episodes"),
        "kind": "episode" if data.get("type") == "tvshow" else data.get("type"),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expectations", type=Path)
    parser.add_argument("results", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    labels = [json.loads(line) for line in args.expectations.read_text().splitlines()]
    outputs = [json.loads(line) for line in args.results.read_text().splitlines()]
    assert len(labels) == len(outputs), "reference/result row counts differ"
    assert all(label["name"] == result["name"] for label, result in zip(labels, outputs)), "reference/result row order differs"
    scores, mismatches = {}, []
    for name in PARSERS:
        correct_count, tested_count, positive_count, positive_correct = Counter(), Counter(), Counter(), Counter()
        full, count = 0, 0
        for label, result in zip(labels, outputs):
            expected = label["expected"]
            if not expected:
                continue
            count += 1
            actual = actual_fields(result, name)
            correct = {field: normal(actual.get(field), field) == normal(value, field) for field, value in expected.items()}
            tested_count.update(expected.keys())
            correct_count.update(field for field, matched in correct.items() if matched)
            positive_count.update(field for field, value in expected.items() if values(value))
            positive_correct.update(field for field, matched in correct.items() if matched and values(expected[field]))
            full += all(correct.values())
            if not all(correct.values()):
                mismatches.append({"name": label["name"], "parser": name, "fixture": label.get("fixture"), "expected": expected, "actual": actual, "wrong": [field for field, matched in correct.items() if not matched]})
        scores[name] = {"count": count, "full": full, "fields": dict(correct_count), "tested": dict(tested_count), "expected_present": dict(positive_count), "present_matches": dict(positive_correct), "parse_errors": sum(bool(row.get("oleoo_error")) for row in outputs) if name == "oleoo" else 0}
        print(f"{name}: {full}/{count} ({100 * full / count:.2f}%)")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "summary.json").write_text(json.dumps({"input_rows": len(labels), "scores": scores}, ensure_ascii=False, indent=2) + "\n")
    (args.output_dir / "mismatches.jsonl").write_text("".join(json.dumps(row, ensure_ascii=False) + "\n" for row in mismatches))


if __name__ == "__main__":
    main()
