"""Identification scoring preserves complete results and shared equivalence rules."""

import unittest

from score_local import candidate_fields, match
from score_public import normal
from scoring import fold_title_accents


class ScoringTests(unittest.TestCase):
    def test_title_aliases_apply_to_title(self):
        aliases = ["bilingual title"]
        self.assertTrue(match("English Title", "english title", "title", aliases))
        self.assertTrue(match("bilingual title", "english title", "title", aliases))
        self.assertFalse(match("wrong title", "english title", "title", aliases))
        self.assertFalse(match("bilingual title", 2024, "year", aliases))

    def test_empty_expectations_are_scored(self):
        self.assertTrue(match(None, None, "year", []))
        self.assertTrue(match([], None, "episodes", []))
        self.assertFalse(match(2024, None, "year", []))
        self.assertFalse(match(None, "Title", "title", []))

    def test_episode_lists_compare_complete_values(self):
        self.assertTrue(match([3, 2, 2], [2, 3], "episodes", []))
        self.assertFalse(match([2], [2, 3], "episodes", []))
        self.assertEqual(normal([2, 3], "episodes"), normal([3, 2], "episodes"))

    def test_latin_accents_and_unicode_normalization_are_equivalent(self):
        self.assertTrue(match("Amélie", "Amelie", "title", []))
        self.assertEqual(normal("Ame\u0301lie", "title"), normal("Amelie", "title"))
        self.assertEqual(fold_title_accents("が"), "が")

    def test_title_punctuation_participates_in_matching(self):
        self.assertFalse(match("L'homme", "L homme", "title", []))
        self.assertNotEqual(normal("L'homme", "title"), normal("L homme", "title"))

    def test_season_packs_use_episode_kind_for_shared_scoring(self):
        fields = candidate_fields({"kind": "season_pack", "episodes": [], "absolute_episodes": []})
        self.assertEqual(fields["kind"], "episode")
        self.assertIsNone(fields["episodes"])


if __name__ == "__main__":
    unittest.main()
