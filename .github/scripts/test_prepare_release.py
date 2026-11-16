"""Exercise release preparation against a remote that rejects direct pushes to main."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("prepare-release.sh").resolve()
FAKE_GH = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys

args = sys.argv[1:]
root = Path(os.environ["RELEASE_FIXTURE"])
state_path = root / "pr.json"
with (root / "calls.jsonl").open("a") as file:
    file.write(json.dumps(args) + "\n")

def git(*args, input=None):
    return subprocess.check_output(
        ["git", "--git-dir", str(root / "origin.git"), *args],
        input=input, text=True,
    ).strip()

def change_application(tree):
    blob = git("hash-object", "-w", "--stdin", input="concurrent change\n")
    entries = git("ls-tree", tree).splitlines()
    entries = [f"100644 blob {blob}\tapplication.rs" if line.endswith("\tapplication.rs")
               else line for line in entries]
    return git("mktree", input="\n".join(entries) + "\n")

if args[:2] == ["pr", "create"]:
    assert args[args.index("--base") + 1] == "main"
    assert args[args.index("--label") + 1] == "skip-changelog"
    branch = args[args.index("--head") + 1]
    state_path.write_text(json.dumps({"branch": branch, "state": "OPEN"}))
    print("https://github.com/example/matrix/pull/1")
elif args[:2] == ["pr", "merge"]:
    if os.environ.get("RELEASE_MERGE_BLOCKED"):
        sys.exit("Changes must satisfy branch rules")
    if os.environ.get("RELEASE_MERGE_PENDING"):
        sys.exit(0)
    state = json.loads(state_path.read_text())
    head = git("rev-parse", "refs/heads/" + state["branch"])
    assert args[args.index("--match-head-commit") + 1] == head
    assert "--squash" in args and "--delete-branch" in args
    parent = git("rev-parse", "refs/heads/main")
    tree = git("rev-parse", head + "^{tree}")
    if os.environ.get("RELEASE_MAIN_ADVANCES"):
        main_tree = change_application(parent + "^{tree}")
        parent = git("commit-tree", main_tree, "-p", parent, "-m", "Concurrent main change")
        git("update-ref", "refs/heads/main", parent)
        tree = change_application(tree)
    sha = git("commit-tree", tree, "-p", parent, "-m", "Release v0.0.5 (#1)")
    # Server-side PR merge, rather than a push, is allowed to update protected main.
    git("update-ref", "refs/heads/main", sha, parent)
    git("update-ref", "-d", "refs/heads/" + state["branch"])
    state.update(state="MERGED", sha=sha)
    state_path.write_text(json.dumps(state))
elif args[:2] == ["pr", "view"]:
    state = json.loads(state_path.read_text())
    if state["state"] == "MERGED":
        print(state["sha"])
else:
    sys.exit("Unexpected gh call: " + repr(args))
'''


class PrepareReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.origin = self.root / "origin.git"
        self.work = self.root / "work"
        self.run_git("init", "--bare", "--initial-branch=main", str(self.origin), cwd=self.root)
        self.run_git("clone", str(self.origin), str(self.work), cwd=self.root)
        self.run_git("config", "user.name", "Release test")
        self.run_git("config", "user.email", "release@example.org")
        (self.work / "Cargo.toml").write_text('[workspace.package]\nversion = "0.0.4"\n')
        (self.work / "Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "better-matrix-desktop"\nversion = "0.0.4"\n'
            '\n[[package]]\nname = "matrix-core"\nversion = "0.0.4"\n'
            '\n[[package]]\nname = "dependency"\nversion = "0.0.4"\n'
        )
        (self.work / "application.rs").write_text("original code\n")
        self.run_git("add", ".")
        self.run_git("commit", "-m", "Application changes")
        self.run_git("push", "origin", "main")
        self.base = self.run_git("rev-parse", "HEAD")
        hook = self.origin / "hooks" / "update"
        hook.write_text('#!/bin/sh\nif [ "$1" = refs/heads/main ]; then\n'
                        '  echo "GH013: Changes must be made through a pull request" >&2\n'
                        '  exit 1\nfi\n')
        hook.chmod(0o755)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        (self.bin / "gh").write_text(FAKE_GH)
        (self.bin / "gh").chmod(0o755)
        self.output = self.root / "output"

    def run_git(self, *args, cwd=None):
        return subprocess.check_output(
            ["git", *args], cwd=cwd or self.work, text=True, stderr=subprocess.PIPE,
        ).strip()

    def prepare(self, **overrides):
        self.output.write_text("")
        env = dict(os.environ, VERSION="0.0.5", GITHUB_REPOSITORY="example/matrix",
                   GITHUB_RUN_ID="123", GITHUB_RUN_ATTEMPT="1", GITHUB_OUTPUT=str(self.output),
                   RELEASE_FIXTURE=str(self.root), PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                   GIT_AUTHOR_NAME="Release test", GIT_AUTHOR_EMAIL="release@example.org",
                   GIT_COMMITTER_NAME="Release test", GIT_COMMITTER_EMAIL="release@example.org")
        env.update(overrides)
        return subprocess.run(["bash", str(SCRIPT)], cwd=self.work, env=env,
                              capture_output=True, text=True)

    def outputs(self):
        return dict(line.split("=", 1) for line in self.output.read_text().splitlines())

    def calls(self):
        path = self.root / "calls.jsonl"
        return [json.loads(line)[:2] for line in path.read_text().splitlines()] if path.exists() else []

    def assert_succeeded(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_new_version_should_reach_protected_main_through_a_pr(self):
        self.assert_succeeded(self.prepare())
        outputs = self.outputs()
        main = self.run_git("ls-remote", "origin", "refs/heads/main").split()[0]
        self.assertEqual(outputs, {"tag": "v0.0.5", "sha": main, "base": self.base})
        self.assertNotEqual(outputs["sha"], self.run_git("rev-parse", "HEAD"))
        self.assertEqual(self.calls(), [["pr", "create"], ["pr", "merge"], ["pr", "view"]])
        manifest = self.run_git("show", main + ":Cargo.toml")
        lock = self.run_git("show", main + ":Cargo.lock")
        self.assertIn('version = "0.0.5"', manifest)
        self.assertEqual(lock.count('version = "0.0.5"'), 2)
        self.assertIn('name = "dependency"\nversion = "0.0.4"', lock)
        self.assertEqual(self.run_git("ls-remote", "--heads", "origin", "release/*"), "")

    def test_a_prerelease_should_update_the_version_without_making_a_tag(self):
        self.assert_succeeded(self.prepare(VERSION="0.0.5-rc.1"))
        outputs = self.outputs()
        self.assertEqual(outputs["tag"], "v0.0.5-rc.1")
        self.assertIn('version = "0.0.5-rc.1"', self.run_git("show", outputs["sha"] + ":Cargo.toml"))
        self.assertEqual(self.run_git("ls-remote", "--tags", "origin"), "")

    def test_a_retry_after_merging_should_not_create_another_version_pr(self):
        self.assert_succeeded(self.prepare())
        merged = self.outputs()["sha"]
        self.run_git("reset", "--hard", merged)
        calls = self.calls()
        self.assert_succeeded(self.prepare(GITHUB_RUN_ATTEMPT="2"))
        self.assertEqual(self.outputs(), {"tag": "v0.0.5", "sha": merged, "base": self.base})
        self.assertEqual(self.calls(), calls)

    def test_changes_merged_to_main_during_preparation_should_need_their_own_tests(self):
        self.assert_succeeded(self.prepare(RELEASE_MAIN_ADVANCES="1"))
        outputs = self.outputs()
        self.assertEqual(outputs["base"], outputs["sha"])
        self.assertEqual(self.run_git("show", outputs["sha"] + ":application.rs"), "concurrent change")

    def test_a_blocked_merge_should_stop_without_releasing_the_branch_commit(self):
        result = self.prepare(RELEASE_MERGE_BLOCKED="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.outputs(), {})
        self.assertEqual(self.run_git("ls-remote", "origin", "refs/heads/main").split()[0], self.base)

    def test_a_merge_that_is_only_queued_should_not_start_a_release(self):
        result = self.prepare(RELEASE_MERGE_PENDING="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("The version PR has not merged", result.stdout)
        self.assertEqual(self.outputs(), {})

    def test_an_existing_tag_should_stop_before_creating_a_pr(self):
        self.run_git("--git-dir", str(self.origin), "tag", "v0.0.5")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("v0.0.5 exists already", result.stdout)
        self.assertEqual(self.calls(), [])

    def test_an_invalid_version_should_leave_the_workspace_unchanged(self):
        for version in ("v0.0.5", "0.00.5", "0.0.5-01", "0.0.5-", "0.0.5;exit 0"):
            with self.subTest(version=version):
                self.assertNotEqual(self.prepare(VERSION=version).returncode, 0)
                self.assertEqual(self.run_git("status", "--porcelain"), "")
                self.assertEqual(self.calls(), [])

    def test_a_remote_error_should_stop_before_changing_the_version(self):
        self.run_git("remote", "set-url", "origin", str(self.root / "missing.git"))
        self.assertNotEqual(self.prepare().returncode, 0)
        self.assertEqual(self.run_git("status", "--porcelain"), "")
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
