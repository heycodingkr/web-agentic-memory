"""Release gate validation uses simulated evidence, never certifies this checkout."""
import copy
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools'))
from release.package import RELEASE_TARGETS
from release.readiness import MAX_ARTIFACT, POLICY, check, load_policy, template
from wamtest.doctrace import defined_ids

COMMIT = 'a' * 40
NOW = datetime(2026, 10, 29, 12, tzinfo=timezone.utc)


class ReadinessTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.repo = self.root / 'source'
        self.repo.mkdir()
        (self.repo / 'LICENSE').write_text('Synthetic fixture license, not a project license.')
        self.path = self.root / 'candidate.json'
        self.artifact = self.root / 'summary.json'
        self.artifact.write_text('{"synthetic_unit_fixture":true}')
        self.value = template(COMMIT, 'service')
        for record in self.value['records']:
            record.update(status='pass', dirty=False, artifact='summary.json', reviewer='fixture-reviewer',
                          sha256=hashlib.sha256(self.artifact.read_bytes()).hexdigest(), observed_at=NOW.isoformat())

    def tearDown(self):
        self.temp.cleanup()

    def result(self, value=None, **kwargs):
        self.path.write_text(json.dumps(self.value if value is None else value))
        return check(self.path, COMMIT, 'service', repo=self.repo, now=NOW, verify_checkout=False, **kwargs)

    def record(self, gate, case=None):
        return next(record for record in self.value['records'] if record['gate'] == gate
                    and (case is None or record['case'] == case))

    def reasons(self, report):
        return {row['reason'] for row in report['blockers']}

    def test_policy_ids_and_platform_cases_match_the_contract(self):
        policy = load_policy()
        ids = {gate['id'] for gate in policy['gates']}
        self.assertEqual(ids, {f'REL-{n:02}' for n in range(1, 13)})
        self.assertTrue(ids <= defined_ids())
        for profile, expected in [('client', {1, 2, 3, 4, 5, 6, 7, 11}), ('service', set(range(1, 12)))]:
            self.assertEqual({gate['id'] for gate in policy['gates'] if profile in gate['profiles'] and not gate.get('optional')},
                             {f'REL-{n:02}' for n in expected})
        for gate in policy['gates']:
            if gate['id'] in {'REL-03', 'REL-04'}:
                self.assertEqual(set(gate['cases']), set(RELEASE_TARGETS))

    def test_complete_fresh_integral_simulated_evidence_passes(self):
        report = self.result()
        self.assertTrue(report['ready'])
        self.assertEqual(len(report['passed']), len(self.value['records']))
        self.assertFalse(report['claims_checked'])

    def test_pending_template_and_all_nonpass_states_block(self):
        value = template(COMMIT, 'service')
        self.assertFalse(self.result(value)['ready'])
        for status in ['pending', 'blocked', 'skipped', 'fail']:
            with self.subTest(status=status):
                self.value['records'][0]['status'] = status
                self.assertIn('not-pass', self.reasons(self.result()))

    def test_missing_case_and_claim_evidence_are_not_assumed_to_pass(self):
        self.value['records'].pop(0)
        self.assertIn('missing-evidence', self.reasons(self.result()))
        report = self.result(claims=True)
        self.assertEqual(sum(row['gate'] == 'REL-12' for row in report['blockers']), 3)

    def test_candidate_source_policy_and_dirty_results_are_bound(self):
        cases = [
            (lambda v: v.update(candidate='b' * 40), 'candidate-mismatch'),
            (lambda v: v.update(policy_sha256='b' * 64), 'policy-mismatch'),
            (lambda v: v['records'][0].update(source_commit='b' * 40), 'source-mismatch'),
            (lambda v: v['records'][0].update(dirty=True), 'dirty-result'),
            (lambda v: v['records'][0].update(dirty=0), 'dirty-result'),
        ]
        for mutate, reason in cases:
            with self.subTest(reason=reason):
                value = copy.deepcopy(self.value)
                mutate(value)
                self.assertIn(reason, self.reasons(self.result(value)))

    def test_fixture_live_wrong_kind_and_missing_manual_review_are_blocked(self):
        self.record('REL-05')['fixture_only'] = True
        self.record('REL-06')['kind'] = 'local'
        self.record('REL-11').pop('reviewer')
        reasons = self.reasons(self.result())
        self.assertTrue({'fixture-not-allowed', 'wrong-evidence-kind', 'missing-reviewer'} <= reasons)

    def test_timestamp_age_future_and_timezone_boundaries(self):
        for stamp, reason in [
            (NOW - timedelta(days=14), None),
            (NOW - timedelta(days=14, seconds=1), 'stale-evidence'),
            (NOW + timedelta(minutes=5), None),
            (NOW + timedelta(minutes=5, seconds=1), 'future-evidence'),
            (NOW.replace(tzinfo=None), 'invalid-timestamp'),
        ]:
            with self.subTest(stamp=stamp):
                value = copy.deepcopy(self.value)
                value['records'][0]['observed_at'] = stamp.isoformat()
                report = self.result(value)
                self.assertEqual(report['ready'], reason is None)
                if reason:
                    self.assertIn(reason, self.reasons(report))

    def test_tampered_missing_empty_and_oversized_artifacts_block(self):
        for mode, reason in [('tamper', 'artifact-hash'), ('missing', 'artifact-missing'),
                             ('empty', 'artifact-size'), ('large', 'artifact-size')]:
            with self.subTest(mode=mode):
                path = self.root / f'{mode}.json'
                if mode == 'tamper': path.write_text('changed')
                if mode == 'empty': path.write_bytes(b'')
                if mode == 'large':
                    with path.open('wb') as file: file.truncate(MAX_ARTIFACT + 1)
                value = copy.deepcopy(self.value)
                value['records'][0]['artifact'] = path.name
                self.assertIn(reason, self.reasons(self.result(value)))

    def test_artifact_paths_cannot_escape_or_follow_symlinks(self):
        linked = self.root / 'linked'
        linked.symlink_to(self.repo, target_is_directory=True)
        leaf = self.root / 'leaf.json'
        leaf.symlink_to(self.artifact)
        for path, reason in [('../outside', 'artifact-path'), (str(self.artifact), 'artifact-path'),
                             ('a/../summary.json', 'artifact-path'), ('C:\\outside', 'artifact-path'),
                             ('leaf.json', 'artifact-symlink'), ('linked/LICENSE', 'artifact-symlink')]:
            with self.subTest(path=path):
                value = copy.deepcopy(self.value)
                value['records'][0]['artifact'] = path
                self.assertIn(reason, self.reasons(self.result(value)))

    def test_missing_project_license_cannot_be_replaced_by_a_claimed_pass(self):
        (self.repo / 'LICENSE').unlink()
        self.assertIn('required-file-missing', self.reasons(self.result()))

    def test_unknown_duplicate_misspelled_and_wrong_schema_fields_are_rejected(self):
        values = []
        duplicate = copy.deepcopy(self.value)
        duplicate['records'].append(duplicate['records'][0])
        values.append(duplicate)
        for fields in [{'gate': 'REL-99'}, {'case': 'unregistered'}, {'status': 'success'}, {'token': 'PRIVATE-SENTINEL'}]:
            value = copy.deepcopy(self.value)
            value['records'][0].update(fields)
            values.append(value)
        values.append({**self.value, 'schema_version': True})
        for value in values:
            with self.subTest(value_type=type(value).__name__), self.assertRaises(ValueError):
                self.result(value)

    def test_duplicate_json_keys_and_nonfinite_values_are_rejected(self):
        for text in ['{"schema_version":1,"schema_version":1}', '{"value":NaN}']:
            self.path.write_text(text)
            with self.assertRaises(ValueError):
                check(self.path, COMMIT, 'service', verify_checkout=False)

    def test_cli_template_is_pending_private_and_never_overwrites(self):
        command = [sys.executable, '-B', str(ROOT / 'tools/release/readiness.py'), '--init', str(self.path),
                   '--candidate', COMMIT, '--profile', 'service']
        done = subprocess.run(command, capture_output=True, text=True, timeout=10)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertFalse(json.loads(done.stdout)['ready'])
        original = self.path.read_bytes()
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        again = subprocess.run(command, capture_output=True, text=True, timeout=10)
        self.assertEqual(again.returncode, 2)
        self.assertEqual(self.path.read_bytes(), original)
        command[command.index('--init')] = '--evidence'
        blocked = subprocess.run(command + ['--json'], capture_output=True, text=True, timeout=10)
        self.assertEqual(blocked.returncode, 1, blocked.stderr)
        self.assertFalse(json.loads(blocked.stdout)['ready'])

    def test_cli_invalid_content_is_never_echoed(self):
        self.path.write_text('not json PRIVATE-SENTINEL')
        done = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/release/readiness.py'), '--evidence', str(self.path),
                               '--candidate', COMMIT, '--profile', 'client', '--json'],
                              capture_output=True, text=True, timeout=10)
        self.assertEqual(done.returncode, 2)
        self.assertNotIn('PRIVATE-SENTINEL', done.stdout + done.stderr)

    def test_boolean_policy_version_cannot_masquerade_as_schema_one(self):
        policy = json.loads(POLICY.read_text())
        policy['schema_version'] = True
        path = self.root / 'policy.json'
        path.write_text(json.dumps(policy))
        with self.assertRaises(ValueError):
            load_policy(path)

    def test_real_checkout_sha_and_dirty_state_are_checked(self):
        import os
        env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1'}
        for args in [ ['init', '-q'], ['add', 'LICENSE'],
                     ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                      '-c', 'commit.gpgsign=false', 'commit', '-qm', 'synthetic fixture'] ]:
            subprocess.run(['git', *args], cwd=self.repo, env=env, check=True, capture_output=True)
        head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=self.repo, env=env,
                              check=True, capture_output=True, text=True).stdout.strip()
        value = copy.deepcopy(self.value)
        value['candidate'] = head
        for record in value['records']: record['source_commit'] = head
        self.path.write_text(json.dumps(value))
        self.assertTrue(check(self.path, head, 'service', repo=self.repo, now=NOW)['ready'])
        (self.repo / 'LICENSE').write_text('uncommitted fixture change')
        self.assertIn('dirty-checkout', self.reasons(check(self.path, head, 'service', repo=self.repo, now=NOW)))


if __name__ == '__main__':
    unittest.main()
