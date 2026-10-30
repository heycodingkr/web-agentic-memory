#!/usr/bin/env python3
"""Run ignored database tests on owned native PostgreSQL/Redis, then remove them.

Never attaches to a configured database/daemon. Requires installed local tools;
all service addresses and credentials are disposable loopback fixtures.
"""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'lib'))
from wamtest.evidence import RunDir
from wamtest.procs import Isolation, ManagedProcess, is_clean, residue

ROOT = Path(__file__).resolve().parents[2]


def port():
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        return probe.getsockname()[1]


def log_file(path):
    return os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb')


def wait_owned(process, probe, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not process.alive():
            raise RuntimeError('owned service exited before readiness')
        if probe() and process.alive():
            return
        time.sleep(0.05)
    raise RuntimeError('owned service did not become ready')


def main():
    names = ('initdb', 'postgres', 'psql', 'redis-server', 'redis-cli', 'cargo')
    tools = {name: shutil.which(name) for name in names}
    missing = [name for name, path in tools.items() if path is None]
    if missing:
        print(json.dumps({'error': 'missing-local-service-tools', 'tools': missing}))
        return 2
    isolation = Isolation('release-data-services')
    processes, streams = [], []
    success, cleanup = False, False
    with RunDir('release-data-services', extra_meta={'synthetic_only': True}) as evidence:
        try:
            pgdata, pgsocket, redisdata = [isolation.root / name for name in ('pgdata', 'socket', 'redis')]
            pgsocket.mkdir()
            redisdata.mkdir()
            service_env = isolation.env()
            pglog = log_file(evidence.path / 'sinks/postgres.log')
            redislog = log_file(evidence.path / 'sinks/redis.log')
            streams.extend([pglog, redislog])
            subprocess.run([tools['initdb'], '-D', str(pgdata), '-A', 'trust', '-U', 'wam_test',
                            '--no-locale', '--encoding=UTF8'], env=service_env, stdout=pglog, stderr=pglog,
                           check=True, timeout=45)
            pgport, redisport = port(), port()
            while redisport == pgport:
                redisport = port()
            pg = ManagedProcess([tools['postgres'], '-D', str(pgdata), '-h', '127.0.0.1',
                                 '-p', str(pgport), '-k', str(pgsocket)], env=service_env, stdout=pglog, stderr=pglog)
            processes.append(pg)
            rd = ManagedProcess([tools['redis-server'], '--bind', '127.0.0.1', '--port', str(redisport),
                                 '--daemonize', 'no', '--protected-mode', 'yes', '--save', '', '--appendonly', 'no',
                                 '--dir', str(redisdata)], env=service_env, stdout=redislog, stderr=redislog)
            processes.append(rd)

            def postgres_owned():
                result = subprocess.run([tools['psql'], '-X', '-w', '-h', '127.0.0.1', '-p', str(pgport),
                                         '-U', 'wam_test', '-d', 'postgres', '-At', '-c', 'SHOW data_directory'],
                                        env=service_env, capture_output=True, text=True, timeout=2)
                return (result.returncode == 0 and bool(result.stdout.strip())
                        and Path(result.stdout.strip()).resolve() == pgdata.resolve())

            def redis_owned():
                result = subprocess.run([tools['redis-cli'], '-h', '127.0.0.1', '-p', str(redisport),
                                         '--raw', 'INFO', 'server'], env=service_env, capture_output=True, text=True, timeout=2)
                return result.returncode == 0 and f'process_id:{rd.pid}' in result.stdout.splitlines()

            wait_owned(pg, postgres_owned)
            wait_owned(rd, redis_owned)
            evidence.case('owned-service-identity', 'pass', expected='new pgdata and Redis PID', actual=True)
            env = {**os.environ, 'WAM_TEST_DATABASE_URL': f'postgres://wam_test@127.0.0.1:{pgport}/postgres',
                   'WAM_TEST_REDIS_URL': f'redis://127.0.0.1:{redisport}'}
            commands = [
                ('application', ['-p', 'wam-server', '--test', 'postgres_application', '--test', 'postgres_diagnostics',
                                 '--test', 'rate_limit_redis', '--test', 'account_cleanup']),
                ('gatekeeper', ['-p', 'wam-gatekeeper', '--test', 'postgres_queue', '--test', 'postgres_store']),
            ]
            success = True
            for name, args in commands:
                with log_file(evidence.path / 'sinks' / f'{name}.log') as out:
                    command = ManagedProcess([tools['cargo'], 'test', '--locked', *args, '--', '--ignored'],
                                             cwd=ROOT, env=env, stdout=out, stderr=out)
                    processes.append(command)
                    exit_code = command.popen.wait(timeout=600)
                evidence.case(name, 'pass' if exit_code == 0 else 'fail', expected=0, actual=exit_code,
                              evidence=f'sinks/{name}.log')
                success = success and exit_code == 0
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            success = False
            evidence.case('fixture-runner', 'fail', expected='owned local fixture', actual=type(error).__name__)
        finally:
            for process in reversed(processes):
                process.stop(term_wait=5, kill_wait=2)
            for stream in streams:
                stream.close()
            isolation.cleanup()
            cleanup = is_clean(residue(isolation, processes))
            evidence.case('cleanup', 'pass' if cleanup else 'fail', expected='no owned residue', actual=cleanup)
            evidence.event('done' if success and cleanup else 'failed')
        print(json.dumps({'passed': success and cleanup, 'cleanup_passed': cleanup, 'evidence': str(evidence.path)}))
    return 0 if success and cleanup else 1


if __name__ == '__main__':
    sys.exit(main())
