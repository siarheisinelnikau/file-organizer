"""Version helper regressions on repository-local artificial files only."""
import tempfile
import unittest
from pathlib import Path

from bump_version import bump


class VersioningTests(unittest.TestCase):
    def fixture(self, directory: Path, version="0.2.7"):
        (directory / "Cargo.toml").write_text(f'[package]\nname = "file-organizer"\nversion = "{version}"\n')
        (directory / "Cargo.lock").write_text(f'[[package]]\nname = "file-organizer"\nversion = "{version}"\n\n[[package]]\nname = "dependency"\nversion = "9.8.7"\n')

    def test_patch_minor_major_keep_lockfile_and_other_packages_consistent(self):
        parent = Path(__file__).resolve().parents[1] / "target/test-fixtures"
        parent.mkdir(parents=True, exist_ok=True)
        for part, expected in [("patch", "0.2.8"), ("minor", "0.3.0"), ("major", "1.0.0")]:
            with self.subTest(part=part), tempfile.TemporaryDirectory(dir=parent) as temp:
                directory = Path(temp)
                self.fixture(directory)
                self.assertEqual(bump(directory, part), expected)
                self.assertIn(f'version = "{expected}"', (directory / "Cargo.toml").read_text())
                self.assertIn(f'name = "file-organizer"\nversion = "{expected}"', (directory / "Cargo.lock").read_text())
                self.assertIn('name = "dependency"\nversion = "9.8.7"', (directory / "Cargo.lock").read_text())

    def test_invalid_lockfile_does_not_partially_change_the_manifest(self):
        parent = Path(__file__).resolve().parents[1] / "target/test-fixtures"
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as temp:
            directory = Path(temp)
            self.fixture(directory)
            original = (directory / "Cargo.toml").read_bytes()
            (directory / "Cargo.lock").write_text('version = 4\n')
            with self.assertRaises(ValueError):
                bump(directory, "patch")
            self.assertEqual((directory / "Cargo.toml").read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
