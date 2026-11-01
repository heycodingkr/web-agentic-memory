"""Exercise rendering failures without launching Chrome or invoking ffmpeg."""
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("wam_demo_render", ROOT / "demo/record/render.py")
render = importlib.util.module_from_spec(spec)
spec.loader.exec_module(render)


class RenderCleanupTest(unittest.TestCase):
    def run_render(self, failure=None, stills=False):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            real_mkdtemp = tempfile.mkdtemp
            proc = Mock()
            page = Mock()
            page.eval.return_value = 1 / render.FPS
            cdp = Mock()
            cdp.new_page.return_value = page

            def make_temp(suffix=None, prefix=None, dir=None):
                return real_mkdtemp(suffix=suffix, prefix=prefix, dir=root)

            def encode(frames, base, fps):
                if failure == "encode":
                    raise RuntimeError("encode")
                mp4 = base.with_suffix(".mp4")
                mp4.write_bytes(b"video")
                return mp4

            def poster(command, **kwargs):
                if failure == "poster":
                    raise RuntimeError("poster")
                Path(command[-1]).write_bytes(b"poster")

            with ExitStack() as stack:
                stack.enter_context(patch.object(render.tempfile, "mkdtemp", side_effect=make_temp))
                stack.enter_context(patch.object(render, "load_footage", return_value={}))
                launch = stack.enter_context(patch.object(render, "launch_chrome", return_value=(proc, "ws://fixture")))
                connect = stack.enter_context(patch.object(render, "CDP", return_value=cdp))
                stack.enter_context(patch.object(render, "encode", side_effect=encode))
                stack.enter_context(patch.object(render, "screenshot", return_value=b"frame"))
                stack.enter_context(patch.object(render.subprocess, "run", side_effect=poster))
                argv = ["render.py", "blog-flow", "--data", "fixture", "--fonts-dir", "fixture", "--out", str(root / "out")]
                if stills:
                    argv.extend(["--at", "0"])
                stack.enter_context(patch.object(sys, "argv", argv))
                if failure == "launch":
                    launch.side_effect = RuntimeError("launch")
                elif failure == "connect":
                    connect.side_effect = RuntimeError("connect")
                elif failure == "render":
                    page.eval.side_effect = RuntimeError("render")
                elif failure == "close":
                    cdp.close.side_effect = RuntimeError("close")
                if failure:
                    with self.assertRaisesRegex(RuntimeError, failure):
                        render.main()
                else:
                    render.main()
            self.assertEqual(sorted(p.name for p in root.iterdir() if p.name.startswith("wam-")), [])
            if failure != "launch":
                proc.terminate.assert_called_once()
                proc.wait.assert_called()
            if failure not in ("launch", "connect"):
                cdp.close.assert_called_once()

    def test_successful_stills_release_profile_and_process(self):
        self.run_render(stills=True)

    def test_successful_movie_releases_frames_profile_and_process(self):
        self.run_render()

    def test_launch_failure_releases_profile(self):
        self.run_render("launch")

    def test_connection_failure_reaps_chrome_and_releases_profile(self):
        self.run_render("connect")

    def test_render_failure_reaps_chrome_and_releases_profile(self):
        self.run_render("render")

    def test_encoding_failure_releases_frames_and_profile(self):
        self.run_render("encode")

    def test_poster_failure_releases_frames_and_profile(self):
        self.run_render("poster")

    def test_connection_close_failure_still_reaps_chrome(self):
        self.run_render("close", stills=True)

    def test_unresponsive_chrome_is_killed_and_reaped(self):
        proc = Mock()
        proc.wait.side_effect = [subprocess.TimeoutExpired("chrome", 5), None]
        render.stop_chrome(proc)
        proc.terminate.assert_called_once()
        proc.kill.assert_called_once()
        self.assertEqual(proc.wait.call_count, 2)

    def test_debug_port_timeout_reaps_chrome(self):
        proc = Mock()
        with patch.object(render.subprocess, "Popen", return_value=proc), \
                patch.object(render.urllib.request, "urlopen", side_effect=OSError), \
                patch.object(render.time, "sleep"):
            with self.assertRaisesRegex(RuntimeError, "debugging port"):
                render.launch_chrome("fixture")
        proc.terminate.assert_called_once()
        proc.wait.assert_called()

    def test_unexpected_debug_probe_error_also_reaps_chrome(self):
        proc = Mock()
        with patch.object(render.subprocess, "Popen", return_value=proc), \
                patch.object(render.urllib.request, "urlopen", side_effect=ValueError("bad response")):
            with self.assertRaisesRegex(ValueError, "bad response"):
                render.launch_chrome("fixture")
        proc.terminate.assert_called_once()
        proc.wait.assert_called()
