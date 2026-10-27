"""Launch the locally installed Google Chrome in a disposable profile and drive it over CDP.

Never attaches to a running Chrome and never uses a real profile: every instance gets its own
--user-data-dir under /tmp/wam-*, a random loopback debugging port, and its own process group.
"""
import os
import subprocess
import time
from pathlib import Path

from .cdp import CDP, CDPError
from .procs import Isolation, ManagedProcess, kill_by_arg

CANDIDATES = (
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
)


def find_chrome():
    override = os.environ.get("WAM_CHROME_BIN")
    if override:
        return override if Path(override).is_file() else None
    for path in CANDIDATES:
        if Path(path).is_file():
            return path
    return None


def chrome_version(binary):
    try:
        out = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=15).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None
    return out or None


class LocalChrome:
    def __init__(self, isolation=None, headless=True, size=(1280, 900), binary=None, extra_args=()):
        self.binary = binary or find_chrome()
        if not self.binary:
            raise FileNotFoundError("Google Chrome not found (set WAM_CHROME_BIN)")
        self.owns_isolation = isolation is None
        self.isolation = isolation or Isolation("chrome")
        self.headless = headless
        self.size = size
        self.extra_args = tuple(extra_args)
        self.proc = None
        self.cdp = None

    def start(self, timeout=30.0):
        profile = self.isolation.profile
        argv = [
            self.binary,
            f"--user-data-dir={profile}",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-sync",
            "--disable-component-update",
            "--disable-default-apps",
            "--disable-extensions",
            "--password-store=basic",
            "--use-mock-keychain",
            f"--window-size={self.size[0]},{self.size[1]}",
        ]
        if self.headless:
            argv.append("--headless=new")
        argv += list(self.extra_args)
        argv.append("about:blank")
        log = (self.isolation.root / "chrome.log").open("wb")
        self.proc = ManagedProcess(argv, env=self.isolation.env(), stdout=log, stderr=log)
        port_file = profile / "DevToolsActivePort"
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not self.proc.alive():
                raise CDPError(f"Chrome exited early with {self.proc.popen.returncode}; see {self.isolation.root}/chrome.log")
            if port_file.is_file():
                lines = port_file.read_text().split()
                if len(lines) >= 2:
                    self.cdp = CDP(f"ws://127.0.0.1:{lines[0]}{lines[1]}")
                    return self
            time.sleep(0.1)
        raise CDPError("timed out waiting for DevToolsActivePort")

    def new_page(self, url="about:blank"):
        return self.cdp.new_page(url, self.size)

    def version(self):
        return self.cdp.send("Browser.getVersion")

    def stop(self):
        if self.cdp:
            try:
                self.cdp.send("Browser.close", timeout=5)
            except CDPError:
                pass
            self.cdp.close()
            self.cdp = None
        if self.proc:
            deadline = time.monotonic() + 5
            while self.proc.alive() and time.monotonic() < deadline:
                time.sleep(0.1)
            self.proc.stop()
        # Helper processes carry our unique profile path in their arguments.
        kill_by_arg(str(self.isolation.root))
        if self.owns_isolation:
            self.isolation.cleanup()

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
        return False
