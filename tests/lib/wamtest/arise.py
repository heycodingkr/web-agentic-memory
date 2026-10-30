"""Launch an isolated second instance of the locally installed Arise browser and drive it over CDP.

Everything the app reads or writes is redirected into a disposable root (HOME, XDG, TMPDIR, CEF cache, sessions,
UI state, profiles), so the user's running Arise, its profile, and its settings are untouched. The app binary is
copied into a skeleton bundle whose Frameworks directory points at the installed frameworks (the recipe of the
legacy harness, reference/agent-browser/scripts/probe_agent_memory_live.py). No model, login, or service is
configured: web memory, telemetry, and update checks are off, and the chat agent loop is disabled.
Only processes this harness started are ever signalled.
"""
import json
import os
import plistlib
import shutil
import socket
import time
import urllib.request
from pathlib import Path

from .cdp import CDP, CDPError, Page
from .procs import Isolation, ManagedProcess, kill_by_arg

DEFAULT_APP = Path("/Applications/Arise.app")


def find_arise():
    override = os.environ.get("WAM_ARISE_APP")
    app = Path(override) if override else DEFAULT_APP
    return app if (app / "Contents" / "MacOS" / "Arise").is_file() else None


def arise_version(app):
    try:
        with open(Path(app) / "Contents" / "Info.plist", "rb") as handle:
            return plistlib.load(handle).get("CFBundleShortVersionString")
    except (OSError, plistlib.InvalidFileException):
        return None


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class _Tab:
    """Arise shows one browser tab; closing it would close the window, so close() is a no-op."""

    def __init__(self, page):
        self._page = page

    def __getattr__(self, name):
        return getattr(self._page, name)

    def close(self):
        pass


class LocalArise:
    def __init__(self, start_url, isolation=None, app=None, debug_port=None):
        self.app = Path(app) if app else find_arise()
        if not self.app:
            raise FileNotFoundError("Arise.app not found (set WAM_ARISE_APP)")
        self.start_url = start_url
        self.owns_isolation = isolation is None
        self.isolation = isolation or Isolation("arise")
        self.debug_port = debug_port or free_port()
        self.proc = None
        self.cdp = None
        self.page = None
        self._settings_path = None
        self._settings_probe_atime_ns = None

    def prepare(self, skeleton=False):
        """Build the environment (and, for unsigned dev builds only, a skeleton bundle). Nothing is launched.

        The installed release app is signed: macOS kills a copy that runs outside its bundle (exit -9), so the
        default runs the installed binary in place and isolates only what it reads and writes, through the
        environment. skeleton=True is the legacy recipe for unsigned dev builds. Returns (argv, env).
        """
        root = self.isolation.root
        bundle = root / "Arise.app"
        contents = bundle / "Contents"
        argv = [str(self.app / "Contents" / "MacOS" / "Arise")]
        bundle_env = {}
        if skeleton:
            (contents / "MacOS").mkdir(parents=True)
            shutil.copy2(self.app / "Contents" / "MacOS" / "Arise", contents / "MacOS" / "Arise")
            shutil.copy2(self.app / "Contents" / "Info.plist", contents / "Info.plist")
            (contents / "Frameworks").symlink_to(self.app / "Contents" / "Frameworks", target_is_directory=True)
            frameworks = contents / "Frameworks"
            argv = [str(contents / "MacOS" / "Arise")]
            bundle_env = {
                "ARRISE_CEF_FRAMEWORK": str(frameworks / "Chromium Embedded Framework.framework"),
                "ARRISE_CEF_BUNDLE": str(bundle),
                "ARRISE_CEF_HELPER": str(frameworks / "Arise Helper.app" / "Contents" / "MacOS" / "Arise Helper"),
            }
        config = root / "xdg" / "config" / "agent-browser"
        config.mkdir(parents=True)
        for sub in ("workspace", "sessions", "ui-state", "profiles", "cache"):
            (root / sub).mkdir(exist_ok=True)
        settings = config / "settings.toml"
        settings.write_text(
            'language = "en"\n'
            "auto_update_check = false\n"
            "telemetry_enabled = false\n"
            "web_agentic_memory_enabled = false\n"
            "web_workflow_contribution = false\n"
            "browser_memory_auto_recall = false\n"
            f'agent_workspace = "{root / "workspace"}"\n'
        )
        written = settings.stat()
        self._settings_probe_atime_ns = written.st_mtime_ns - 1_000_000_000
        os.utime(settings, ns=(self._settings_probe_atime_ns, written.st_mtime_ns))
        self._settings_path = settings
        env = self.isolation.env(
            {
                "AGENT_BROWSER_SESSIONS_DIR": str(root / "sessions"),
                "AGENT_BROWSER_UI_STATE": str(root / "ui-state"),
                "AGENT_BROWSER_PROFILES_DIR": str(root / "profiles"),
                "AGENT_BROWSER_NO_KEYCHAIN": "1",
                "ARRISE_ENGINE": "cef",
                "ARRISE_CEF_URL": self.start_url,
                "ARRISE_CEF_CACHE": str(root / "cache"),
                "ARRISE_CEF_DEBUG_PORT": str(self.debug_port),
                "ARRISE_MCP_BIN": str(self.app / "Contents" / "MacOS" / "agent_browser"),
                "ARISE_NO_UPDATE_CHECK": "1",
                "ARRISE_CHAT_AGENT_LOOP": "0",
                **bundle_env,
            }
        )
        return argv, env

    def runtime_config_path_verified(self):
        """Whether the isolated settings file was read after the probe was armed.

        Its atime is reset below its mtime during prepare(). A runtime read by
        the launched host updates atime on supported filesystems; otherwise the
        run fails closed instead of assuming that XDG_CONFIG_HOME was honored.
        """
        if self._settings_path is None or self._settings_probe_atime_ns is None:
            return False
        try:
            return self._settings_path.stat().st_atime_ns > self._settings_probe_atime_ns
        except OSError:
            return False

    def _http_json(self, path):
        with urllib.request.urlopen(f"http://127.0.0.1:{self.debug_port}{path}", timeout=3) as response:
            return json.loads(response.read())

    def start(self, timeout=60.0):
        argv, env = self.prepare()
        log = (self.isolation.root / "arise.log").open("wb")
        self.proc = ManagedProcess(argv, env=env, stdout=log, stderr=log)
        deadline = time.monotonic() + timeout
        prefix = self.start_url.split("/editor/")[0]
        while time.monotonic() < deadline:
            if not self.proc.alive():
                raise CDPError(f"Arise exited early ({self.proc.popen.returncode}); see {self.isolation.root}/arise.log")
            try:
                pages = [t for t in self._http_json("/json/list") if t.get("type") == "page" and t.get("url", "").startswith(prefix)]
            except (OSError, ValueError):
                pages = []
            if pages:
                self.cdp = CDP(pages[0]["webSocketDebuggerUrl"])
                self.page = Page(self.cdp, None, pages[0]["id"])
                self.page.call("Page.enable")
                self.page.ensure_focus()
                return self
            time.sleep(0.25)
        raise CDPError("timed out waiting for the Arise page to appear on the debugging port")

    def new_page(self, url="about:blank"):
        if url != "about:blank":
            self.page.navigate(url)
        return _Tab(self.page)

    def version(self):
        return {"product": self._http_json("/json/version").get("Browser")}

    def stop(self):
        if self.cdp:
            self.cdp.close()
            self.cdp = None
        if self.proc:
            self.proc.stop()
        kill_by_arg(str(self.isolation.root))
        if self.owns_isolation:
            self.isolation.cleanup()

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
        return False
