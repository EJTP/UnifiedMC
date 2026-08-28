#!/usr/bin/env python3
"""Write the manifest the in-app updater reads.

Every platform's build signs its own updater artifact and uploads the `.sig` beside it. This
collects those into the one file the updater fetches, which is why it can only run once all
three builds have finished rather than each writing its own third.

Usage: latest_json.py <tag> <directory of downloaded .sig files and notes.md> <output path>
"""

import json
import pathlib
import sys
import urllib.parse
from datetime import datetime, timezone

REPO = "EJTP/UnifiedMC"

# What each platform's updater installs, by the suffix of the artifact it wants. Tauri names
# these keys; they are not ours to choose.
WANTED = [
    ("linux-x86_64", ".AppImage"),
    # One universal bundle serves both Macs, so both keys point at the same file.
    ("darwin-x86_64", ".app.tar.gz"),
    ("darwin-aarch64", ".app.tar.gz"),
    # Tauri 2 signs the NSIS installer itself and its updater installs that. There is no
    # "-setup.nsis.zip" - that was Tauri 1's format, and asking for it is how v0.1.9 shipped
    # a manifest with no Windows entry at all.
    ("windows-x86_64", "-setup.exe"),
]


def main() -> int:
    tag, work, out = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
    base = f"https://github.com/{REPO}/releases/download/{urllib.parse.quote(tag)}"

    # A signature file is named after the artifact it signs, plus ".sig".
    sigs = {
        path.name[: -len(".sig")]: path.read_text().strip()
        for path in work.glob("*.sig")
    }

    platforms = {}
    for key, suffix in WANTED:
        match = next((name for name in sorted(sigs) if name.endswith(suffix)), None)
        if match is None:
            # Left out rather than guessed at: an entry pointing at a file that was never
            # uploaded fails every client on that platform, which is worse than offering
            # them nothing and letting the release page serve them.
            print(f"  {key}: nothing ending in {suffix} was signed - no entry")
            continue
        platforms[key] = {
            "signature": sigs[match],
            "url": f"{base}/{urllib.parse.quote(match)}",
        }
        print(f"  {key}: {match}")

    if not platforms:
        print("nothing was signed; refusing to publish an empty manifest", file=sys.stderr)
        return 1

    notes_file = work / "notes.md"
    notes = notes_file.read_text().strip() if notes_file.exists() else ""

    out.write_text(
        json.dumps(
            {
                "version": tag.lstrip("v"),
                # The updater shows this verbatim in a dialog, so it is trimmed rather than
                # rendered - a release body can be very long.
                "notes": notes[:1000],
                "pub_date": datetime.now(timezone.utc).isoformat(timespec="seconds"),
                "platforms": platforms,
            },
            indent=2,
        )
        + "\n"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
