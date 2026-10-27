"""Block-randomized condition crossover (docs/test/metrics.md 1.1: a fixed T-M-I order is forbidden).

Every (task, block) contains each condition exactly once, in a random order drawn from a recorded seed,
so account state, quota, and cache effects cannot favor one condition. The plan is written before the run.
"""
import json
import random
from pathlib import Path


def make_plan(tasks, conditions, repeats, seed):
    rng = random.Random(seed)
    runs = []
    for block in range(1, repeats + 1):
        for task in tasks:
            order = list(conditions)
            rng.shuffle(order)
            for position, condition in enumerate(order, 1):
                runs.append(
                    {
                        "run_id": f"{task}-b{block}-{condition}",
                        "task": task,
                        "block": block,
                        "position": position,
                        "condition": condition,
                    }
                )
    return {"seed": seed, "tasks": list(tasks), "conditions": list(conditions), "repeats": repeats, "runs": runs}


def check_balance(plan):
    """Problems found: each (task, block) must hold every condition exactly once at positions 1..k."""
    problems = []
    conditions = sorted(plan["conditions"])
    for task in plan["tasks"]:
        for block in range(1, plan["repeats"] + 1):
            rows = [r for r in plan["runs"] if r["task"] == task and r["block"] == block]
            if sorted(r["condition"] for r in rows) != conditions:
                problems.append(f"{task} block {block}: conditions {[r['condition'] for r in rows]}")
            if sorted(r["position"] for r in rows) != list(range(1, len(conditions) + 1)):
                problems.append(f"{task} block {block}: positions are not 1..{len(conditions)}")
    ids = [r["run_id"] for r in plan["runs"]]
    if len(ids) != len(set(ids)):
        problems.append("duplicate run ids")
    return problems


def fixed_order(plan, task=None):
    """True if every block of a task uses the same order (the forbidden design) and there are 2+ blocks."""
    tasks = [task] if task else plan["tasks"]
    for name in tasks:
        orders = {}
        for row in plan["runs"]:
            if row["task"] == name:
                orders.setdefault(row["block"], []).append((row["position"], row["condition"]))
        sequences = [tuple(c for _, c in sorted(rows)) for rows in orders.values()]
        if len(sequences) >= 2 and len(set(sequences)) == 1:
            return True
    return False


def write_plan(plan, path):
    Path(path).write_text(json.dumps(plan, ensure_ascii=False, indent=2), encoding="utf-8")
