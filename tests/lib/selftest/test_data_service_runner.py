import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from wamtest.evidence import RunDir

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('data_services', ROOT / 'tests/env/check_data_services.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class DataServiceRunnerTest(unittest.TestCase):
    def test_missing_local_tools_are_not_counted_as_a_success_or_skip(self):
        out = io.StringIO()
        with patch.object(runner.shutil, 'which', return_value=None), patch('sys.stdout', out):
            self.assertEqual(runner.main(), 2)
        self.assertEqual(json.loads(out.getvalue())['error'], 'missing-local-service-tools')

    def test_test_command_timeout_cannot_leave_the_fixture_result_successful(self):
        def process(argv, **kwargs):
            proc = Mock()
            if 'test' in argv:
                proc.popen.wait.side_effect = subprocess.TimeoutExpired('cargo', 1)
            return proc

        def run(argv, **kwargs):
            if 'test' in argv:
                raise subprocess.TimeoutExpired('cargo', 1)
            return subprocess.CompletedProcess(argv, 0, stdout='')

        with tempfile.TemporaryDirectory() as root:
            out = io.StringIO()
            with patch.object(runner.shutil, 'which', return_value='/synthetic/tool'), \
                 patch.object(runner, 'ManagedProcess', side_effect=process), \
                 patch.object(runner, 'wait_owned'), patch.object(runner.subprocess, 'run', side_effect=run), \
                 patch.object(runner, 'residue', return_value={'group_pids': [], 'arg_pids': [], 'dir': None}), \
                 patch.object(runner, 'RunDir', side_effect=lambda *args, **kw: RunDir(*args, root=root, **kw)), \
                 patch('sys.stdout', out):
                self.assertEqual(runner.main(), 1)
            self.assertFalse(json.loads(out.getvalue())['passed'])
