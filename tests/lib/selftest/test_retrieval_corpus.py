import tempfile
import unittest
from pathlib import Path

from wamtest.retrieval_corpus import (
    build_corpus,
    evaluate_results,
    load_corpus,
    validate_corpus,
    write_corpus,
)


class RetrievalCorpusTest(unittest.TestCase):
    def test_checked_in_corpus_matches_the_default_generator(self):
        root = Path(__file__).resolve().parents[3]
        directory = root / "tests" / "fixtures" / "retrieval"
        corpus = load_corpus(directory)
        generated = build_corpus(corpus["manifest"]["seed"])
        for name, rows in generated.items():
            self.assertEqual(corpus[name], rows)

    def test_generator_creates_500_queries_with_a_grouped_20_percent_holdout(self):
        corpus = build_corpus()
        self.assertEqual(len(corpus["records"]), 600)
        self.assertEqual(len(corpus["queries"]), 500)
        self.assertEqual(len(corpus["labels"]), 500)
        self.assertEqual(len(corpus["expected"]), 500)

        splits_by_group = {}
        for query in corpus["queries"]:
            splits_by_group.setdefault(query["group_id"], set()).add(query["split"])
        self.assertEqual(len(splits_by_group), 100)
        self.assertTrue(all(len(splits) == 1 for splits in splits_by_group.values()))
        self.assertEqual(sum(query["split"] == "heldout" for query in corpus["queries"]), 100)
        self.assertEqual(sum(query["cache_mode"] == "cold" for query in corpus["queries"]), 250)
        self.assertEqual(sum(query["cache_mode"] == "warm" for query in corpus["queries"]), 250)
        for row in corpus["expected"]:
            if row["must_be_empty"]:
                self.assertEqual(row["reason"], "no_match")
                self.assertIsNone(row["matched_on"])
            else:
                self.assertIsNone(row["reason"])
                self.assertIsInstance(row["matched_on"], dict)

    def test_same_seed_builds_identical_rows_and_different_seed_changes_order(self):
        first = build_corpus(seed=41)
        repeated = build_corpus(seed=41)
        other = build_corpus(seed=42)
        self.assertEqual(first, repeated)
        self.assertNotEqual(first["queries"], other["queries"])
        self.assertEqual({q["query_id"] for q in first["queries"]}, {q["query_id"] for q in other["queries"]})

    def test_write_and_load_round_trip_verifies_hashes_and_references(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = write_corpus(Path(tmp), seed=7)
            loaded = load_corpus(Path(tmp))
            summary = validate_corpus(Path(tmp))
            self.assertEqual(manifest["query_count"], 500)
            self.assertEqual(manifest["index_row_count"], 1500)
            self.assertEqual(len(loaded["records"]), 600)
            self.assertEqual(summary["heldout_queries"], 100)
            self.assertFalse(manifest["human_reviewed"])
            self.assertEqual(manifest["label_source"], "deterministic_synthetic_oracle")

    def test_loader_rejects_file_hash_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            write_corpus(Path(tmp))
            queries = Path(tmp) / "queries.jsonl"
            queries.write_text(queries.read_text(encoding="utf-8") + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                load_corpus(Path(tmp))

    def test_perfect_expected_results_pass_quality_and_leakage_metrics(self):
        corpus = build_corpus()
        results = [
            {"query_id": row["query_id"], "memory_ids": [item["memory_id"] for item in row["top3"]]}
            for row in corpus["expected"]
        ]
        metrics = evaluate_results(corpus, results)
        self.assertEqual(metrics["precision_at_3"], 1.0)
        self.assertEqual(metrics["recall_at_3"], 1.0)
        self.assertEqual(metrics["empty_accuracy"], 1.0)
        self.assertEqual(metrics["cross_host_leaks"], 0)
        self.assertEqual(metrics["cross_task_leaks"], 0)
        self.assertEqual(metrics["cross_kind_leaks"], 0)
        self.assertEqual(metrics["inactive_hits"], 0)
        self.assertTrue(metrics["gate_pass"])

    def test_evaluator_reports_host_task_kind_and_inactive_leaks(self):
        corpus = build_corpus()
        query = next(row for row in corpus["queries"] if row["query_type"] == "compose")
        own = next(row for row in corpus["records"] if row["id"] == f"mem-{query['group_index']:03d}-compose")
        foreign = next(row for row in corpus["records"] if row["id"] == f"mem-{query['group_index']:03d}-foreign")
        research = next(row for row in corpus["records"] if row["id"] == f"mem-{query['group_index']:03d}-research")
        retired = next(row for row in corpus["records"] if row["id"] == f"mem-{query['group_index']:03d}-retired")
        rows = [{"query_id": row["query_id"], "memory_ids": []} for row in corpus["queries"]]
        selected = next(row for row in rows if row["query_id"] == query["query_id"])
        selected["memory_ids"] = [foreign["id"], research["id"], retired["id"], own["id"]]

        metrics = evaluate_results(corpus, rows)
        self.assertEqual(metrics["cross_host_leaks"], 1)
        self.assertEqual(metrics["cross_task_leaks"], 1)
        self.assertEqual(metrics["inactive_hits"], 1)
        self.assertFalse(metrics["gate_pass"])

    def test_evaluator_rejects_missing_duplicate_and_unknown_query_or_memory(self):
        corpus = build_corpus()
        results = [{"query_id": q["query_id"], "memory_ids": []} for q in corpus["queries"]]
        with self.assertRaisesRegex(ValueError, "exactly one result"):
            evaluate_results(corpus, results[:-1])
        results[0]["memory_ids"] = ["does-not-exist"]
        with self.assertRaisesRegex(ValueError, "unknown memory"):
            evaluate_results(corpus, results)
        results[0]["memory_ids"] = [corpus["records"][0]["id"], corpus["records"][0]["id"]]
        with self.assertRaisesRegex(ValueError, "duplicate memory"):
            evaluate_results(corpus, results)


if __name__ == "__main__":
    unittest.main()
