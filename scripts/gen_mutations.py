#!/usr/bin/env python3
"""Derive placeholder-mutation fixtures from the corpus.

For every (source, translation) pair that contains placeholders, emit a few
corrupted translations that a validator MUST flag:
  drop      – remove one placeholder
  retype    – %d → %s (or %@ → %d)
  dup       – repeat a placeholder
  swap      – swap the first two unnumbered printf placeholders
  braces    – {{x}} → {x} or {x} → {{x}}
Output: <file>.mutations.json next to each corpus file (deterministic, capped).
"""
import json, re, sys, os, random, collections, xml.etree.ElementTree as ET

PRINTF = re.compile(r"%(?!%)(\d+\$)?[-+#0',]*\d*(?:\.\d+)?(?:hh|h|ll|l|q|L|z|t|j)?([@diufFeEgGxXosScCpaAb])")
FAMILY = {"d": "d", "i": "d", "u": "d", "x": "d", "X": "d", "o": "d", "f": "f", "F": "f", "e": "f", "E": "f", "g": "f", "G": "f", "a": "f", "A": "f", "s": "s", "S": "s", "c": "c", "C": "c"}

def canon(text):
    """Approximation of the Rust canonicaliser, good enough to pick consistent pairs."""
    out = []; n = 1
    sd = list(SDVAR.finditer(text))
    for m in PRINTF.finditer(text):
        if any(v.start() <= m.start() < v.end() for v in sd): continue
        num = int(m.group(1)[:-1]) if m.group(1) else n
        if not m.group(1): n += 1
        out.append("%%%d$%s" % (num, FAMILY.get(m.group(2), m.group(2))))
    out += ["sd"] * len(SDVAR.findall(text))
    out += [x.strip("{} ") + "2" for x in BRACE2.findall(text)]
    out += [x for x in BRACE1.findall(text)]
    return sorted(set(out)) if m_numbered(text) else sorted(out)

def m_numbered(text):
    return any(m.group(1) for m in PRINTF.finditer(text))
BRACE2 = re.compile(r"\{\{\s*[\w.-]+\s*\}\}")
BRACE1 = re.compile(r"(?<!\{)\{[\w.-]+\}(?!\})")
CAP = 60

SDVAR = re.compile(r"%(\d+\$)?#@\w+@")

def mutations(src, tr):
    out = []
    sd = list(SDVAR.finditer(tr))
    pf = [m for m in PRINTF.finditer(tr) if not any(v.start() <= m.start() < v.end() for v in sd)]
    if sd:
        v = sd[0]
        out.append(("drop", tr[:v.start()] + tr[v.end():]))
        out.append(("dup", tr[:v.end()] + " " + v.group(0) + tr[v.end():]))
    b2 = list(BRACE2.finditer(tr)); b1 = list(BRACE1.finditer(tr))
    if pf:
        m = pf[0]
        occurrences = sum(1 for x in pf if x.group(0) == m.group(0))
        if not (m.group(1) and occurrences > 1):  # dropping one of a repeated numbered arg is harmless
            out.append(("drop", tr[:m.start()] + tr[m.end():]))
        conv = m.group(2)
        new = {"d": "s", "@": "d", "s": "d", "f": "d"}.get(conv)
        if new:
            out.append(("retype", tr[:m.end()-1] + new + tr[m.end():]))
        if not m.group(1):  # duplicating an explicitly numbered arg is harmless
            out.append(("dup", tr[:m.end()] + " " + m.group(0) + tr[m.end():]))
        unnum = [x for x in pf if not x.group(1)]
        if len(unnum) >= 2 and unnum[0].group(2) != unnum[1].group(2):
            a, b = unnum[0], unnum[1]
            out.append(("swap", tr[:a.start()] + b.group(0) + tr[a.end():b.start()] + a.group(0) + tr[b.end():]))
    if b2:
        m = b2[0]
        out.append(("drop", tr[:m.start()] + tr[m.end():]))
        out.append(("braces", tr[:m.start()] + m.group(0)[1:-1] + tr[m.end():]))
    elif b1:
        m = b1[0]
        out.append(("drop", tr[:m.start()] + tr[m.end():]))
        out.append(("braces", tr[:m.start()] + "{" + m.group(0) + "}" + tr[m.end():]))
    return out

def pairs_xcstrings(path):
    d = json.load(open(path, encoding="utf-8"))
    src_lang = d.get("sourceLanguage", "en")
    for key, e in d.get("strings", {}).items():
        locs = e.get("localizations") or {}
        s = (locs.get(src_lang, {}).get("stringUnit") or {}).get("value") or key
        for loc, v in locs.items():
            if loc == src_lang: continue
            t = (v.get("stringUnit") or {}).get("value")
            if t: yield key, loc, s, t

def pairs_android(path):
    root = ET.parse(path).getroot()
    for el in root.findall("string"):
        if el.get("translatable") == "false" or el.text is None: continue
        yield el.get("name"), "en", el.text, el.text

def pairs_json(path):
    d = json.load(open(path, encoding="utf-8"))
    def walk(o, p):
        if isinstance(o, dict):
            for k, v in o.items(): yield from walk(v, p + [k])
        elif isinstance(o, str): yield ".".join(p), "en", o, o
    yield from walk(d, [])

def main():
    root = os.path.join(os.path.dirname(__file__), "..", "tests", "corpus")
    for sub, reader, ext in [("xcstrings", pairs_xcstrings, ".xcstrings"), ("android", pairs_android, ".xml"), ("json", pairs_json, ".json")]:
        d = os.path.join(root, sub)
        for fn in sorted(os.listdir(d)):
            if not fn.endswith(ext) or fn.endswith(".mutations.json") or fn == "known_bugs.json": continue
            path = os.path.join(d, fn)
            rng = random.Random(fn)
            cands = []
            for key, loc, s, t in reader(path):
                if not (PRINTF.search(t) or BRACE2.search(t) or BRACE1.search(t)): continue
                if "plural" in t or "select" in t: continue  # ICU bodies are covered by unit tests
                if canon(s) != canon(t): continue  # skip pairs that are already inconsistent upstream
                for kind, mutated in mutations(s, t):
                    if mutated != t:
                        cands.append({"key": key, "locale": loc, "source": s, "translation": mutated, "kind": kind})
            rng.shuffle(cands)
            cands = cands[:CAP]
            cands.sort(key=lambda m: (m["key"], m["locale"], m["kind"]))
            out = path[: -len(ext)] + ".mutations.json"
            with open(out, "w", encoding="utf-8") as f:
                json.dump(cands, f, ensure_ascii=False, indent=1)
            print(f"{sub}/{fn}: {len(cands)} mutations")

if __name__ == "__main__":
    main()
