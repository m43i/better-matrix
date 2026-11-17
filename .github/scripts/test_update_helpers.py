"""Exercise the shipped update helpers with real files and a parent process.

Only relaunch is substituted: fixtures are text rather than runnable app binaries.
Run on macOS and Windows in CI to cover native move/lock semantics.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPERS = ROOT / 'apps/desktop/src/platform/updates'


class HelperTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='update [test] ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.work = self.root / 'work with spaces'
        self.work.mkdir()
        self.error = self.root / 'error.txt'
        self.launched = self.root / 'launched.txt'
        self.windows = os.name == 'nt' or os.environ.get('BM_TEST_WINDOWS') == '1'
        self.target = self.root / ('Better Matrix.exe' if self.windows else 'Better Matrix.app')
        self.staged = self.work / self.target.name
        for path, content in [(self.target, 'old'), (self.staged, 'new')]:
            if not self.windows:
                path.mkdir()
                path = path / 'version'
            path.write_text(content)
        # Keep the current file open in a separate process, as a running Windows exe is locked.
        locked = self.target if self.windows else self.target / 'version'
        self.parent = subprocess.Popen([sys.executable, '-c',
            'import sys,time; f=open(sys.argv[1]); print("ready",flush=True); time.sleep(60)',
            str(locked)], stdout=subprocess.PIPE, text=True)
        self.parent.stdout.readline()
        self.addCleanup(self.stop_parent)

    def stop_parent(self):
        if self.parent.poll() is None:
            self.parent.terminate()
        self.parent.wait(timeout=10)
        self.parent.stdout.close()

    def launch(self, fail_launch=False, expect_ready=True):
        env = dict(os.environ, BM_LAUNCHED=str(self.launched), BM_FAIL_LAUNCH='1' if fail_launch else '0')
        if self.windows:
            # Invoke the exact script under Windows PowerShell; only Start-Process is a stub.
            wrapper = self.root / 'test.ps1'
            wrapper.write_text('''
function Start-Process {
    param($FilePath, $WorkingDirectory)
    if ($env:BM_FAIL_LAUNCH -eq '1' -and (Get-Content -LiteralPath $FilePath) -eq 'new') {
        throw 'simulated launch failure'
    }
    [IO.File]::WriteAllText($env:BM_LAUNCHED, (Get-Content -LiteralPath $FilePath))
}
& $env:BM_HELPER $env:BM_TARGET $env:BM_STAGED $env:BM_PARENT $env:BM_WORK $env:BM_ERROR
exit $LASTEXITCODE
''')
            env.update(BM_HELPER=str(HELPERS / 'install.ps1'), BM_TARGET=str(self.target),
                       BM_STAGED=str(self.staged), BM_PARENT=str(self.parent.pid),
                       BM_WORK=str(self.work), BM_ERROR=str(self.error))
            command = [os.environ.get('BM_POWERSHELL', 'powershell.exe'), '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', str(wrapper)]
        else:
            # macOS transaction logic also runs on Linux; use a deterministic fake launcher.
            helper = self.root / 'install.sh'
            source = (HELPERS / 'install.sh').read_text()
            source = source.replace('/usr/bin/open -n -a "$target"', '''relaunch "$target"''')
            helper.write_text('''relaunch() {
    if [ "$BM_FAIL_LAUNCH" = 1 ] && [ "$(cat "$1/version")" = new ]; then return 1; fi
    cat "$1/version" > "$BM_LAUNCHED"
}
''' + source)
            command = ['/bin/sh', str(helper), str(self.target), str(self.staged),
                       str(self.parent.pid), str(self.work), str(self.error)]
        self.helper = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop_helper)
        if not expect_ready:
            return
        deadline = time.monotonic() + 15
        while not (self.work / 'ready').exists():
            if self.helper.poll() is not None:
                self.fail(f'Helper exited before ready: {self.helper.communicate()}')
            if time.monotonic() > deadline:
                self.fail('Helper never became ready')
            time.sleep(0.05)

    def stop_helper(self):
        if self.helper.poll() is None:
            self.helper.kill()
        self.helper.communicate(timeout=10)

    def version(self):
        return (self.target if self.windows else self.target / 'version').read_text()

    def finish(self):
        self.stop_parent()
        stdout, stderr = self.helper.communicate(timeout=30)
        return self.helper.returncode, stdout, stderr

    def test_the_helper_should_wait_for_the_running_process_before_replacing_it(self):
        self.launch()
        self.assertEqual(self.version(), 'old')
        self.assertIsNone(self.helper.poll())

    def test_a_successful_update_should_replace_and_relaunch_the_install(self):
        self.launch()
        result = self.finish()
        self.assertEqual(result[0], 0, result)
        self.assertEqual(self.version(), 'new')
        self.assertEqual(self.launched.read_text(), 'new')
        self.assertFalse(self.work.exists())

    def test_a_failed_replacement_should_restore_and_relaunch_the_previous_install(self):
        self.launch()
        if self.staged.is_dir():
            shutil.rmtree(self.staged)
        else:
            self.staged.unlink()
        self.assertNotEqual(self.finish()[0], 0)
        self.assertEqual(self.version(), 'old')
        self.assertEqual(self.launched.read_text(), 'old')
        self.assertTrue(self.error.exists())

    def test_a_preflight_failure_should_not_relaunch_or_leave_a_stale_error(self):
        if self.staged.is_dir():
            shutil.rmtree(self.staged)
        else:
            self.staged.unlink()
        self.launch(expect_ready=False)
        self.helper.communicate(timeout=15)
        self.assertNotEqual(self.helper.returncode, 0)
        self.assertIsNone(self.parent.poll())
        self.assertFalse(self.launched.exists())
        self.assertFalse(self.error.exists())
        self.assertFalse((self.work / 'ready').exists())

    def test_a_failed_error_report_should_not_prevent_rollback(self):
        self.error.mkdir()
        self.launch(fail_launch=True)
        self.assertNotEqual(self.finish()[0], 0)
        self.assertEqual(self.version(), 'old')
        self.assertEqual(self.launched.read_text(), 'old')

    def test_a_failed_relaunch_should_restore_the_previous_install(self):
        self.launch(fail_launch=True)
        self.assertNotEqual(self.finish()[0], 0)
        self.assertEqual(self.version(), 'old')
        self.assertEqual(self.launched.read_text(), 'old')
        self.assertTrue(self.error.exists())


if __name__ == '__main__':
    unittest.main()
