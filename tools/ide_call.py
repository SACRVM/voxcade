#!/usr/bin/env python3
"""Call one tool of the IDE running on a game, from another session:

    python3 tools/ide_call.py GAME_DIR TOOL ['{"json": "arguments"}']

It speaks MCP to `voxcade mcp --remote` in the game folder, which passes
the call on to the IDE there (`voxcade ide GAME_DIR --remote`). Tools:
view_state, open, game_frame, errors, play, stop, eval, and with --remote
console_send (a prompt for the agent in the IDE's console) and
console_read (what the console shows). A picture is written beside the
game's folder name in the current directory and its path printed.
"""
import base64
import json
import os
import subprocess
import sys

if len(sys.argv) < 3:
    sys.exit(__doc__)
game, tool = sys.argv[1], sys.argv[2]
args = json.loads(sys.argv[3]) if len(sys.argv) > 3 else {}
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
exe = os.environ.get("VOXCADE", os.path.join(root, "target", "release", "voxcade"))
messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "ide_call", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": tool, "arguments": args}},
]
text = "\n".join(json.dumps(m) for m in messages) + "\n"
out = subprocess.run([exe, "mcp", "--remote"], cwd=game, input=text, capture_output=True, text=True, timeout=180)
failed = True
for line in out.stdout.splitlines():
    try:
        answer = json.loads(line)
    except ValueError:
        continue
    if answer.get("id") != 2:
        continue
    if "error" in answer:
        print("error:", answer["error"].get("message", answer["error"]))
        continue
    result = answer["result"]
    failed = bool(result.get("isError"))
    for part in result.get("content", []):
        if part.get("type") == "text":
            print(("refused: " if failed else "") + part["text"])
        elif part.get("type") == "image":
            path = os.path.abspath(f"{os.path.basename(os.path.normpath(game))}-{tool}.png")
            with open(path, "wb") as f:
                f.write(base64.b64decode(part["data"]))
            print(path)
if out.stderr.strip():
    print(out.stderr.strip(), file=sys.stderr)
sys.exit(1 if failed else 0)
