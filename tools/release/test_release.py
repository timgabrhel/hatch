"""Release management tests use temporary manifests and mocked GitHub responses."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import complete
import version


class Versions(unittest.TestCase):
    def test_bump_uses_highest_stable_version(self):
        self.assertEqual(version.select_version("0.5.1", ["v0.6.2", "v9.0.0-rc1"], "patch"), "0.6.3")
        self.assertEqual(version.select_version("0.5.1", [], "minor"), "0.6.0")
        self.assertEqual(version.select_version("0.5.1", [], "major"), "1.0.0")
        self.assertEqual(version.select_version("0.5.1", [], "patch", "v1.2.3"), "1.2.3")
        for value in ["0.5.1", "v0.4.0", "v01.2.3", "1.0.0-rc1", "oops"]:
            with self.assertRaises(ValueError):
                version.select_version("0.5.1", [], "patch", value)

    def test_manifest_and_lock_change_together_without_dependency_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname = "hatch-cli"\nversion = "0.5.1"\n\n[dependencies]\nexample = "0.5.1"\n')
            dependency = '[[package]]\nname = "example"\nversion = "0.5.1"\n\n'
            (root / "Cargo.lock").write_text(dependency + '[[package]]\nname = "hatch-cli"\nversion = "0.5.1"\n')
            self.assertEqual(version.project_version(root), "0.5.1")
            version.write_version(root, "0.5.1", "0.6.0")
            self.assertEqual(version.project_version(root), "0.6.0")
            self.assertIn(dependency, (root / "Cargo.lock").read_text())
            self.assertIn('example = "0.5.1"', (root / "Cargo.toml").read_text())
            (root / "Cargo.lock").write_text(dependency)
            with self.assertRaises(ValueError):
                version.project_version(root)
            before = (root / "Cargo.toml").read_text()
            with self.assertRaises(ValueError):
                version.write_version(root, "0.6.0", "0.7.0")
            self.assertEqual((root / "Cargo.toml").read_text(), before)

    def test_merged_version_does_not_retag_or_downgrade(self):
        self.assertIsNone(version.merged_tag("0.5.1", "0.5.1", []))
        self.assertEqual(version.merged_tag("0.6.0", "0.5.1", []), "v0.6.0")
        for current, previous, tags in [("0.4.0", "0.5.1", []), ("0.6.0", "0.5.1", ["v1.0.0"])]:
            with self.assertRaises(ValueError):
                version.merged_tag(current, previous, tags)


class Merge(unittest.TestCase):
    repo = "example/hatch"
    sha = "a" * 40
    tag = "v0.6.0"
    branch = "release/v0.6.0"

    def pr(self):
        return {"state": "open", "draft": False,
                "base": {"ref": "main", "repo": {"full_name": self.repo}},
                "head": {"ref": self.branch, "repo": {"full_name": self.repo}, "sha": self.sha}}

    def test_changed_head_and_foreign_repository_are_rejected(self):
        for key, value in [("sha", "b" * 40), ("ref", "other"), ("repo", {"full_name": "other/hatch"})]:
            pr = self.pr()
            pr["head"][key] = value
            with self.assertRaises(ValueError):
                complete.validate_pr(pr, self.repo, self.branch, self.sha)

    @patch.object(complete, "wait_for_checks")
    @patch.object(complete, "gh")
    def test_only_version_files_merge_at_validated_sha(self, gh, checks):
        files = [[{"filename": "Cargo.toml"}, {"filename": "Cargo.lock"}]]
        gh.side_effect = [self.pr(), files, self.pr(), {"merged": True, "sha": "b" * 40}]
        self.assertEqual(complete.complete(self.repo, 12, self.branch, self.sha, self.tag), "b" * 40)
        checks.assert_called_once_with(self.repo, self.branch, self.sha)
        self.assertIn(f"sha={self.sha}", gh.call_args.args)
        files[0].append({"filename": "src/main.rs"})
        gh.reset_mock()
        gh.side_effect = [self.pr(), files]
        with self.assertRaises(ValueError):
            complete.complete(self.repo, 12, self.branch, self.sha, self.tag)
        self.assertEqual(gh.call_count, 2)

    @patch.object(complete, "gh")
    def test_failed_or_skipped_checks_cannot_merge(self, gh):
        runs = {"workflow_runs": [{"head_sha": self.sha, "id": 1, "html_url": "checks"}]}
        run = {"head_sha": self.sha, "event": "pull_request", "status": "completed", "conclusion": "failure"}
        gh.side_effect = [runs, run]
        with self.assertRaises(ValueError):
            complete.wait_for_checks(self.repo, self.branch, self.sha)
        run["conclusion"] = "success"
        gh.side_effect = [runs, run, {"jobs": [{"name": "check", "conclusion": "skipped"}]}]
        with self.assertRaises(ValueError):
            complete.wait_for_checks(self.repo, self.branch, self.sha)
        gh.side_effect = [runs, run, {"jobs": [{"name": "check", "conclusion": "success"}]}]
        complete.wait_for_checks(self.repo, self.branch, self.sha)


if __name__ == "__main__":
    unittest.main()
