#!/usr/bin/env python3
"""Puts a build's folder together, as the Builds workflow ships it: the
program (started without anything it opens the launcher), the games and
the Lua definitions, dist/README.txt and LIESMICH.txt, on Linux and
macOS a start script to double-click (a file manager runs a script
sooner than a program), and THIRD-PARTY-LICENSES.txt
(tools/third_party_licenses.py).

    python3 tools/package.py --system linux --target x86_64-unknown-linux-gnu \\
        --program target/x86_64-unknown-linux-gnu/release/voxcade --out voxcade-linux-x86_64

--from takes games/ and lua/ from another folder (an earlier build's, to
pack it anew with the same games).
"""
import argparse
import shutil
import stat
import subprocess
import sys
from pathlib import Path

SH = """#!/bin/sh
cd "$(dirname "$0")" || exit 1
exec ./voxcade
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--system", choices=["windows", "linux", "macos"], required=True)
    ap.add_argument("--target", required=True)
    ap.add_argument("--program", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--from", dest="source", default=".")
    ap.add_argument("--features", default="static-sdl")
    args = ap.parse_args()

    repo = Path(__file__).resolve().parent.parent
    out, source = Path(args.out), Path(args.source)
    out.mkdir(parents=True)
    # The Bench (a load test) and the Arena (the first test yard) are no
    # games to show: they stay in the source.
    shutil.copytree(source / "games", out / "games", ignore=shutil.ignore_patterns("bench", "arena", "shots", ".mcp.json"))
    shutil.copytree(source / "lua", out / "lua")
    for text in ["dist/README.txt", "dist/LIESMICH.txt", "LICENSE.txt"]:
        # Windows' own editor wants its line ends.
        (out / Path(text).name).write_text((repo / text).read_text(), newline="\r\n" if args.system == "windows" else "\n")
    program = Path(args.program)
    shutil.copy2(program, out / program.name)

    if args.system != "windows":
        script = out / ("start-voxcade.command" if args.system == "macos" else "start-voxcade.sh")
        script.write_text(SH)
        script.chmod(script.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    licences = subprocess.run([sys.executable, str(repo / "tools" / "third_party_licenses.py"),
                               "--target", args.target, "--features", args.features],
                              cwd=repo, capture_output=True, text=True, check=True)
    if licences.stderr:
        sys.exit(licences.stderr)
    (out / "THIRD-PARTY-LICENSES.txt").write_text(licences.stdout, newline="\r\n" if args.system == "windows" else "\n")
    print(f"{out}: " + ", ".join(sorted(p.name for p in out.iterdir())))


if __name__ == "__main__":
    main()
