"""Disposable profiles and own-process-group management.

docs/test/README.md principles 3 and 7: tests run in disposable HOME/XDG/TMPDIR directories and clean up
only what they created. Paths stay short (/tmp/wam-*) because long macOS TMPDIR paths break unix sockets.
Nothing here attaches to, signals, or deletes anything that this process did not start.
"""
import os
import secrets
import shutil
import signal
import subprocess
import time
from pathlib import Path

TMP_ROOT = Path("/tmp")
PREFIX = "wam-"
MARKER = ".wam-owned"


class Isolation:
    """A disposable root with HOME, XDG_*, TMPDIR and a browser profile directory."""

    def __init__(self, label="run", root=TMP_ROOT):
        self.parent = Path(root)
        self.root = self.parent / f"{PREFIX}{secrets.token_hex(3)}"
        self.root.mkdir(mode=0o700)
        (self.root / MARKER).write_text(label)
        for sub in ("home", "xdg/config", "xdg/cache", "xdg/data", "tmp", "profile"):
            (self.root / sub).mkdir(parents=True)

    @property
    def profile(self):
        return self.root / "profile"

    def env(self, extra=None, inherit=("PATH", "LANG", "LC_ALL")):
        env = {k: os.environ[k] for k in inherit if k in os.environ}
        env.update(
            HOME=str(self.root / "home"),
            XDG_CONFIG_HOME=str(self.root / "xdg/config"),
            XDG_CACHE_HOME=str(self.root / "xdg/cache"),
            XDG_DATA_HOME=str(self.root / "xdg/data"),
            TMPDIR=str(self.root / "tmp") + "/",
        )
        if extra:
            env.update(extra)
        return env

    def owned(self):
        return self.root.name.startswith(PREFIX) and (self.root / MARKER).is_file()

    def cleanup(self):
        """Delete the root only if this class created it (name prefix and marker file)."""
        if self.owned() and self.root.parent == self.parent:
            shutil.rmtree(self.root, ignore_errors=True)
            return True
        return False


def _ps(fmt):
    return subprocess.run(["ps", "-axo", fmt], capture_output=True, text=True, check=True).stdout.splitlines()


def group_pids(pgid):
    """Live (non-zombie) pids in a process group."""
    pids = []
    for line in _ps("pid=,pgid=,stat="):
        parts = line.split()
        if len(parts) >= 3 and parts[1] == str(pgid) and not parts[2].startswith("Z"):
            pids.append(int(parts[0]))
    return pids


def pids_with_arg(fragment):
    """Pids whose command line contains a fragment (a unique path of ours), excluding this process."""
    me = os.getpid()
    found = []
    for line in _ps("pid=,command="):
        pid, _, command = line.strip().partition(" ")
        if pid.isdigit() and int(pid) != me and fragment in command:
            found.append(int(pid))
    return found


def kill_by_arg(fragment, term_wait=3.0):
    """Stop stragglers that carry our unique path in their arguments (browser helpers that left the group)."""
    for sig in (signal.SIGTERM, signal.SIGKILL):
        pids = pids_with_arg(fragment)
        if not pids:
            return
        for pid in pids:
            try:
                os.kill(pid, sig)
            except (ProcessLookupError, PermissionError):
                pass
        deadline = time.monotonic() + term_wait
        while time.monotonic() < deadline and pids_with_arg(fragment):
            time.sleep(0.05)


class ManagedProcess:
    """A child in its own session and process group, so that stop() only signals what we started."""

    def __init__(self, argv, env=None, cwd=None, stdout=None, stderr=None, stdin=subprocess.DEVNULL):
        self.argv = list(argv)
        self.popen = subprocess.Popen(
            self.argv, env=env, cwd=cwd, stdout=stdout, stderr=stderr, stdin=stdin, start_new_session=True
        )
        self.pgid = os.getpgid(self.popen.pid)

    @property
    def pid(self):
        return self.popen.pid

    def alive(self):
        return self.popen.poll() is None

    def stop(self, term_wait=5.0, kill_wait=3.0):
        for sig, wait in ((signal.SIGTERM, term_wait), (signal.SIGKILL, kill_wait)):
            self.popen.poll()
            if not group_pids(self.pgid):
                break
            try:
                os.killpg(self.pgid, sig)
            except (ProcessLookupError, PermissionError):
                pass
            deadline = time.monotonic() + wait
            while time.monotonic() < deadline:
                self.popen.poll()
                if not group_pids(self.pgid):
                    break
                time.sleep(0.05)
        try:
            self.popen.wait(timeout=2)
        except subprocess.TimeoutExpired:
            pass


def residue(isolation=None, procs=()):
    """What is left after cleanup: live group members, processes that mention our root, the root itself."""
    found = {"group_pids": [], "arg_pids": [], "dir": None}
    for proc in procs:
        found["group_pids"] += group_pids(proc.pgid)
    if isolation is not None:
        found["arg_pids"] = pids_with_arg(str(isolation.root))
        found["dir"] = str(isolation.root) if isolation.root.exists() else None
    return found


def is_clean(found):
    return not (found["group_pids"] or found["arg_pids"] or found["dir"])
