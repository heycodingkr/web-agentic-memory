import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from wamtest.evidence import RunDir

ROOT = Path(__file__).resolve().parents[3]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


profile = load('search_profile', 'tests/bench/profile_search.py')
install = load('native_install', 'tests/env/verify_native_install.py')
recovery = load('release_services', 'tests/env/verify_release_services.py')


def report():
    measurement = {'n': 2, 'raw_ms': [1.0, 2.0], 'p50_ms': 1.0, 'p95_ms': 2.0, 'p99_ms': 2.0}
    return {
        'schema_version': 1, 'passed': True, 'profile': 'release', 'synthetic_only': True,
        'human_reviewed': False, 'concurrency': 1, 'errors': 0, 'records': 1000,
        'index_rows': 12500, 'seed': 20261030, 'generator': 'search-profile-v1',
        'samples_per_trial': 2, 'repeats': 2, 'database_bytes': 123456, 'corpus_sha256': 'a' * 64,
        'cases': [{'case': name, 'expected': [] if name.startswith('missing-') else [['memory', 'version', 100]],
                   'first_observed': {'candidate_count': 0 if name.startswith('missing-') else 1},
                   'explain': [{'Plan': {'Node Type': 'Index Scan'}}],
                   'trials': [{'trial': repeat, **{phase: copy.deepcopy(measurement) for phase in ('core', 'database', 'http')}}
                              for repeat in range(2)]}
                  for name in sorted(profile.CASES)],
    }


class ProfileEvidenceTest(unittest.TestCase):
    def test_rss_monitor_failure_after_one_good_sample_cannot_pass(self):
        failed = threading.Event()
        process = Mock(pid=123456)
        process.alive.return_value = True
        process.popen.wait.side_effect = lambda **_: 0 if failed.wait(3) else 1

        def spawn(*args, **kwargs):
            kwargs['stdout'].write(json.dumps(report()).encode())
            kwargs['stdout'].flush()
            return process

        samples = 0

        def read_rss(*args, **kwargs):
            nonlocal samples
            samples += 1
            if samples == 1:
                return subprocess.CompletedProcess(args[0], 0, stdout='123')
            failed.set()
            raise subprocess.TimeoutExpired('ps', 2)

        with tempfile.TemporaryDirectory() as temporary:
            with RunDir('rss-failure', root=temporary) as evidence:
                services = SimpleNamespace(env={}, isolation=SimpleNamespace(root=Path(temporary)),
                                           pg_url=lambda _: 'postgres://owned-fixture', processes=[])
                with patch.object(profile, 'ManagedProcess', side_effect=spawn), \
                     patch.object(profile.subprocess, 'run', side_effect=read_rss):
                    with self.assertRaisesRegex(RuntimeError, 'RSS'):
                        profile.execute(evidence, services, 1000, 2, 2, 4096)

    def test_incomplete_duplicate_or_errored_workloads_cannot_pass(self):
        for mutate in (
            lambda value: value['cases'].pop(),
            lambda value: value['cases'].__setitem__(0, copy.deepcopy(value['cases'][1])),
            lambda value: value.__setitem__('errors', 1),
            lambda value: value.__setitem__('profile', 'debug'),
            lambda value: value['cases'][0]['trials'].pop(),
            lambda value: value['cases'][0].__setitem__('expected', []),
            lambda value: value['cases'][0].__setitem__('explain', []),
        ):
            with self.subTest(mutate=mutate):
                value = report()
                mutate(value)
                with self.assertRaises(ValueError):
                    profile.summarize(value)

    def test_nonfinite_missing_and_invented_percentiles_are_rejected(self):
        for mutate in (
            lambda phase: phase['raw_ms'].__setitem__(0, float('nan')),
            lambda phase: phase['raw_ms'].__setitem__(0, -1),
            lambda phase: phase['raw_ms'].pop(),
            lambda phase: phase.__setitem__('p95_ms', .01),
            lambda phase: phase.__setitem__('n', 99),
        ):
            with self.subTest(mutate=mutate):
                value = report()
                mutate(value['cases'][0]['trials'][0]['database'])
                with self.assertRaises(ValueError):
                    profile.summarize(value)

    def test_measured_latency_goal_miss_is_reported_separately_from_parity(self):
        value = report()
        value['cases'][0]['trials'][0]['database'] = {
            'n': 2, 'raw_ms': [120, 150], 'p50_ms': 120, 'p95_ms': 150, 'p99_ms': 150}
        summary = profile.summarize(value)
        self.assertFalse(summary['db_p95_goal_100ms_met'])
        self.assertTrue(summary['http_loopback_p95_goal_500ms_met'])
        self.assertEqual(len(summary['cases']), 7)

    def test_malformed_reports_fail_closed(self):
        for value in (None, [], {}, {'passed': True}, {**report(), 'cases': [None] * 7},
                      {**report(), 'corpus_sha256': []}):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    profile.summarize(value)


class NativeRunnerFailureTest(unittest.TestCase):
    def assert_timeout_fails(self, module, *args):
        process = Mock()
        process.popen.wait.side_effect = subprocess.TimeoutExpired('owned build', 1)
        output = io.StringIO()
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(module, 'ManagedProcess', return_value=process), \
                 patch.object(module, 'RunDir', side_effect=lambda *a, **kw: RunDir(*a, root=temporary, **kw)), \
                 patch('sys.stdout', output):
                self.assertEqual(module.main(*args), 1)
            result = json.loads(output.getvalue())
            self.assertFalse(result['passed'])
            metadata = json.loads((Path(result['evidence']) / 'meta.json').read_text())
            self.assertGreater(metadata['counts']['fail'], 0)
            self.assertTrue(result['cleanup_passed'])
        process.stop.assert_called()

    def test_search_build_timeout_is_not_a_success(self):
        self.assert_timeout_fails(profile, ['--sizes', '1000'])

    def test_recovery_build_timeout_is_not_a_success(self):
        self.assert_timeout_fails(recovery)

    def test_install_build_timeout_is_not_a_success(self):
        with patch.object(install.shutil, 'which', return_value='/owned/tool'), \
             patch.object(install, 'residue', return_value={'group_pids': [], 'arg_pids': [], 'dir': None}):
            self.assert_timeout_fails(install)


if __name__ == '__main__':
    unittest.main()
