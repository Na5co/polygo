#!/usr/bin/env python3
"""G6.5 acceptance: word counts + a second model rates each launch draft.

Question to the judge: "Would a maintainer of that community remove this as
spam?" scored 1–5 where 5 = clearly not. Every draft must score ≥ 4.
"""
import json
import re
import sys
import urllib.request

MODEL = sys.argv[1] if len(sys.argv) > 1 else "gemma4"
MAX_WORDS = 300
MIN_SCORE = 4

DRAFTS = [
    ("launch/reddit-iosprogramming.md", "the r/iOSProgramming subreddit (iOS/macOS developers)"),
    ("launch/reddit-flutterdev.md", "the r/FlutterDev subreddit"),
    ("launch/reddit-androiddev.md", "the r/androiddev subreddit"),
    ("launch/reddit-reactjs.md", "the r/reactjs subreddit"),
    ("launch/show-hn.md", "Hacker News (a Show HN submission with the author's first comment)"),
    ("launch/terminal-trove.md", "Terminal Trove, a curated directory of CLI tools"),
    ("launch/awesome-lists.md", "GitHub awesome-* curated lists (entries + PR body)"),
]

SYSTEM = (
    "You moderate developer communities. Judge the submission below strictly by that "
    "community's norms: self-promotion is fine when it is on-topic, concrete, honest about "
    "limitations, free/open-source, and invites discussion; it is spam when it is generic "
    "marketing, off-topic, dishonest, begs for stars/upvotes, or hides that it is a product. "
    'Reply with JSON only: {"score": <1-5>, "reason": "<one sentence>"} where 5 = a '
    "maintainer would clearly NOT remove it as spam and 1 = clearly spam."
)


def body(path: str) -> str:
    text = open(path, encoding="utf-8").read()
    m = re.search(r"^\*\*Body:\*\*\s*$|^## First comment.*$", text, re.M)
    return text[m.end():].strip() if m else text


def ask(community: str, text: str) -> dict:
    req = urllib.request.Request(
        "http://127.0.0.1:11434/api/chat",
        data=json.dumps({
            "model": MODEL,
            "stream": False,
            "think": False,
            "format": {"type": "object", "properties": {"score": {"type": "integer"}, "reason": {"type": "string"}}, "required": ["score", "reason"]},
            "options": {"temperature": 0},
            "messages": [
                {"role": "system", "content": SYSTEM},
                {"role": "user", "content": f"Community: {community}\n\nSubmission:\n\n{text}"},
            ],
        }).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=600) as r:
        return json.loads(json.load(r)["message"]["content"])


def main() -> int:
    failures = 0
    for path, community in DRAFTS:
        text = open(path, encoding="utf-8").read()
        post = body(path)
        words = len(post.split())
        too_long = path.startswith("launch/reddit") or path == "launch/show-hn.md"
        if too_long and words > MAX_WORDS:
            print(f"FAIL  {path}: {words} words > {MAX_WORDS}")
            failures += 1
            continue
        verdict = ask(community, text)
        ok = int(verdict.get("score", 0)) >= MIN_SCORE
        failures += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {path}  {words}w  score {verdict.get('score')}/5: {verdict.get('reason')}")
    print(f"\n{len(DRAFTS) - failures}/{len(DRAFTS)} drafts pass ({MODEL}, min score {MIN_SCORE})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
