#!/usr/bin/env python3
"""The licences of everything built into the voxcade program, as one text
to ship beside it (THIRD-PARTY-LICENSES.txt in every build's folder): the
crates it links for one system, with SDL, Lua and the IDE's fonts that
come inside them, and what the games folder carries from others.

Most crates are MIT or Apache-2.0 (often either: then MIT is taken, the
shorter); their licences ask that the notice travel with every copy. Each
crate's own licence file is used where it has one; else the licence's
text (MIT with the crate's authors). Identical texts are written once,
with every crate under them.

    python3 tools/third_party_licenses.py --target x86_64-unknown-linux-gnu \\
        --features static-sdl > THIRD-PARTY-LICENSES.txt

Needs the crates' sources (a build or `cargo fetch --target ...` first).
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

PREFER = ["MIT", "Apache-2.0", "Zlib", "BSD-3-Clause", "BSD-2-Clause", "ISC", "0BSD", "Unlicense",
          "CC0-1.0", "BSL-1.0", "Unicode-3.0", "Unicode-DFS-2016", "CDLA-Permissive-2.0", "MPL-2.0",
          "OFL-1.1", "Ubuntu-font-1.0", "LGPL-2.1-or-later"]

MIT = """Copyright (c) {who}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

# Crates that build a C library into the program from their sources (as
# build dependencies, so not among the linked crates): SDL3 and Lua.
VENDORED = {"sdl3-src", "lua-src"}

# Licences inside a crate that its own licence file does not cover: the
# libraries and fonts it carries.
CARRIED = {
    "sdl3-src": ["SDL/LICENSE.txt"],
    "epaint_default_fonts": ["fonts/*.txt"],
}

# What the games folder carries from others (not in the program).
GAMES = """Sokovox's puzzles are Microban by David W. Skinner, passed on with his
credit as he asks (games/sokovox/levels/).

Several songs are composed from chord progressions of free-midi-chords by
Ludovic Drolez (MIT License).

The big letters of the games' titles (ui/display_font.png) are drawn from
Fira Sans Compressed Heavy, SIL Open Font License 1.1."""


def classify(text):
    t = text.lower()
    if "unicode license v3" in t or "unicode license version 3" in t:
        return "Unicode-3.0"
    if "sil open font license" in t:
        return "OFL-1.1"
    if "ubuntu font licence" in t:
        return "Ubuntu-font-1.0"
    if "apache license" in t and "version 2.0" in t:
        return "Apache-2.0"
    if "permission is hereby granted, free of charge" in t:
        return "MIT"
    if "permission to use, copy, modify, and/or distribute this software" in t:
        return "ISC"
    if "redistribution and use in source and binary forms" in t:
        return "BSD-3-Clause" if "neither the name" in t else "BSD-2-Clause"
    if "provided 'as-is'" in t or "provided \"as-is\"" in t:
        return "Zlib"
    if "free and unencumbered software released into the public domain" in t:
        return "Unlicense"
    if "mozilla public license" in t and "2.0" in t:
        return "MPL-2.0"
    if "community data license agreement" in t:
        return "CDLA-Permissive-2.0"
    if "boost software license" in t:
        return "BSL-1.0"
    if "cc0" in t or "creative commons legal code" in t:
        return "CC0-1.0"
    if "gnu lesser general public license" in t:
        return "LGPL-2.1-or-later"
    return None


def parse(expr):
    """An SPDX expression as a tree: ("or"|"and", [...]) or an id."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))
    pos = 0

    def atom():
        nonlocal pos
        tok = tokens[pos]
        pos += 1
        if tok == "(":
            node = either()
            pos += 1   # )
            return node
        if pos < len(tokens) and tokens[pos] == "WITH":
            pos += 2   # an exception only widens the licence
        return tok

    def both():
        nonlocal pos
        parts = [atom()]
        while pos < len(tokens) and tokens[pos] == "AND":
            pos += 1
            parts.append(atom())
        return parts[0] if len(parts) == 1 else ("and", parts)

    def either():
        nonlocal pos
        parts = [both()]
        while pos < len(tokens) and tokens[pos] == "OR":
            pos += 1
            parts.append(both())
        return parts[0] if len(parts) == 1 else ("or", parts)

    return either()


def chosen(node):
    """The licences to follow: every one of an AND, the preferred of an OR."""
    if isinstance(node, str):
        return [node]
    kind, parts = node
    if kind == "and":
        return [x for p in parts for x in chosen(p)]
    options = [chosen(p) for p in parts]
    rank = lambda ids: max(PREFER.index(i) if i in PREFER else len(PREFER) for i in ids)
    return min(options, key=rank)


def licence_files(root):
    return sorted(p for p in root.iterdir()
                  if p.is_file() and re.match(r"(licen[cs]e|copying|notice|unlicense)", p.name, re.I))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True)
    ap.add_argument("--features", default="")
    args = ap.parse_args()
    cmd = ["cargo", "metadata", "--format-version", "1", "--filter-platform", args.target]
    if args.features:
        cmd += ["--features", args.features]
    meta = json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    me = packages[root]

    # What is linked: the root's normal dependencies, and theirs.
    seen, todo = set(), [root]
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
            elif packages[dep["pkg"]]["name"] in VENDORED:
                seen.add(dep["pkg"])

    # Reference texts by licence, from the crates' own files, for crates
    # that name a licence but carry no file of it.
    reference = {}
    for pid in sorted(seen):
        for f in licence_files(Path(packages[pid]["manifest_path"]).parent):
            kind = classify(f.read_text(errors="replace"))
            if kind and kind != "MIT" and kind not in reference:
                reference[kind] = f.read_text(errors="replace")

    texts = {}      # text -> [crate names]
    unknown = []
    for pid in sorted(seen, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        p = packages[pid]
        name = f'{p["name"]} {p["version"]}'
        base = Path(p["manifest_path"]).parent
        ids = chosen(parse(p.get("license") or "UNKNOWN"))
        files = [(f, f.read_text(errors="replace")) for f in licence_files(base)]
        carried = [(f, f.read_text(errors="replace")) for pattern in CARRIED.get(p["name"], [])
                   for f in sorted(base.glob(pattern))]
        for lid in ids:
            mine = [t for f, t in files if classify(t) == lid]
            if not mine and len(files) == 1 and len(ids) == 1 and classify(files[0][1]) is None:
                mine = [files[0][1]]
            if not mine and lid == "MIT":
                who = ", ".join(re.sub(r"\s*<[^>]*>", "", a) for a in p.get("authors") or []) or f'the {p["name"]} authors'
                mine = [MIT.format(who=who)]
            if not mine and any(classify(t) == lid for f, t in carried):
                continue   # written with the carried files below
            if not mine and lid in reference:
                mine = [reference[lid]]
            if not mine:
                mine = [f"{lid}: https://spdx.org/licenses/{lid}.html"]
                unknown.append(f"{name} ({lid})")
            for t in mine:
                texts.setdefault(t.strip(), []).append(f"{name} ({lid})")
        if "Apache-2.0" in ids:
            for f, t in files:
                if f.name.upper().startswith("NOTICE"):
                    texts.setdefault(t.strip(), []).append(f"{name} (NOTICE)")
        for f, t in carried:
            texts.setdefault(t.strip(), []).append(f"{name}: {f.relative_to(base)}")
        if p["name"] == "lua-src":
            # Lua itself (the 5.4 the program runs): its notice ends lua.h.
            for h in sorted(base.glob("lua-5.4*/lua.h"))[-1:]:
                notice = h.read_text(errors="replace")
                notice = notice[notice.rfind("Copyright (C)"):]
                notice = re.sub(r"^\* ?", "", notice.split("*****")[0], flags=re.M).strip()
                texts.setdefault(notice, []).append(f"Lua {h.parent.name[4:]} (inside {name})")

    out = [f"Voxcade {me['version']}: the licences of what is built into the program",
           f"({args.target}), and of what the games carry from others.", "",
           f"{len(seen)} libraries; each text below is followed by the libraries under it.", "",
           "=" * 78, "The games", "=" * 78, "", GAMES, ""]
    for text, names in sorted(texts.items(), key=lambda kv: kv[1][0].lower()):
        out += ["=" * 78] + names + ["=" * 78, "", text, ""]
    sys.stdout.write("\n".join(out))
    if unknown:
        print(f"no licence text found for: {', '.join(unknown)}", file=sys.stderr)


if __name__ == "__main__":
    main()
