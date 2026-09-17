#!/usr/bin/env python3
"""Ask a second model a fixed rubric about README.md and grade the answers.

Each rubric question has a list of regexes; the answer passes when every one
matches (case-insensitive). The README is the model's only source.
"""
import json
import re
import sys
import urllib.request

MODEL = sys.argv[1] if len(sys.argv) > 1 else "gemma4"
README = open("README.md", encoding="utf-8").read()

RUBRIC = [
    (
        "What is polygo, in one or two sentences?",
        [r"locali[sz]ation|translat", r"\bcli\b|command[- ]line|binary", r"local|offline|ollama"],
    ),
    (
        "Who is it for? Name the kind of person or team and the kind of projects.",
        [r"solo|indie|one person|individual|single developer|small team", r"\bapp|project|software"],
    ),
    (
        "What is the single shell command to install it? Reply with the command only.",
        [r"curl -fsSL https://raw\.githubusercontent\.com/Na5co/polygo/main/install\.sh \| sh|brew install na5co/tap/polygo|cargo (bin)?install polygo"],
    ),
    (
        "Which localization file formats does it support? List all of them.",
        [r"xcstrings", r"android|strings\.xml", r"json|i18next", r"\barb\b|flutter", r"\bpo\b|gettext", r"resx|resw|\.net"],
    ),
    (
        "Does polygo phone home (send telemetry, analytics or your strings to the author's servers)? Where do network requests go?",
        [r"\bno\b|does not|doesn't|never", r"provider|ollama|localhost|127\.0\.0\.1|endpoint you configure"],
    ),
]

SYSTEM = (
    "You are grading a software README. Answer ONLY from the README text provided. "
    "Be concrete and brief; quote commands and file formats exactly as written."
)


def ask(question: str) -> str:
    body = {
        "model": MODEL,
        "stream": False,
        "think": False,
        "options": {"temperature": 0, "num_ctx": 8192},  # default 4096 silently truncates the README
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": f"README.md:\n\n{README}\n\n---\nQuestion: {question}"},
        ],
    }
    req = urllib.request.Request(
        "http://127.0.0.1:11434/api/chat",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=600) as r:
        return json.load(r)["message"]["content"].strip()


def main() -> int:
    failures = 0
    for q, patterns in RUBRIC:
        a = ask(q)
        missing = [p for p in patterns if not re.search(p, a, re.I | re.S)]
        ok = not missing
        failures += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {q}")
        print("      " + a.replace("\n", "\n      ")[:700])
        if missing:
            print(f"      missing: {missing}")
    print(f"\n{len(RUBRIC) - failures}/{len(RUBRIC)} answered correctly ({MODEL})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
