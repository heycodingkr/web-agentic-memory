#!/usr/bin/env python3
"""Measure the actual core/PG/loopback HTTP path on an owned fixed synthetic corpus."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'lib'))
from wamtest.evidence import RunDir
from wamtest.native_services import NativeServices, private_log
from wamtest.procs import ManagedProcess

ROOT = Path(__file__).resolve().parents[2]
CASES = {'exact', 'prefix', 'exact-other-task', 'missing-route', 'missing-host', 'missing-kind', 'hot-host'}


def positive_integer(value):
    return type(value) is int and value > 0


def duration(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def summarize(report):
    try:
        return _summarize(report)
    except (KeyError, TypeError, AttributeError) as error:
        raise ValueError('malformed profile result') from error


def _summarize(report):
    if not isinstance(report, dict):
        raise ValueError('missing profile object')
    if (report.get('schema_version') != 1 or report.get('passed') is not True or report.get('profile') != 'release'
            or type(report.get('errors')) is not int or report['errors'] != 0
            or report.get('synthetic_only') is not True or report.get('human_reviewed') is not False
            or report.get('concurrency') != 1 or report.get('seed') != 20261030
            or report.get('generator') != 'search-profile-v1'
            or report.get('records') not in (1000, 10000, 100000, 1000000)
            or report.get('index_rows') != report['records'] * 25 // 2
            or not positive_integer(report.get('samples_per_trial')) or not 2 <= report['samples_per_trial'] <= 100
            or not positive_integer(report.get('repeats')) or not 2 <= report['repeats'] <= 10
            or not positive_integer(report.get('database_bytes'))
            or not isinstance(report.get('corpus_sha256'), str)
            or not re.fullmatch(r'[a-f0-9]{64}', report['corpus_sha256'])
            or not isinstance(report.get('cases'), list) or len(report['cases']) != len(CASES)
            or any(not isinstance(case, dict) for case in report['cases'])
            or {case.get('case') for case in report['cases']} != CASES):
        raise ValueError('incomplete profile result')
    summary = {'records': report['records'], 'index_rows': report['index_rows'],
               'corpus_sha256': report['corpus_sha256'], 'database_bytes': report['database_bytes'], 'cases': []}
    for case in report['cases']:
        expected = case.get('expected')
        if (not isinstance(expected, list) or len(expected) > 3
                or bool(expected) != (not case['case'].startswith('missing-'))
                or not isinstance(case.get('explain'), list) or not case['explain']
                or not isinstance(case['explain'][0].get('Plan'), dict)):
            raise ValueError('missing parity or query-plan evidence')
        if (not isinstance(case.get('trials'), list) or len(case['trials']) != report['repeats']
                or [trial.get('trial') for trial in case['trials']] != list(range(report['repeats']))):
            raise ValueError('missing repeat')
        row = {'case': case['case'], 'candidate_count': case['first_observed']['candidate_count'], 'phases': {}}
        if type(row['candidate_count']) is not int or row['candidate_count'] < len(expected):
            raise ValueError('candidate count contradicts parity evidence')
        for phase in ('core', 'database', 'http'):
            p95 = []
            for trial in case['trials']:
                measured = trial[phase]
                raw = measured.get('raw_ms')
                if (measured.get('n') != report['samples_per_trial'] or not isinstance(raw, list)
                        or len(raw) != report['samples_per_trial'] or not all(duration(value) for value in raw)):
                    raise ValueError('missing or invalid samples')
                ordered = sorted(raw)
                for label, percentile in (('p50_ms', .5), ('p95_ms', .95), ('p99_ms', .99)):
                    if not duration(measured.get(label)) or measured[label] != ordered[math.ceil(len(raw) * percentile) - 1]:
                        raise ValueError('percentile disagrees with raw measurements')
                p95.append(measured['p95_ms'])
            row['phases'][phase] = {'worst_trial_p95_ms': max(p95), 'mean_trial_p95_ms': statistics.mean(p95),
                                    'trial_p95_cv': statistics.pstdev(p95) / statistics.mean(p95) if statistics.mean(p95) else 0}
        summary['cases'].append(row)
    summary['db_p95_goal_100ms_met'] = all(case['phases']['database']['worst_trial_p95_ms'] <= 100 for case in summary['cases'])
    summary['http_loopback_p95_goal_500ms_met'] = all(case['phases']['http']['worst_trial_p95_ms'] <= 500 for case in summary['cases'])
    return summary


def execute(evidence, services, count, samples, repeats, max_rss_mib):
    env = {**services.env, 'WAM_RELEASE_PROBE_ROOT': str(services.isolation.root),
           'WAM_TEST_DATABASE_URL': services.pg_url('wam_profile')}
    output = evidence.path / 'sinks' / f'profile-{count}.json'
    error_log = evidence.path / 'sinks' / f'profile-{count}.stderr'
    peak = 0
    exceeded = False
    monitor_failed = False
    rss_samples = 0
    stop = threading.Event()
    with private_log(output) as stdout, private_log(error_log) as stderr:
        process = ManagedProcess([str(ROOT / 'target/release/examples/release_search_profile'), str(count), str(samples), str(repeats)],
                                 env=env, stdout=stdout, stderr=stderr, cwd=ROOT)
        services.processes.append(process)
        def sample():
            nonlocal peak, exceeded, monitor_failed, rss_samples
            while not stop.wait(.2) and process.alive():
                try:
                    result = subprocess.run(['ps', '-p', str(process.pid), '-o', 'rss='], capture_output=True, text=True, timeout=2)
                    if result.returncode == 0 and result.stdout.strip():
                        rss_samples += 1
                        peak = max(peak, int(result.stdout.strip()))
                        if peak > max_rss_mib * 1024:
                            exceeded = True
                            process.stop()
                            return
                    elif process.alive():
                        monitor_failed = True
                        return
                except (OSError, ValueError, subprocess.SubprocessError):
                    monitor_failed = True
                    return
        worker = threading.Thread(target=sample, daemon=True)
        worker.start()
        try:
            code = process.popen.wait(timeout=1200)
        finally:
            stop.set()
            worker.join(timeout=5)
        evidence.event('probe-resource-measured', records=count, exit_code=code,
                       peak_probe_rss_kib=peak, max_rss_mib=max_rss_mib, exceeded=exceeded,
                       rss_samples=rss_samples, monitor_failed=monitor_failed)
        if exceeded:
            raise RuntimeError('owned profile exceeded its declared RSS budget')
        if monitor_failed or worker.is_alive() or code or not peak:
            raise RuntimeError('owned profile failed or RSS was not measured')
    report = json.loads(output.read_text())
    if (report.get('records'), report.get('samples_per_trial'), report.get('repeats')) != (count, samples, repeats):
        raise ValueError('probe result differs from requested workload')
    summary = summarize(report)
    summary['peak_probe_rss_kib'] = peak
    summary['rss_scope'] = 'native profile process, including exhaustive oracle and embedded HTTP; PostgreSQL memory excluded'
    return summary


def environment(services):
    sources = ['Cargo.toml', 'Cargo.lock', 'crates/wam-server/Cargo.toml',
               'crates/wam-server/examples/release_search_profile.rs',
               'crates/wam-server/examples/release_support/mod.rs',
               'crates/wam-server/tests/lifecycle_support/mod.rs',
               'crates/wam-server/src/durable/postgres.rs', 'crates/wam-server/src/durable/recall_candidates.sql',
               'crates/wam-core/src/recall.rs', 'crates/wam-core/src/text_search.rs',
               'tests/bench/profile_search.py']
    sources += [str(path.relative_to(ROOT)) for path in sorted((ROOT / 'crates/wam-server/migrations').glob('*.sql'))]
    settings = services.psql("""SELECT jsonb_object_agg(name, setting) FROM pg_settings
        WHERE name IN ('server_version','shared_buffers','effective_cache_size','work_mem','max_connections',
                       'max_parallel_workers_per_gather','random_page_cost','jit','track_io_timing','statement_timeout')""")
    binary = ROOT / 'target/release/examples/release_search_profile'
    return {'platform': platform.platform(), 'machine': platform.machine(), 'logical_cpus': os.cpu_count(),
            'rustc': subprocess.run(['rustc', '-Vv'], capture_output=True, text=True, check=True, timeout=10).stdout.strip(),
            'cargo': subprocess.run(['cargo', '--version'], capture_output=True, text=True, check=True, timeout=10).stdout.strip(),
            'postgres_settings': json.loads(settings), 'probe_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'source_sha256': {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in sources},
            'profile': 'release', 'topology': 'one owned native PostgreSQL, loopback HTTP, concurrency=1',
            'default_rss_budget_mib': 4096, 'rss_scope': 'probe only; PostgreSQL excluded',
            'workload_limitations': ['fixed seven cases, not shuffled or production-weighted',
                                     'warm repeats, OS cache not flushed', 'synthetic English keywords, one kind',
                                     '1000 even host shards; hot-host case omits task; no extreme host skew',
                                     'in-memory admission quota x100; not production Redis throughput']}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sizes', nargs='+', type=int, choices=[1000, 10000, 100000, 1000000], default=[1000, 10000, 100000])
    parser.add_argument('--samples', type=int, default=10)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--max-rss-mib', type=int, default=4096)
    args = parser.parse_args(argv)
    if not 2 <= args.samples <= 100 or not 2 <= args.repeats <= 10 or not 256 <= args.max_rss_mib <= 8192 or len(set(args.sizes)) != len(args.sizes):
        parser.error('invalid fixed profile bounds or duplicate size')
    services = None
    results = []
    passed = cleanup = False
    with RunDir('search-profile', extra_meta={'synthetic_only': True, 'human_reviewed': False,
                'scope': 'shared local development machine; not dedicated-runner or production capacity certification'}) as evidence:
        try:
            with private_log(evidence.path / 'sinks/build.log') as log:
                build = ManagedProcess(['cargo', 'build', '--release', '--locked', '-p', 'wam-server', '--example', 'release_search_profile'], cwd=ROOT, stdout=log, stderr=log)
                try:
                    if build.popen.wait(timeout=600): raise RuntimeError('release build failed')
                finally:
                    build.stop()
            services = NativeServices(evidence, redis=False)
            services.start()
            with private_log(evidence.path / 'sinks/environment.json') as output:
                output.write((json.dumps(environment(services), indent=2) + '\n').encode())
            for count in args.sizes:
                services.psql('DROP DATABASE IF EXISTS wam_profile WITH (FORCE)')
                services.psql('CREATE DATABASE wam_profile')
                result = execute(evidence, services, count, args.samples, args.repeats, args.max_rss_mib)
                results.append(result)
                evidence.case(f'parity-{count}', 'pass', expected='all repeated core/DB/HTTP IDs, versions and scores identical; HTTP error0', actual=True,
                              evidence=f'sinks/profile-{count}.json')
                evidence.event('latency-target-measured', records=count,
                    database_goal_met=result['db_p95_goal_100ms_met'], http_goal_met=result['http_loopback_p95_goal_500ms_met'])
            with private_log(evidence.path / 'sinks/summary.json') as output:
                output.write((json.dumps(results, indent=2) + '\n').encode())
            passed = True
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
            evidence.case('profile-execution', 'fail', expected='complete measured native release profiles',
                          actual={'error': type(error).__name__, 'oracle': str(error) if isinstance(error, RuntimeError) else None})
        finally:
            cleanup = services.close() if services else True
            evidence.case('cleanup', 'pass' if cleanup else 'fail', expected='no owned processes/data', actual=cleanup)
            evidence.event('done' if passed and cleanup else 'failed')
        print(json.dumps({'passed': passed and cleanup, 'cleanup_passed': cleanup, 'evidence': str(evidence.path),
                          'profiles_completed': [row['records'] for row in results], 'latency_goals_are_separate': True}))
    return 0 if passed and cleanup else 1


if __name__ == '__main__':
    raise SystemExit(main())
