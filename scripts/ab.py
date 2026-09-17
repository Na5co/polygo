#!/usr/bin/env python3
"""G4.4 A/B harness: does polygo's context retrieval improve translations?

Takes a real repo containing an .xcstrings catalog. For N sampled keys that already
have a human translation in the target locale, the translation is removed in two
temp copies of the repo; copy A runs `polygo translate` with context, copy B with
--no-context (same model). A second model judges each pair blind. Results land in
bench/ab-<locale>.json.

Usage: scripts/ab.py <repo> <path/to/Localizable.xcstrings> <locale> [--n 40] [--model qwen3:8b] [--judge gemma4]
"""
import argparse, json, os, random, re, shutil, subprocess, sys, tempfile, time, urllib.request

OLLAMA = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.environ.get("POLYGO_BIN", os.path.join(ROOT, "target", "release", "polygo"))

JUDGE_SYSTEM = (
    "You are a senior {dst} localization reviewer for consumer apps. You will see an English UI string, "
    "where it is used in the app, and two candidate {dst} translations, A and B. Judge which one a native speaker "
    "would prefer to ship: natural, idiomatic, correct meaning for that UI element, consistent app tone, placeholders "
    "intact. Answer with exactly one token: A, B, or TIE."
)

def chat(model, system, user, timeout=180):
    body = json.dumps({"model": model, "stream": False, "think": False, "options": {"temperature": 0.0},
                       "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]}).encode()
    req = urllib.request.Request(f"{OLLAMA}/api/chat", data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)["message"]["content"].strip()

def strip_locale(doc, keys, locale):
    for k in keys:
        doc["strings"][k]["localizations"].pop(locale, None)

def run_polygo(copy, locale, no_context, model):
    cfg = os.path.join(copy, "polygo.toml")
    subprocess.run([BIN, "init", "--force"], cwd=copy, check=True, capture_output=True)
    text = open(cfg).read()
    text = re.sub(r'model = "[^"]*"', f'model = "{model}"', text)
    open(cfg, "w").write(text)
    args = [BIN, "translate", "--locale", locale]
    if no_context:
        args.append("--no-context")
    t0 = time.time()
    p = subprocess.run(args, cwd=copy, capture_output=True, text=True)
    if p.returncode not in (0, 3):
        sys.exit(f"polygo failed in {copy}:\n{p.stderr}")
    return time.time() - t0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("repo"); ap.add_argument("xcstrings"); ap.add_argument("locale")
    ap.add_argument("--n", type=int, default=40); ap.add_argument("--seed", type=int, default=11)
    ap.add_argument("--model", default="qwen3:8b"); ap.add_argument("--judge", default="gemma4")
    a = ap.parse_args()

    rel = os.path.relpath(os.path.abspath(a.xcstrings), os.path.abspath(a.repo))
    doc = json.load(open(a.xcstrings, encoding="utf-8"))
    src_lang = doc.get("sourceLanguage", "en")
    eligible = []
    for k, e in doc["strings"].items():
        locs = e.get("localizations") or {}
        s = locs.get(src_lang, {})
        t = locs.get(a.locale, {})
        if "stringUnit" in t and t["stringUnit"].get("value") and "variations" not in s and "variations" not in t:
            src = s.get("stringUnit", {}).get("value") or k
            if 3 <= len(src) <= 120 and "\n" not in src:
                eligible.append((k, src, t["stringUnit"]["value"]))
    rng = random.Random(a.seed)
    sample = rng.sample(eligible, min(a.n, len(eligible)))
    keys = [k for k, _, _ in sample]
    print(f"eligible={len(eligible)} sampled={len(sample)} locale={a.locale} model={a.model} judge={a.judge}")

    copies = {}
    for name, no_ctx in (("A_context", False), ("B_nocontext", True)):
        copy = tempfile.mkdtemp(prefix=f"polygo-ab-{name}-")
        shutil.copytree(a.repo, copy, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git", "polygo.lock", "polygo.toml"))
        d = json.load(open(os.path.join(copy, rel), encoding="utf-8"))
        strip_locale(d, keys, a.locale)
        json.dump(d, open(os.path.join(copy, rel), "w", encoding="utf-8"), ensure_ascii=False, indent=2)
        dt = run_polygo(copy, a.locale, no_ctx, a.model)
        out = json.load(open(os.path.join(copy, rel), encoding="utf-8"))
        copies[name] = {k: out["strings"][k].get("localizations", {}).get(a.locale, {}).get("stringUnit", {}).get("value") for k in keys}
        print(f"{name}: translated in {dt:.0f}s, {sum(1 for v in copies[name].values() if v)} / {len(keys)} written")

    # Blind judging.
    results = []
    wins = losses = ties = 0
    for k, src, human in sample:
        ta, tb = copies["A_context"].get(k), copies["B_nocontext"].get(k)
        if not ta or not tb:
            results.append({"key": k, "source": src, "human": human, "context": ta, "nocontext": tb, "verdict": "missing"})
            continue
        flip = rng.random() < 0.5
        first, second = (tb, ta) if flip else (ta, tb)
        user = f"English: {src}\n\nA: {first}\nB: {second}\n\nAnswer A, B, or TIE."
        try:
            out = chat(a.judge, JUDGE_SYSTEM.format(dst=a.locale), user).upper()
        except Exception as e:  # noqa: BLE001
            out = "TIE"
        m = re.search(r"\b(A|B|TIE)\b", out)
        v = m.group(1) if m else "TIE"
        if v == "TIE":
            verdict = "tie"; ties += 1
        elif (v == "A") != flip:
            verdict = "context"; wins += 1
        else:
            verdict = "nocontext"; losses += 1
        results.append({"key": k, "source": src, "human": human, "context": ta, "nocontext": tb, "verdict": verdict})
        print(f"  {verdict:9} {src[:40]!r:44} ctx={ta[:40]!r} noctx={tb[:40]!r}")

    os.makedirs(os.path.join(ROOT, "bench"), exist_ok=True)
    summary = {"locale": a.locale, "model": a.model, "judge": a.judge, "n": len(sample),
               "context_wins": wins, "nocontext_wins": losses, "ties": ties, "results": results}
    out_path = os.path.join(ROOT, "bench", f"ab-{a.locale}.json")
    json.dump(summary, open(out_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    decided = wins + losses
    rate = wins / decided if decided else 0.0
    print(f"\ncontext wins={wins} nocontext wins={losses} ties={ties}  win rate among decided: {rate:.0%}\nwrote {out_path}")

if __name__ == "__main__":
    main()
