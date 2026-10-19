#!/usr/bin/env python3
"""Report only public, allowlisted test names and states; never annotate raw logs."""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def failed_test_names(text, allowed):
    names = set()
    for name in re.findall(r"test ([A-Za-z0-9_:]+) \.\.\. FAILED", text):
        short = name.split("::")[-1]
        if short in allowed:
            names.add(short)
    return sorted(names)


def broker_panic_lines(text):
    # Only numeric locations in this public, fixed source file. Never raw logs.
    return sorted({int(n) for n in re.findall(
        r"^thread '[A-Za-z0-9_:]+'(?: \([0-9]+\))? panicked at tests/video_broker\.rs:([0-9]{1,5}):[0-9]+:",
        text, re.MULTILINE)})


def main():
    commands = {
        "portable": ["cargo", "test", "--locked", "--workspace", "--no-fail-fast"],
        "arm": [sys.executable, "tests/mcp_real_smoke.py"],
        "guest": ["cargo", "test", "--locked", "-p", "freelane-agent-protocol", "-p", "freelane-guest-agent"],
    }
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        raise SystemExit("Use portable, arm or guest")
    allowed = set()
    for directory in (ROOT / "tests", ROOT / "crates"):
        for file in directory.rglob("*.rs"):
            allowed.update(re.findall(r"\bfn ([A-Za-z0-9_]+)\s*\(", file.read_text()))
    process = subprocess.Popen(commands[sys.argv[1]], cwd=ROOT, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT)
    failed = set()
    error_codes = set()
    broker_lines = set()
    # Preserve the existing command's console output; new annotations contain names only.
    for line in iter(process.stdout.readline, b""):
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
        text = line.decode(errors="replace")
        failed.update(failed_test_names(text, allowed))
        broker_lines.update(broker_panic_lines(text))
        error_codes.update(int(n) for n in re.findall(r"\(os error ([0-9]{1,3})\)", text) if 0 < int(n) <= 133)
    status = process.wait()
    process.stdout.close()
    if status:
        for line in sorted(broker_lines):
            print("::error title=Video broker source location::tests/video_broker.rs line " + str(line))
        if error_codes:
            print("::error title=OS error status::Codes observed: " + ", ".join(map(str, sorted(error_codes))))
        for name in sorted(failed):
            print("::error title=Test status::FAILED: " + name)
        if not failed:
            print("::error title=Test status::Check failed; no allowlisted Rust failure name available")
    return status if status >= 0 else 128 - status


if __name__ == "__main__":
    raise SystemExit(main())
