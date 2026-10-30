"""Deterministic, synthetic recall corpus and scorer (RCL-22, PERF-13)."""
import hashlib
import json
import random
from pathlib import Path
from urllib.parse import urlsplit


FORMAT_VERSION = 1
GENERATOR_VERSION = "1.0.0"
DEFAULT_SEED = 20261028
GROUPS = 100
QUERIES_PER_GROUP = 5
K = 3
FILES = ("records.jsonl", "queries.jsonl", "labels.jsonl", "expected.jsonl")


def _encode_json(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _encode_jsonl(rows):
    return b"".join(_encode_json(row) + b"\n" for row in rows)


def read_jsonl(path):
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            raise ValueError(f"blank row in {path.name}:{line_number}")
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise ValueError(f"invalid JSON in {path.name}:{line_number}") from error
    return rows


def _memory(memory_id, host, route, task, kind, status, keywords, intent):
    return {
        "id": memory_id,
        "version": 1,
        "status": status,
        "verified_at": 1_800_000_000,
        "confidence_band": 1,
        "l0": {
            "kind": kind,
            "site_host": host,
            "url_patterns": [route],
            "task_category": task,
            "intent_key": intent,
            "keywords": keywords,
        },
        "l1": {
            "title": "Synthetic browser workflow",
            "summary": "Use the fixture page and confirm its saved state.",
            "key_points": ["Check the independent save oracle."],
            "pitfalls": [],
        },
        "l2": {
            "preconditions": ["synthetic_fixture_open"],
            "steps": [{"action": "click", "note_code": "save_draft"}],
            "structure": {"page_types": ["editor"]},
            "recovery": [],
            "detail": "Synthetic fixture record; contains no user data.",
        },
    }


def build_corpus(seed=DEFAULT_SEED):
    """Build 500 fixed-shape queries, 600 records, and deterministic golden labels.

    Labels are derived from the synthetic construction rules. They are not
    human-reviewed and must not be reported as independent product-quality
    evaluation.
    """
    records, queries, labels, expected = [], [], [], []
    for group in range(GROUPS):
        group_id = f"g{group:03d}"
        host = f"tenant-{group:03d}.example.com"
        foreign_host = f"mirror-{group:03d}.example.net"
        route = "/editor/post/:id"
        sibling_route = "/settings/profile"
        ids = {
            "compose": f"mem-{group:03d}-compose",
            "research": f"mem-{group:03d}-research",
            "gotcha": f"mem-{group:03d}-gotcha",
            "sibling": f"mem-{group:03d}-sibling",
            "retired": f"mem-{group:03d}-retired",
            "foreign": f"mem-{group:03d}-foreign",
        }
        records.extend(
            [
                _memory(ids["compose"], host, route, "compose-draft", "workflow", "active",
                        ["blog", "draft", "write"], "write-post"),
                _memory(ids["research"], host, route, "research", "workflow", "active",
                        ["research", "search"], "research-site"),
                _memory(ids["gotcha"], host, route, "compose-draft", "gotcha", "active",
                        ["draft", "publish"], "avoid-publish"),
                _memory(ids["sibling"], host, sibling_route, "compose-draft", "workflow", "active",
                        ["blog", "write"], "edit-profile"),
                _memory(ids["retired"], host, route, "compose-draft", "workflow",
                        "retired" if group % 2 == 0 else "superseded",
                        ["blog", "draft", "write"], "old-write-post"),
                _memory(ids["foreign"], foreign_host, route, "compose-draft", "workflow", "active",
                        ["blog", "draft", "write"], "foreign-write-post"),
            ]
        )

        query_specs = [
            ("compose", f"https://{host}/editor/post/{group}", "compose-draft", "workflow",
             ["blog", "draft", "write"], ids["compose"], {"route": 3, "task": True, "keyword_matches": 3}),
            ("research", f"https://{host}/editor/post/{group}", "research", "workflow",
             ["research", "search"], ids["research"], {"route": 3, "task": True, "keyword_matches": 2}),
            ("gotcha", f"https://{host}/editor/post/{group}", "compose-draft", "gotcha",
             ["draft", "publish"], ids["gotcha"], {"route": 3, "task": True, "keyword_matches": 2}),
            ("sibling", f"https://{host}/settings/profile", None, "workflow",
             ["blog", "write"], ids["sibling"], {"route": 3, "task": False, "keyword_matches": 2}),
        ]
        if group % 2 == 0:
            query_specs.append(("wrong-kind", f"https://{host}/editor/post/{group}",
                                "compose-draft", "site_note", ["blog", "draft"], None, None))
        else:
            absent_host = f"missing-{group:03d}.example.org"
            query_specs.append(("missing-host", f"https://{absent_host}/editor/post/{group}",
                                "compose-draft", "workflow", ["blog", "draft"], None, None))

        split = "heldout" if group % 5 == 0 else "train"
        group_record_ids = sorted(ids.values())
        for query_type, url, task, kind, keywords, relevant_id, matched_on in query_specs:
            query_id = f"rcl-{group:03d}-{query_type}"
            queries.append({
                "query_id": query_id,
                "group_id": group_id,
                "group_index": group,
                "split": split,
                "query_type": query_type,
                "cache_mode": "warm" if group % 2 else "cold",
                "url": url,
                "task_category": task,
                "kind": kind,
                "keywords": keywords,
            })
            labels.append({
                "query_id": query_id,
                "relevant": [] if relevant_id is None else [{"memory_id": relevant_id, "grade": 3}],
                "must_not_return": [memory_id for memory_id in group_record_ids if memory_id != relevant_id],
            })
            expected.append({
                "query_id": query_id,
                "top3": [] if relevant_id is None else [{"memory_id": relevant_id, "version": 1}],
                "must_be_empty": relevant_id is None,
                "reason": "no_match" if relevant_id is None else None,
                "matched_on": matched_on,
            })

    rng = random.Random(seed)
    for rows in (records, queries, labels, expected):
        rng.shuffle(rows)
    return {"records": records, "queries": queries, "labels": labels, "expected": expected}


def _validate_rows(corpus):
    records, queries, labels, expected = (corpus[name] for name in ("records", "queries", "labels", "expected"))
    records_by_id = {row.get("id"): row for row in records}
    queries_by_id = {row.get("query_id"): row for row in queries}
    labels_by_id = {row.get("query_id"): row for row in labels}
    expected_by_id = {row.get("query_id"): row for row in expected}
    if None in records_by_id or len(records_by_id) != len(records):
        raise ValueError("record IDs must be present and unique")
    if None in queries_by_id or len(queries_by_id) != len(queries):
        raise ValueError("query IDs must be present and unique")
    if set(queries_by_id) != set(labels_by_id) or set(queries_by_id) != set(expected_by_id):
        raise ValueError("query, label and expected IDs must match exactly")

    for memory_id, record in records_by_id.items():
        l0 = record.get("l0") or {}
        host = l0.get("site_host", "")
        if record.get("version") != 1 or record.get("status") not in (
            "active", "needs-reverify", "superseded", "retired"
        ):
            raise ValueError(f"invalid memory version or status for {memory_id}")
        if not host.endswith((".example.com", ".example.net")):
            raise ValueError(f"record host is outside the reserved example domains: {memory_id}")
        if l0.get("kind") not in ("workflow", "gotcha", "site_note"):
            raise ValueError(f"unknown memory kind for {memory_id}")
        if l0.get("task_category") not in ("compose-draft", "research"):
            raise ValueError(f"unknown memory task category for {memory_id}")
        if (not isinstance(l0.get("url_patterns"), list) or not 1 <= len(l0["url_patterns"]) <= 4
                or any(not isinstance(pattern, str) or not pattern.startswith("/") for pattern in l0["url_patterns"])):
            raise ValueError(f"memory needs URL patterns: {memory_id}")
        if not isinstance(l0.get("keywords"), list) or len(l0["keywords"]) > 8:
            raise ValueError(f"invalid memory keywords for {memory_id}")

    groups = {}
    for query_id, query in queries_by_id.items():
        if query.get("split") not in ("train", "heldout"):
            raise ValueError(f"invalid split for {query_id}")
        groups.setdefault(query.get("group_id"), set()).add(query["split"])
        parsed = urlsplit(query.get("url", ""))
        if parsed.scheme != "https" or not parsed.hostname or parsed.query or parsed.fragment or parsed.username:
            raise ValueError(f"query URL must be a synthetic canonical HTTPS URL: {query_id}")
        if not parsed.hostname.endswith((".example.com", ".example.net", ".example.org")):
            raise ValueError(f"query host is outside the reserved example domains: {query_id}")
        if query.get("task_category") not in (None, "compose-draft", "research"):
            raise ValueError(f"unknown task category for {query_id}")
        if query.get("kind") not in ("workflow", "gotcha", "site_note"):
            raise ValueError(f"unknown kind for {query_id}")
        if query.get("cache_mode") not in ("cold", "warm"):
            raise ValueError(f"invalid cache mode for {query_id}")
        if (not isinstance(query.get("keywords"), list) or len(query["keywords"]) > 8
                or len(query["keywords"]) != len(set(query["keywords"]))
                or not set(query["keywords"]).issubset({"blog", "draft", "write", "research", "search", "publish"})):
            raise ValueError(f"too many keywords for {query_id}")

        label = labels_by_id[query_id]
        result = expected_by_id[query_id]
        relevant = label.get("relevant")
        top3 = result.get("top3")
        if not isinstance(relevant, list) or not isinstance(top3, list) or len(top3) > K:
            raise ValueError(f"invalid relevance or top3 label for {query_id}")
        if any(item.get("grade") not in (1, 2, 3) for item in relevant):
            raise ValueError(f"relevance grade must be between 1 and 3 for {query_id}")
        relevant_ids = {item.get("memory_id") for item in relevant}
        top3_ids = [item.get("memory_id") for item in top3]
        if not relevant_ids.issubset(records_by_id) or not set(top3_ids).issubset(records_by_id):
            raise ValueError(f"label references an unknown memory for {query_id}")
        if (len(top3_ids) != len(set(top3_ids)) or bool(relevant_ids) == result.get("must_be_empty")
                or not isinstance(result.get("must_be_empty"), bool)):
            raise ValueError(f"inconsistent expected result for {query_id}")
        if len(relevant_ids) <= K and set(top3_ids) != relevant_ids:
            raise ValueError(f"golden top3 and relevance labels disagree for {query_id}")
        for item in top3:
            if item.get("version") != records_by_id[item["memory_id"]]["version"]:
                raise ValueError(f"golden version label disagrees with record for {query_id}")
        if not set(label.get("must_not_return", [])).issubset(records_by_id):
            raise ValueError(f"exclusion label references an unknown memory for {query_id}")
        if len(label.get("must_not_return", [])) != len(set(label.get("must_not_return", []))):
            raise ValueError(f"duplicate exclusion label for {query_id}")
        if relevant_ids.intersection(label.get("must_not_return", [])):
            raise ValueError(f"relevant memory is also excluded for {query_id}")

    if (len(groups) != GROUPS or any(len(splits) != 1 for splits in groups.values())
            or any(count != QUERIES_PER_GROUP for count in _group_query_counts(queries).values())):
        raise ValueError("split assignment must keep each query group together")
    if len(queries) < 500:
        raise ValueError("recall corpus must contain at least 500 queries")
    return records_by_id, queries_by_id, labels_by_id, expected_by_id


def _group_query_counts(queries):
    counts = {}
    for query in queries:
        counts[query["group_id"]] = counts.get(query["group_id"], 0) + 1
    return counts


def write_corpus(directory, seed=DEFAULT_SEED):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    corpus = build_corpus(seed)
    file_info = {}
    for name in FILES:
        key = name.removesuffix(".jsonl")
        data = _encode_jsonl(corpus[key])
        (directory / name).write_bytes(data)
        file_info[name] = {"rows": len(corpus[key]), "sha256": hashlib.sha256(data).hexdigest()}
    distributions = {
        "record_status": _counts(corpus["records"], "status"),
        "record_kind": _counts(corpus["records"], "l0.kind"),
        "query_type": _counts(corpus["queries"], "query_type"),
        "query_split": _counts(corpus["queries"], "split"),
        "cache_mode": _counts(corpus["queries"], "cache_mode"),
    }
    manifest = {
        "format_version": FORMAT_VERSION,
        "generator_version": GENERATOR_VERSION,
        "seed": seed,
        "schema_version": 1,
        "vocabulary_version": "2026-01",
        "label_source": "deterministic_synthetic_oracle",
        "human_reviewed": False,
        "warning": "Synthetic scope/filter labels; not an independent human relevance evaluation.",
        "record_count": len(corpus["records"]),
        "version_count": len(corpus["records"]),
        "index_row_count": sum(
            len(row["l0"]["url_patterns"]) * max(1, len(row["l0"]["keywords"]))
            for row in corpus["records"]
        ),
        "query_count": len(corpus["queries"]),
        "label_count": len(corpus["labels"]),
        "expected_count": len(corpus["expected"]),
        "heldout_count": sum(query["split"] == "heldout" for query in corpus["queries"]),
        "distribution": distributions,
        "files": file_info,
    }
    (directory / "manifest.json").write_bytes(_encode_json(manifest) + b"\n")
    _validate_rows(corpus)
    return manifest


def load_corpus(directory):
    directory = Path(directory)
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("format_version") != FORMAT_VERSION:
        raise ValueError("unsupported corpus format version")
    if manifest.get("label_source") != "deterministic_synthetic_oracle" or manifest.get("human_reviewed") is not False:
        raise ValueError("corpus must identify its labels as synthetic and not human-reviewed")
    corpus = {}
    for name in FILES:
        data = (directory / name).read_bytes()
        info = manifest.get("files", {}).get(name, {})
        if hashlib.sha256(data).hexdigest() != info.get("sha256"):
            raise ValueError(f"hash mismatch for {name}")
        key = name.removesuffix(".jsonl")
        corpus[key] = read_jsonl(directory / name)
        if len(corpus[key]) != info.get("rows"):
            raise ValueError(f"row count mismatch for {name}")
    records_by_id, queries_by_id, labels_by_id, expected_by_id = _validate_rows(corpus)
    counts = {
        "record_count": len(records_by_id),
        "version_count": len(records_by_id),
        "index_row_count": sum(
            len(row["l0"]["url_patterns"]) * max(1, len(row["l0"]["keywords"]))
            for row in corpus["records"]
        ),
        "query_count": len(queries_by_id),
        "label_count": len(labels_by_id),
        "expected_count": len(expected_by_id),
    }
    if any(manifest.get(key) != value for key, value in counts.items()):
        raise ValueError("manifest row totals do not match corpus files")
    heldout_count = sum(query["split"] == "heldout" for query in corpus["queries"])
    if manifest.get("heldout_count") != heldout_count:
        raise ValueError("manifest heldout count does not match corpus files")
    distributions = {
        "record_status": _counts(corpus["records"], "status"),
        "record_kind": _counts(corpus["records"], "l0.kind"),
        "query_type": _counts(corpus["queries"], "query_type"),
        "query_split": _counts(corpus["queries"], "split"),
        "cache_mode": _counts(corpus["queries"], "cache_mode"),
    }
    if manifest.get("distribution") != distributions:
        raise ValueError("manifest distribution does not match corpus files")
    corpus["manifest"] = manifest
    return corpus


def validate_corpus(directory):
    corpus = load_corpus(directory)
    return {
        "records": len(corpus["records"]),
        "queries": len(corpus["queries"]),
        "labels": len(corpus["labels"]),
        "heldout_queries": corpus["manifest"]["heldout_count"],
        "index_rows": corpus["manifest"]["index_row_count"],
        "human_reviewed": corpus["manifest"]["human_reviewed"],
    }


def _counts(rows, key):
    counts = {}
    for row in rows:
        value = row
        for part in key.split("."):
            value = value[part]
        counts[value] = counts.get(value, 0) + 1
    return dict(sorted(counts.items()))


def evaluate_results(corpus, result_rows, k=K):
    """Score normalized `{query_id, memory_ids}` rows against synthetic labels."""
    records_by_id, queries_by_id, labels_by_id, expected_by_id = _validate_rows(corpus)
    if isinstance(k, bool) or not isinstance(k, int) or k < 1:
        raise ValueError("k must be a positive integer")
    results = {}
    for row in result_rows:
        query_id = row.get("query_id")
        if query_id not in queries_by_id:
            raise ValueError(f"unknown query in results: {query_id}")
        if query_id in results:
            raise ValueError(f"expected exactly one result per query; duplicate {query_id}")
        memory_ids = row.get("memory_ids")
        if not isinstance(memory_ids, list) or any(not isinstance(value, str) for value in memory_ids):
            raise ValueError(f"memory_ids must be a list of strings for {query_id}")
        if len(memory_ids) != len(set(memory_ids)):
            raise ValueError(f"duplicate memory ID in results for {query_id}")
        for memory_id in memory_ids:
            if memory_id not in records_by_id:
                raise ValueError(f"unknown memory in results for {query_id}: {memory_id}")
        results[query_id] = memory_ids[:k]
    if set(results) != set(queries_by_id):
        raise ValueError("expected exactly one result per query")

    positive_count = empty_count = empty_correct = exact_top3 = 0
    precision_total = recall_total = reciprocal_rank_total = 0.0
    top1_correct = 0
    scope_leaks = cross_host_leaks = cross_task_leaks = cross_kind_leaks = inactive_hits = 0
    false_positive_hits = 0
    for query_id, query in queries_by_id.items():
        returned = results[query_id]
        relevant = {item["memory_id"] for item in labels_by_id[query_id]["relevant"]}
        forbidden = set(labels_by_id[query_id]["must_not_return"])
        expected_ids = [item["memory_id"] for item in expected_by_id[query_id]["top3"]]
        exact_top3 += returned == expected_ids
        if relevant:
            positive_count += 1
            hits = [memory_id for memory_id in returned if memory_id in relevant]
            precision_total += len(hits) / max(1, min(k, len(returned)))
            recall_total += len(hits) / len(relevant)
            rank = next((index for index, memory_id in enumerate(returned, 1) if memory_id in relevant), None)
            reciprocal_rank_total += 0.0 if rank is None else 1.0 / rank
            top1_correct += bool(returned and returned[0] in relevant)
        else:
            empty_count += 1
            empty_correct += not returned
        query_host = urlsplit(query["url"]).hostname
        for memory_id in returned:
            record = records_by_id[memory_id]
            l0 = record["l0"]
            scope_leaks += memory_id in forbidden
            cross_host_leaks += l0["site_host"] != query_host
            cross_task_leaks += query["task_category"] is not None and l0["task_category"] != query["task_category"]
            cross_kind_leaks += query["kind"] is not None and l0["kind"] != query["kind"]
            inactive_hits += record["status"] not in ("active", "needs-reverify")
            false_positive_hits += memory_id not in relevant

    metrics = {
        "query_count": len(queries_by_id),
        "positive_queries": positive_count,
        "empty_queries": empty_count,
        "precision_at_3": round(precision_total / positive_count, 6),
        "recall_at_3": round(recall_total / positive_count, 6),
        "mrr_at_3": round(reciprocal_rank_total / positive_count, 6),
        "top1_accuracy": round(top1_correct / positive_count, 6),
        "exact_top3_accuracy": round(exact_top3 / len(queries_by_id), 6),
        "empty_accuracy": round(empty_correct / empty_count, 6) if empty_count else 1.0,
        "false_positive_hits": false_positive_hits,
        "scope_leaks": scope_leaks,
        "cross_host_leaks": cross_host_leaks,
        "cross_task_leaks": cross_task_leaks,
        "cross_kind_leaks": cross_kind_leaks,
        "inactive_hits": inactive_hits,
    }
    metrics["gate_pass"] = (
        metrics["precision_at_3"] >= 0.90
        and metrics["recall_at_3"] >= 0.85
        and metrics["empty_accuracy"] == 1.0
        and scope_leaks == 0
        and cross_host_leaks == 0
        and cross_task_leaks == 0
        and cross_kind_leaks == 0
        and inactive_hits == 0
    )
    return metrics
