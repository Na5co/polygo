#!/usr/bin/env python3
"""G0.1 kill test: does code context improve LLM translations of .xcstrings?

Translates N source strings twice with a local Ollama model:
  (a) key + source string only
  (b) key + source + code-usage context + up to 3 similar existing translations
and writes a blinded CSV (A/B order shuffled per row) plus a key file.

Usage:
  scripts/killtest.py <Localizable.xcstrings> <locale> [--repo ROOT] [--model qwen3:8b] [--n 30]
"""
import argparse
import csv
import json
import os
import random
import re
import sys
import time
import urllib.request

OLLAMA = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434")
SRC_EXT = (".swift", ".m", ".mm", ".kt", ".java", ".dart", ".ts", ".tsx", ".js", ".jsx")


def load_units(path):
    doc = json.load(open(path, encoding="utf-8"))
    src_lang = doc.get("sourceLanguage", "en")
    units = []
    for key, entry in doc.get("strings", {}).items():
        locs = entry.get("localizations", {}) or {}
        src = locs.get(src_lang, {}).get("stringUnit", {}).get("value") or key
        if "variations" in locs.get(src_lang, {}):
            continue  # skip plural/device variations for the kill test
        if not src or len(src) < 3 or len(src) > 120 or "\n" in src:
            continue
        comment = entry.get("comment", "")
        existing = {
            loc: v["stringUnit"]["value"]
            for loc, v in locs.items()
            if loc != src_lang and "stringUnit" in v and v["stringUnit"].get("value")
        }
        units.append({"key": key, "source": src, "comment": comment, "existing": existing})
    return src_lang, units


def find_usage(root, key, max_files=4000):
    """Return (relpath, enclosing identifier, snippet) for the first usage of key."""
    if not root:
        return None
    pat = re.compile(re.escape(key))
    ident_re = re.compile(
        r"^\s*(?:@\w+\s+)*(?:public |private |fileprivate |internal |static |override |final )*"
        r"(?:func|var|let|struct|class|enum|extension|protocol|init|case)\s+([A-Za-z_][\w]*)"
    )
    n = 0
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if not d.startswith(".") and d not in ("build", "node_modules", "Pods", "DerivedData")]
        for fn in filenames:
            if not fn.endswith(SRC_EXT):
                continue
            n += 1
            if n > max_files:
                return None
            p = os.path.join(dirpath, fn)
            try:
                lines = open(p, encoding="utf-8", errors="ignore").read().splitlines()
            except OSError:
                continue
            for i, line in enumerate(lines):
                if pat.search(line):
                    lo, hi = max(0, i - 6), min(len(lines), i + 7)
                    ident = ""
                    for j in range(i, -1, -1):
                        m = ident_re.match(lines[j])
                        if m:
                            ident = m.group(1)
                            break
                    snippet = "\n".join(lines[lo:hi])
                    return os.path.relpath(p, root), ident, snippet
    return None


def tokens(s):
    return set(re.findall(r"[a-z0-9]+", s.lower()))


def similar(units, unit, locale, k=3):
    cands = [u for u in units if u is not unit and locale in u["existing"]]
    t = tokens(unit["source"])
    scored = []
    for u in cands:
        tu = tokens(u["source"])
        if not tu or not t:
            continue
        j = len(t & tu) / len(t | tu)
        if j > 0:
            scored.append((j, u))
    scored.sort(key=lambda x: -x[0])
    return [u for _, u in scored[:k]]


def chat(model, system, user, timeout=180):
    body = json.dumps({
        "model": model,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "stream": False,
        "think": False,
        "options": {"temperature": 0.2, "num_predict": 200},
    }).encode()
    req = urllib.request.Request(f"{OLLAMA}/api/chat", data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        out = json.load(r)["message"]["content"].strip()
    out = re.sub(r"<think>.*?</think>", "", out, flags=re.S).strip()
    out = out.strip('"').strip("“”").splitlines()[0].strip() if out else out
    return out


SYSTEM = (
    "You are a professional app localizer. Translate the given user-interface string from {src} to {dst}. "
    "Preserve placeholders (%@, %d, %lld, %1$@, {{name}}) exactly. Keep it as short and natural as a native app would. "
    "Return ONLY the translated string, no quotes, no explanation."
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("xcstrings")
    ap.add_argument("locale")
    ap.add_argument("--repo", default=None, help="repo root for code-usage context (default: parent dirs of the file)")
    ap.add_argument("--model", default="qwen3:8b")
    ap.add_argument("--n", type=int, default=30)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--out", default="bench/killtest")
    a = ap.parse_args()

    root = a.repo
    if root is None:
        # walk up until a .git or Package.swift/xcodeproj is found
        d = os.path.dirname(os.path.abspath(a.xcstrings))
        while d != "/":
            if os.path.isdir(os.path.join(d, ".git")) or any(x.endswith(".xcodeproj") for x in os.listdir(d)):
                root = d
                break
            d = os.path.dirname(d)

    src_lang, units = load_units(a.xcstrings)
    if len(units) < a.n:
        sys.exit(f"only {len(units)} usable units, need {a.n}")

    rng = random.Random(a.seed)
    with_ctx = []
    for u in units:
        u["usage"] = find_usage(root, u["key"]) if root else None
        if u["usage"]:
            with_ctx.append(u)
    pool = with_ctx if len(with_ctx) >= a.n else units
    sample = rng.sample(pool, a.n)
    has_existing = sum(1 for u in units if a.locale in u["existing"])
    print(f"units={len(units)} with_code_context={len(with_ctx)} existing_{a.locale}_translations={has_existing} model={a.model}")

    system = SYSTEM.format(src=src_lang, dst=a.locale)
    os.makedirs(a.out, exist_ok=True)
    rows, keymap = [], {}
    t0 = time.time()
    for i, u in enumerate(sample, 1):
        base = f"Key: {u['key']}\nSource: {u['source']}"
        if u["comment"]:
            base += f"\nDeveloper comment: {u['comment']}"
        ta = chat(a.model, system, base)

        ctx = base
        if u["usage"]:
            path, ident, snippet = u["usage"]
            ctx += f"\n\nWhere it is used: {path}" + (f", inside `{ident}`" if ident else "") + f"\n```\n{snippet}\n```"
        sims = similar(units, u, a.locale)
        if sims:
            ctx += "\n\nExisting translations in this app (match their tone and terminology):"
            for s in sims:
                ctx += f"\n- {s['source']} → {s['existing'][a.locale]}"
        tb = chat(a.model, system, ctx)

        flip = rng.random() < 0.5
        A, B = (tb, ta) if flip else (ta, tb)
        keymap[i] = {"context_is": "A" if flip else "B", "key": u["key"], "had_usage": bool(u["usage"]), "n_similar": len(sims)}
        rows.append({"id": i, "source": u["source"], "A": A, "B": B, "better (A/B/tie)": ""})
        print(f"[{i:02d}/{a.n}] {u['source'][:40]!r:44} ctx={'y' if u['usage'] else 'n'} sims={len(sims)} {time.time()-t0:5.0f}s")

    csv_path = os.path.join(a.out, f"{a.locale}.csv")
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["id", "source", "A", "B", "better (A/B/tie)"])
        w.writeheader()
        w.writerows(rows)
    key_path = os.path.join(a.out, f"{a.locale}.key.json")
    json.dump({"model": a.model, "file": os.path.abspath(a.xcstrings), "repo": root, "rows": keymap}, open(key_path, "w"), indent=2, ensure_ascii=False)
    print(f"\nwrote {csv_path} ({len(rows)} rows) and {key_path}\n"
          f"Rate each row (A / B / tie) in the last column, then run: scripts/killtest.py --score {csv_path}")


JUDGE_SYSTEM = (
    "You are a senior {dst} localization reviewer for consumer apps. You will see an English UI string and two candidate "
    "{dst} translations, A and B. Judge which one a native speaker would prefer to ship: natural, idiomatic, correct meaning, "
    "consistent app tone, placeholders intact, not over-literal. Answer with exactly one token: A, B, or TIE."
)


def judge(csv_path, judge_model, timeout=120):
    """Blind second-model judging: fills the 'better' column without reading the key file."""
    locale = os.path.basename(csv_path).split(".")[0]
    rows = list(csv.DictReader(open(csv_path, encoding="utf-8")))
    system = JUDGE_SYSTEM.format(dst=locale)
    for r in rows:
        user = f"English: {r['source']}\n\nA: {r['A']}\nB: {r['B']}\n\nAnswer A, B, or TIE."
        try:
            out = chat(judge_model, system, user, timeout=timeout).upper()
        except Exception as e:  # noqa: BLE001
            out = f"ERR {e}"
        m = re.search(r"\b(A|B|TIE)\b", out)
        r["better (A/B/tie)"] = m.group(1) if m else "tie"
        print(f"[{r['id']:>2}] {r['source'][:40]!r:44} -> {r['better (A/B/tie)']}")
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["id", "source", "A", "B", "better (A/B/tie)"])
        w.writeheader()
        w.writerows(rows)
    print(f"judged {len(rows)} rows with {judge_model} -> {csv_path}")


def score(csv_path):
    key = json.load(open(csv_path.replace(".csv", ".key.json")))["rows"]
    wins = ties = losses = unrated = 0
    for row in csv.DictReader(open(csv_path, encoding="utf-8")):
        v = row["better (A/B/tie)"].strip().upper()
        ctx = key[row["id"]]["context_is"]
        if v == "":
            unrated += 1
        elif v == "TIE":
            ties += 1
        elif v == ctx:
            wins += 1
        else:
            losses += 1
    n = wins + ties + losses
    print(f"context wins={wins} losses={losses} ties={ties} unrated={unrated}  (kill rule: need >= 18/30 wins)")
    ok = wins >= 18
    print("PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "--score":
        score(sys.argv[2])
    elif len(sys.argv) >= 3 and sys.argv[1] == "--judge":
        jm = "gemma4"
        if "--judge-model" in sys.argv:
            jm = sys.argv[sys.argv.index("--judge-model") + 1]
        judge(sys.argv[2], jm)
    else:
        main()
