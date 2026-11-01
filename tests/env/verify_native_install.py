#!/usr/bin/env python3
"""Install actual source-built binaries into an owned root and smoke native hosts.

This does not replace signed release/OIDC or real-account acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import shutil
import subprocess
import sys
import threading
import time
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'lib'))
from wamtest.evidence import RunDir
from wamtest.native_services import private_log
from wamtest.procs import Isolation, ManagedProcess, is_clean, residue

ROOT = Path(__file__).resolve().parents[2]
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def require(value, message):
    if not value:
        raise RuntimeError(message)


def command(evidence, processes, name, args, *, env, cwd, timeout=60):
    with private_log(evidence.path / 'sinks' / name) as log:
        process = ManagedProcess(args, env=env, cwd=cwd, stdout=log, stderr=log)
        processes.append(process)
        code = process.popen.wait(timeout=timeout)
    require(code == 0, f'owned command failed: {name}')
    return (evidence.path / 'sinks' / name).read_text()


def mcp(evidence, processes, executable, env, request_url):
    replies = []
    received = queue.Queue()
    start = time.monotonic()
    with private_log(evidence.path / 'sinks/mcp.stderr') as log:
        child = ManagedProcess([str(executable)], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log)
        processes.append(child)
        def reader():
            while True:
                line = child.popen.stdout.readline(1024 * 1024 + 1)
                if not line:
                    return
                try:
                    if len(line) > 1024 * 1024: raise ValueError()
                    received.put((time.monotonic(), json.loads(line)))
                except ValueError:
                    received.put((0, None))
                    return
        worker = threading.Thread(target=reader, daemon=True)
        worker.start()
        methods = [
            {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'owned-release-check', 'version': '1'}}},
            {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/list', 'params': {}},
            {'jsonrpc': '2.0', 'id': 3, 'method': 'tools/call', 'params': {'name': 'web_memory_recall', 'arguments': {'url': request_url}}},
        ]
        first_ms = None
        for request in methods:
            child.popen.stdin.write((json.dumps(request) + '\n').encode()); child.popen.stdin.flush()
            observed, reply = received.get(timeout=15)
            require(reply is not None and reply.get('id') == request['id'] and 'error' not in reply, 'native MCP protocol failed')
            if request['id'] == 1: first_ms = (observed - start) * 1000
            replies.append(reply)
            if request['id'] == 1:
                child.popen.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                child.popen.stdin.flush()
        child.popen.stdin.close()
        require(child.popen.wait(timeout=5) == 0, 'native MCP did not exit cleanly')
        worker.join(timeout=2)
    require(len(replies[1]['result']['tools']) == 10, 'installed catalog is not ten tools')
    call = replies[2]['result']
    require(call['isError'] is False and json.loads(call['content'][0]['text'])['hits'], 'installed MCP recall did not hit seeded memory')
    with private_log(evidence.path / 'sinks/mcp-replies.json') as output:
        output.write(json.dumps(replies).encode())
    return first_ms


def main():
    missing = [name for name in ('cargo', 'codex', 'claude') if not shutil.which(name)]
    if missing:
        print(json.dumps({'passed': False, 'missing_tools': missing})); return 2
    isolation = Isolation('native-install-verification')
    processes = []
    passed = cleanup = False
    with RunDir('native-install', extra_meta={'scope': 'native source build and isolated host registration; no signed release or real account', 'synthetic_only': True}) as evidence:
        try:
            install = isolation.root / 'installed'
            started = time.monotonic()
            build_env = {**os.environ, 'CARGO_TARGET_DIR': str(ROOT / 'target')}
            for package in ('wam-cli', 'wam-mcp'):
                command(evidence, processes, f'{package}-install.log', ['cargo', 'install', '--locked', '--offline', '--root', str(install), '--path', str(ROOT / 'crates' / package)],
                        env=build_env, cwd=ROOT, timeout=600)
            installed_seconds = time.monotonic() - started
            command(evidence, processes, 'server-build.log',
                    ['cargo', 'build', '--locked', '--offline', '-p', 'wam-server', '--bin', 'wam-server'],
                    env=build_env, cwd=ROOT, timeout=600)
            suffix = '.exe' if os.name == 'nt' else ''
            cli, bridge = [install / 'bin' / (name + suffix) for name in ('wam', 'wam-mcp')]
            for binary in (cli, bridge): require(binary.is_file(), 'cargo installation omitted a binary')
            workspace = isolation.root / 'workspace'; workspace.mkdir()
            env = isolation.env({'CODEX_HOME': str(isolation.root / 'codex'), 'CLAUDE_CONFIG_DIR': str(isolation.root / 'claude'),
                                 'WAM_CONFIG_DIR': str(isolation.root / 'wam-config'), 'WAM_ANONYMOUS': '1'})
            # Codex requires CODEX_HOME to exist before its read-only `mcp get`.
            for variable in ('CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'WAM_CONFIG_DIR'):
                Path(env[variable]).mkdir()
            # Version/help are actual installed executables, not copied placeholders.
            command(evidence, processes, 'help.log', [str(cli), '--help'], env=env, cwd=workspace)
            server_env = isolation.env({'WAM_ENV': 'development', 'WAM_BIND': '127.0.0.1:0',
                'WAM_DATA_DIR': str(isolation.root / 'api'), 'WAM_DIAGNOSTICS_PATH': str(isolation.root / 'diagnostics.sqlite')})
            server_log = private_log(evidence.path / 'sinks/server.log')
            server = ManagedProcess([str(ROOT / 'target/debug/wam-server')], env=server_env, stdout=server_log, stderr=server_log)
            processes.append(server)
            try:
                deadline = time.monotonic() + 15
                origin = None
                while time.monotonic() < deadline and server.alive():
                    text = (evidence.path / 'sinks/server.log').read_text()
                    for line in text.splitlines():
                        if line.startswith('wam-server listening on '): origin = 'http://' + line.split(' on ', 1)[1]
                    if origin: break
                    time.sleep(.05)
                require(origin, 'owned native API did not start')
                env['WAM_ENDPOINT'] = origin
                with HTTP.open(origin + '/v1/memories', timeout=5) as response: catalog = json.load(response)
                require(catalog['memories'], 'native API has no seeded memory for first recall')
                first = catalog['memories'][0]
                url = first['canonical_url_patterns'][0].replace('/**', '/draft').replace(':id', 'fixture')
                status = json.loads(command(evidence, processes, 'cli-status.json', [str(cli), '--endpoint', origin, '--anonymous', 'status', '--json'], env=env, cwd=workspace))
                require(isinstance(status, dict), 'installed CLI status failed')
                cold_start_ms = mcp(evidence, processes, bridge, env, url)
                first_recall_seconds = time.monotonic() - started
                for host in ('codex', 'claude-code'):
                    program = 'codex' if host == 'codex' else 'claude'
                    scope = [] if host == 'codex' else ['--scope', 'user']
                    # Prove our install/remove cycle preserves a second owned registration.
                    command(evidence, processes, f'{host}-seed-other.log',
                            [program, 'mcp', 'add', 'wam-unrelated-fixture', *scope,
                             '--env', f'WAM_ENDPOINT={origin}', '--env', f'WAM_CONFIG_DIR={env["WAM_CONFIG_DIR"]}', '--', str(bridge)],
                            env=env, cwd=workspace)
                    args = [str(cli), '--endpoint', origin, 'install', host, '--scope', 'user', '--local', '--mcp-bin', str(bridge), '--json']
                    first = json.loads(command(evidence, processes, f'{host}-register.json', args, env=env, cwd=workspace))
                    again = json.loads(command(evidence, processes, f'{host}-repeat.json', args, env=env, cwd=workspace))
                    require(first['status'] == 'registered' and again['status'] == 'already_registered', 'host registration is not idempotent')
                    remove = [str(cli), 'uninstall', host, '--scope', 'user', '--json']
                    removed = json.loads(command(evidence, processes, f'{host}-remove.json', remove, env=env, cwd=workspace))
                    absent = json.loads(command(evidence, processes, f'{host}-absent.json', remove, env=env, cwd=workspace))
                    require(removed['status'] == 'removed' and absent['status'] == 'already_absent', 'host removal is not idempotent')
                    command(evidence, processes, f'{host}-preserved.log', [program, 'mcp', 'get', 'wam-unrelated-fixture'], env=env, cwd=workspace)
                    command(evidence, processes, f'{host}-remove-other.log', [program, 'mcp', 'remove', 'wam-unrelated-fixture', *scope], env=env, cwd=workspace)
                    evidence.case(f'{host}-registration-removal', 'pass', expected='registered/repeated/removed/absent in owned home', actual=True)
                host_versions = {name: command(evidence, processes, f'{name}-version.log', [name, '--version'], env=env, cwd=workspace).strip()
                                 for name in ('codex', 'claude')}
                report = {'passed': True, 'platform': platform.platform(), 'machine': platform.machine(),
                    'method': 'cargo install --locked --offline from dirty development source',
                    'signed_release_verified': False, 'real_account_authenticated': False,
                    'host_versions': host_versions,
                    'install_seconds': installed_seconds, 'first_successful_recall_seconds': first_recall_seconds,
                    'mcp_initialize_ms': cold_start_ms,
                    'binaries': {binary.name: {'bytes': binary.stat().st_size, 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()} for binary in (cli, bridge)}}
                with private_log(evidence.path / 'sinks/native-summary.json') as output: output.write(json.dumps(report, indent=2).encode())
                evidence.case('native-cli-mcp', 'pass', expected='installed CLI, ten-tool MCP handshake/recall, clean exit', actual=True)
                passed = True
            finally:
                server.stop(); server_log.close()
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, queue.Empty) as error:
            evidence.case('native-install', 'fail', expected='owned native installation and host smoke',
                          actual={'error': type(error).__name__, 'oracle': str(error) if isinstance(error, RuntimeError) else None})
        finally:
            for process in reversed(processes): process.stop()
            isolation.cleanup()
            cleanup = is_clean(residue(isolation, processes))
            evidence.case('cleanup', 'pass' if cleanup else 'fail', expected='no owned residues', actual=cleanup)
            evidence.event('done' if passed and cleanup else 'failed')
        print(json.dumps({'passed': passed and cleanup, 'cleanup_passed': cleanup, 'evidence': str(evidence.path)}))
    return 0 if passed and cleanup else 1


if __name__ == '__main__': raise SystemExit(main())
