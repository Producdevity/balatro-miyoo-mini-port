#!/usr/bin/env python3
"""Collect license notices from the locked ARM dependency graph."""

import argparse
import json
from pathlib import Path
import subprocess


def collect(root):
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version=1",
        "--filter-platform", "armv7-unknown-linux-musleabihf",
    ], cwd=root, text=True))
    active = {node["id"] for node in metadata["resolve"]["nodes"]}
    packages = sorted(
        (p for p in metadata["packages"] if p["source"] and p["id"] in active),
        key=lambda p: (p["name"], p["version"]),
    )
    by_name = {p["name"]: p for p in packages}
    sections = ["Rust dependencies\n\nSources and notices for the locked ARM build.\n"]
    for package in packages:
        directory = Path(package["manifest_path"]).parent
        notices = sorted(p for p in directory.iterdir() if p.is_file() and
                         p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")))
        if package.get("license_file"):
            notices.append(directory / package["license_file"])
        if not notices and package["name"] == "mlua-sys":
            # The mlua-sys crate omits the shared repository license from its tarball.
            notices = [Path(by_name["mlua"]["manifest_path"]).parent / "LICENSE"]
        if not notices:
            raise ValueError(f"No license text found for {package['name']} {package['version']}")
        title = f"{package['name']} {package['version']}"
        sections.append(f"\n{title}\n{'=' * len(title)}\n"
                        f"License: {package['license']}\n"
                        f"Source: https://crates.io/crates/{package['name']}/{package['version']}\n")
        for notice in dict.fromkeys(notices):
            sections.append(f"\n{notice.name}\n\n{notice.read_text(encoding='utf-8').rstrip()}\n")
    return "".join(sections)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check the committed notices")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    destination = root / "licenses" / "Rust.txt"
    notices = collect(root)
    if args.check:
        if not destination.exists() or destination.read_text(encoding="utf-8") != notices:
            parser.exit(1, "Run python3 scripts/collect-licenses.py to update licenses/Rust.txt\n")
    else:
        destination.write_text(notices, encoding="utf-8")


if __name__ == "__main__":
    main()
