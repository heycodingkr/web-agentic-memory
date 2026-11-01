#!/usr/bin/env python3
"""Exercise real owned local backup/restore, Redis promotion and in-flight shutdown.

This is a synthetic local operational rehearsal, never a production/staging claim.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'lib'))
from wamtest.evidence import RunDir
from wamtest.native_services import NativeServices, free_port, private_log
from wamtest.procs import ManagedProcess

ROOT = Path(__file__).resolve().parents[2]
TOKEN = None
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


class OracleFailure(RuntimeError):
    """Only static, input-free oracle descriptions may enter shared evidence."""


def require(condition, message):
    if not condition:
        raise OracleFailure(message)


def request(origin, path, body=None, *, timeout=15):
    payload = json.dumps(body).encode() if body is not None else None
    headers = {'content-type': 'application/json'}
    if TOKEN:
        headers['authorization'] = f'Bearer {TOKEN}'
    req = urllib.request.Request(origin + path, data=payload, headers=headers)
    try:
        response = HTTP.open(req, timeout=timeout)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, dict(response.headers), response.read()


def run(evidence, processes, name, argv, *, env=None, timeout=180):
    path = evidence.path / 'sinks' / name
    with private_log(path) as log:
        child = ManagedProcess(argv, cwd=ROOT, env=env, stdout=log, stderr=log)
        processes.append(child)
        code = child.popen.wait(timeout=timeout)
    require(code == 0, 'owned command failed')
    return path


def probe(evidence, services, name, mode, *, database='wam_source', replica=False):
    env = {**services.env, 'WAM_RELEASE_PROBE_ROOT': str(services.isolation.root),
           'WAM_TEST_DATABASE_URL': services.pg_url(database),
           'WAM_TEST_REDIS_URL': f'redis://127.0.0.1:{services.replica_port if replica else services.primary_port}',
           'WAM_RELEASE_REDIS_PID': str(services.replica.pid if replica else services.primary.pid)}
    result = run(evidence, services.processes, name, [str(ROOT / 'target/debug/examples/release_probe'), mode], env=env)
    data = json.loads(result.read_text())
    require(data.get('passed') is True, 'probe did not explicitly pass')
    return data


def start_server(evidence, services, name, *, replica=False):
    port = free_port()
    env = services.isolation.env({
        'WAM_ENV': 'development', 'WAM_BIND': f'127.0.0.1:{port}',
        'WAM_DATA_DIR': str(services.isolation.root / name),
        'WAM_DATABASE_URL': services.pg_url('wam_shutdown') + '?application_name=wam_release_server',
        'WAM_RATE_LIMIT_REDIS_URL': f'redis://127.0.0.1:{services.replica_port if replica else services.primary_port}',
        'WAM_RATE_LIMIT_KEY_SECRET': 'wam-owned-release-key-secret-32-bytes',
    })
    log = private_log(evidence.path / 'sinks' / f'{name}.log')
    services.logs.append(log)
    server = ManagedProcess([str(ROOT / 'target/debug/wam-server')], env=env, stdout=log, stderr=log)
    services.processes.append(server)
    origin = f'http://127.0.0.1:{port}'
    services.wait(server, lambda: request(origin, '/healthz', timeout=1)[0] == 200)
    return server, origin


def inflight_shutdown(evidence, services):
    global TOKEN
    services.psql('CREATE DATABASE wam_shutdown')
    probe(evidence, services, 'shutdown-identity.json', 'bootstrap', database='wam_shutdown')
    TOKEN = (services.isolation.root / 'shutdown-device-token').read_text()
    server, origin = start_server(evidence, services, 'api-inflight')
    body = {'url': 'https://example.org/editor', 'task_category': 'compose-draft',
            'actions': ['navigate', 'edit', 'save'], 'consent': True, 'consent_version': 'public-sharing-v1', 'nonce': 'release-prime'}
    prime_status, _, prime_body = request(origin, '/v1/me/contributions', body)
    evidence.sink('prime-response.json', json.dumps({'status': prime_status, 'body': json.loads(prime_body)}))
    require(prime_status == 201, 'prime actual write')
    log = private_log(evidence.path / 'sinks/lock-holder.log')
    services.logs.append(log)
    env = {**services.env, 'PGAPPNAME': 'wam_release_holder'}
    holder = ManagedProcess([services.tools['psql'], '-X', '-w', '-h', '127.0.0.1', '-p', str(services.pgport),
                             '-U', 'wam_verify', '-d', 'wam_shutdown', '-v', 'ON_ERROR_STOP=1', '-At'],
                            env=env, stdin=subprocess.PIPE, stdout=log, stderr=log)
    services.processes.append(holder)
    holder.popen.stdin.write(b'SELECT pg_advisory_lock(hashtext(current_schema()),971803);\n')
    holder.popen.stdin.flush()
    services.wait(holder, lambda: services.psql("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.application_name='wam_release_holder' AND l.locktype='advisory' AND l.granted", 'wam_shutdown') == '1')
    pool = ThreadPoolExecutor(max_workers=1)
    future = pool.submit(request, origin, '/v1/me/contributions', {**body, 'nonce': 'release-inflight'})
    try:
        services.wait(server, lambda: services.psql("SELECT count(*) FROM pg_stat_activity WHERE application_name='wam_release_server' AND wait_event_type='Lock' AND position('pg_advisory_xact_lock' in query)>0", 'wam_shutdown') == '1')
        os.kill(server.pid, signal.SIGTERM)
        time.sleep(.15)
        require(server.alive(), 'SIGTERM killed a blocked admitted write')
        holder.popen.stdin.write(b'SELECT pg_advisory_unlock(hashtext(current_schema()),971803);\n\\q\n')
        holder.popen.stdin.flush()
        holder.popen.stdin.close()
        status, headers, data = future.result(timeout=15)
        require(status == 201, 'admitted write did not finish during drain')
        require(server.popen.wait(timeout=10) == 0, 'server did not shut down cleanly')
        response = json.loads(data)
        # Application nonce storage deliberately uses SHA-256, never the raw nonce.
        nonce = hashlib.sha256(b'release-inflight').hexdigest()
        require(services.psql(f"SELECT count(*) FROM wam_contributions WHERE nonce='{nonce}'", 'wam_shutdown') == '1', 'in-flight write did not commit exactly once')
        require(services.psql(f"SELECT count(*) FROM wam_work_items WHERE contribution_id=(SELECT id FROM wam_contributions WHERE nonce='{nonce}')", 'wam_shutdown') == '1', 'queue commit was partial')
        trace = headers.get('x-wam-trace-id') or headers.get('X-Wam-Trace-Id')
        require(trace and len(trace) == 32 and all(c in '0123456789abcdef' for c in trace), 'missing diagnostic trace')
        require(services.psql(f"SELECT count(*) FROM wam_diagnostic_events WHERE trace_id='{trace}' AND status_code=201", 'wam_shutdown') == '1', 'diagnostics not flushed before exit')
        restarted, address = start_server(evidence, services, 'api-restarted')
        status, _, listing = request(address, '/v1/me/contributions')
        require(status == 200 and any(row['id'] == response['id'] for row in json.loads(listing)['contributions']), 'committed write lost on restart')
        retry_status, _, retry_body = request(address, '/v1/me/contributions', {**body, 'nonce': 'release-inflight'})
        require(retry_status == 201 and json.loads(retry_body)['id'] == response['id'], 'restart retry broke idempotency')
        evidence.case('sigterm-inflight', 'pass', expected='201, exactly-one queue/write, flush, restart', actual=True)
        return restarted, address
    finally:
        if not holder.popen.stdin.closed:
            try:
                holder.popen.stdin.write(b'SELECT pg_advisory_unlock_all();\n\\q\n')
                holder.popen.stdin.flush()
                holder.popen.stdin.close()
            except (OSError, BrokenPipeError):
                pass
        holder.stop()
        pool.shutdown(wait=True)


def main():
    passed = cleanup = False
    services = None
    processes = []
    with RunDir('release-services', extra_meta={'synthetic_only': True, 'scope': 'owned native loopback; not staging'}) as evidence:
        try:
            run(evidence, processes, 'build.log', ['cargo', 'build', '--locked', '-p', 'wam-server', '--example', 'release_probe', '--bin', 'wam-server'])
            services = NativeServices(evidence, replicas=True)
            services.start()
            evidence.case('owned-service-identity', 'pass', expected='new PG data directory and two Redis PIDs', actual=True)
            for database in ('wam_source', 'wam_restored'):
                services.psql(f'CREATE DATABASE {database}')
            before = probe(evidence, services, 'source.json', 'seed')
            backup = evidence.path / 'sinks/application.dump'
            run(evidence, services.processes, 'dump.log', [services.tools['pg_dump'], '-h', '127.0.0.1', '-p', str(services.pgport), '-U', 'wam_verify', '-Fc', '-f', str(backup), 'wam_source'], env=services.env)
            os.chmod(backup, 0o600)
            require(backup.stat().st_size > 0, 'empty database backup')
            run(evidence, services.processes, 'restore.log', [services.tools['pg_restore'], '-h', '127.0.0.1', '-p', str(services.pgport), '-U', 'wam_verify', '--exit-on-error', '-d', 'wam_restored', str(backup)], env=services.env)
            restored = probe(evidence, services, 'restored.json', 'verify', database='wam_restored')
            require(before == restored, 'restored data/schema differs')
            evidence.case('database-restore', 'pass', expected='all table/schema hashes, catalog, receipt, +22 and immutability', actual=True)
            services.pg.stop()
            services.start_postgres()
            require(before == probe(evidence, services, 'postgres-restarted.json', 'verify', database='wam_restored'), 'PostgreSQL restart lost state')
            evidence.case('database-restart', 'pass', expected='same all-table snapshot', actual=True)
            api, address = inflight_shutdown(evidence, services)
            probe(evidence, services, 'quota-source.json', 'quota-seed')
            barrier = services.redis(script='SET wam:verification:barrier 1\nWAIT 1 5000\n')
            require(barrier.splitlines() == ['OK', '1'], 'replication acknowledgement missing')
            require(services.redis('GET', 'wam:verification:barrier', replica=True) == '1', 'replica is not caught up')
            services.primary.stop()
            require(request(address, '/v1/memories', timeout=8)[0] == 503, 'Redis outage did not fail closed')
            evidence.case('redis-outage', 'pass', expected='actual API 503 while Redis is down', actual=True)
            require(services.redis('REPLICAOF', 'NO', 'ONE', replica=True) == 'OK', 'replica promotion failed')
            require(services.redis('ROLE', replica=True).splitlines()[0] == 'master', 'replica not promoted')
            probe(evidence, services, 'quota-promoted.json', 'quota-verify', replica=True)
            api.stop()
            api, address = start_server(evidence, services, 'api-promoted-redis', replica=True)
            require(request(address, '/v1/memories')[0] == 200, 'API did not recover after reconnect to promoted Redis')
            evidence.case('redis-manual-promotion', 'pass', expected='quota preserved; denial then API reconnect recovery', actual=True)
            versions = {tool: services.command([services.tools[tool], '--version']).stdout.strip()
                        for tool in ('postgres', 'pg_dump', 'pg_restore', 'redis-server')}
            evidence.sink('versions.json', json.dumps(versions))
            passed = True
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, urllib.error.URLError) as error:
            evidence.case('rehearsal', 'fail', expected='all owned local oracles', actual={
                'error': type(error).__name__, 'oracle': str(error) if isinstance(error, OracleFailure) else None})
        finally:
            for process in reversed(processes):
                process.stop()
            cleanup = services.close() if services else True
            evidence.case('cleanup', 'pass' if cleanup else 'fail', expected='no owned process/data residue', actual=cleanup)
            evidence.event('done' if passed and cleanup else 'failed')
        result = {'passed': passed and cleanup, 'cleanup_passed': cleanup, 'evidence': str(evidence.path),
                  'scope': 'local native rehearsal; manual Redis promotion/reconnect, no production topology or TLS'}
        print(json.dumps(result))
    return 0 if passed and cleanup else 1


if __name__ == '__main__':
    raise SystemExit(main())
