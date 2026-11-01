"""Owned native PostgreSQL/Redis fixtures; never accepts an existing endpoint."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import socket
import subprocess
import time

from .procs import Isolation, ManagedProcess, is_clean, residue


def free_port():
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        return probe.getsockname()[1]


def private_log(path):
    return os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb')


class NativeServices:
    """Every daemon is new, identity-checked, and in an owned process group."""
    def __init__(self, evidence, *, replicas=False, redis=True):
        names = ('initdb', 'postgres', 'psql', 'pg_dump', 'pg_restore', 'redis-server', 'redis-cli')
        self.tools = {name: shutil.which(name) for name in names}
        if any(value is None for value in self.tools.values()):
            raise RuntimeError('native service tools are missing')
        self.evidence = evidence
        self.isolation = Isolation('release-verification')
        self.env = self.isolation.env()
        self.processes, self.logs = [], []
        self.pgdata = self.isolation.root / 'pgdata'
        self.socket_dir = self.isolation.root / 'socket'
        self.socket_dir.mkdir()
        self.pgport, self.primary_port, self.replica_port = [free_port() for _ in range(3)]
        while len({self.pgport, self.primary_port, self.replica_port}) != 3:
            self.pgport, self.primary_port, self.replica_port = [free_port() for _ in range(3)]
        self.pg = self.primary = self.replica = None
        self.with_replica = replicas
        self.with_redis = redis

    def command(self, argv, *, input=None, timeout=30):
        return subprocess.run(argv, env=self.env, input=input, capture_output=True,
                              text=True, check=True, timeout=timeout)

    def psql(self, sql, database='postgres'):
        if database not in {'postgres', 'wam_source', 'wam_restored', 'wam_shutdown', 'wam_profile'}:
            raise ValueError('not an owned fixture database')
        return self.command([self.tools['psql'], '-X', '-w', '-h', '127.0.0.1', '-p', str(self.pgport),
                             '-U', 'wam_verify', '-d', database, '-v', 'ON_ERROR_STOP=1', '-At'], input=sql).stdout.strip()

    def pg_url(self, database):
        if database not in {'postgres', 'wam_source', 'wam_restored', 'wam_shutdown', 'wam_profile'}:
            raise ValueError('not an owned fixture database')
        return f'postgres://wam_verify@127.0.0.1:{self.pgport}/{database}'

    def redis(self, *args, replica=False, script=None):
        port = self.replica_port if replica else self.primary_port
        return self.command([self.tools['redis-cli'], '-h', '127.0.0.1', '-p', str(port), '--raw', *args],
                            input=script, timeout=10).stdout.strip()

    def wait(self, process, condition, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not process.alive():
                raise RuntimeError('owned service exited before readiness')
            try:
                if condition() and process.alive():
                    return
            except (OSError, subprocess.SubprocessError):
                pass
            time.sleep(.05)
        raise RuntimeError('owned service readiness timeout')

    def start(self):
        with private_log(self.evidence.path / 'sinks/initdb.log') as log:
            subprocess.run([self.tools['initdb'], '-D', str(self.pgdata), '-A', 'trust', '-U', 'wam_verify',
                            '--no-locale', '--encoding=UTF8'], env=self.env, stdout=log, stderr=log,
                           check=True, timeout=45)
        self.start_postgres()
        if self.with_redis:
            self.primary = self.start_redis('primary', self.primary_port)
        if self.with_replica:
            self.replica = self.start_redis('replica', self.replica_port, follow=self.primary_port)
            self.wait(self.replica, lambda: 'master_link_status:up' in self.redis('INFO', 'replication', replica=True))

    def start_postgres(self):
        log = private_log(self.evidence.path / f'sinks/postgres-{len(self.processes)}.log')
        self.logs.append(log)
        self.pg = ManagedProcess([self.tools['postgres'], '-D', str(self.pgdata), '-h', '127.0.0.1',
                                  '-p', str(self.pgport), '-k', str(self.socket_dir)],
                                 env=self.env, stdout=log, stderr=log)
        self.processes.append(self.pg)
        self.wait(self.pg, lambda: Path(self.psql('SHOW data_directory')).resolve() == self.pgdata.resolve())

    def start_redis(self, name, port, *, follow=None):
        directory = self.isolation.root / name
        directory.mkdir()
        log = private_log(self.evidence.path / f'sinks/{name}.log')
        self.logs.append(log)
        args = [self.tools['redis-server'], '--bind', '127.0.0.1', '--port', str(port),
                '--daemonize', 'no', '--protected-mode', 'yes', '--save', '', '--appendonly', 'no', '--dir', str(directory)]
        if follow:
            args += ['--replicaof', '127.0.0.1', str(follow)]
        process = ManagedProcess(args, env=self.env, stdout=log, stderr=log)
        self.processes.append(process)
        def owned():
            info = self.command([self.tools['redis-cli'], '-h', '127.0.0.1', '-p', str(port), '--raw', 'INFO', 'server']).stdout
            return f'process_id:{process.pid}' in info.splitlines()
        self.wait(process, owned)
        return process

    def close(self):
        for process in reversed(self.processes):
            process.stop(term_wait=5, kill_wait=2)
        for log in self.logs:
            log.close()
        self.isolation.cleanup()
        return is_clean(residue(self.isolation, self.processes))
