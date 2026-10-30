"""Offline adapter for already-normalized traces plus independent oracle verdicts."""
import copy


class ReplayAdapter:
    replay_only = True

    def __init__(self, records):
        self.records = copy.deepcopy(records)

    def __call__(self, context, spec):
        record = self.records[spec["run_id"]]
        for signal in record.get("signals", []):
            context.signal(signal)
        for event in record["events"]:
            row = dict(event)
            context.event(row.pop("type"), **row)
        oracle = record["oracle_pass"]
        if type(oracle) is not bool:
            raise ValueError("oracle_pass must be a bool")
        return oracle
