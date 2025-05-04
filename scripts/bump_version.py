"""Advance the single package version in Cargo.toml and its Cargo.lock entry."""
import argparse
import re
from pathlib import Path


def bump(directory: Path, part: str) -> str:
    manifest_path = directory / "Cargo.toml"
    lock_path = directory / "Cargo.lock"
    manifest, lock = manifest_path.read_text(encoding="utf-8"), lock_path.read_text(encoding="utf-8")
    match = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"$', manifest, re.M)
    if not match:
        raise ValueError("Expected a stable package version in Cargo.toml")
    version = [int(n) for n in match.groups()]
    index = {"major": 0, "minor": 1, "patch": 2}[part]
    version[index] += 1
    version[index + 1:] = [0] * (2 - index)
    if any(n > 65535 for n in version):
        raise ValueError("Windows version fields must fit in 16 bits")
    new = ".".join(map(str, version))
    lock_pattern = r'(\[\[package\]\]\nname = "file-organizer"\nversion = ")[^"]+("\n)'
    updated_lock, count = re.subn(lock_pattern, lambda m: m[1] + new + m[2], lock)
    if count != 1:
        raise ValueError("Expected exactly one FileOrganizer package in Cargo.lock")
    updated_manifest = manifest[:match.start()] + f'version = "{new}"' + manifest[match.end():]
    manifest_path.write_text(updated_manifest, encoding="utf-8")
    lock_path.write_text(updated_lock, encoding="utf-8")
    return new


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("part", choices=["patch", "minor", "major"])
    args = parser.parse_args()
    print(bump(Path(__file__).resolve().parents[1], args.part))
