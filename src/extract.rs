//! `polygo extract`: pull user-facing text out of web source (JSX/TSX, HTML in
//! template literals, .html/.vue/.svelte) into an i18next `locales/en.json`, with a
//! report of where every string came from. `--rewrite` replaces the strings in
//! JS/TS files with `t("key")` calls and generates a small `i18n.ts`.

use crate::config::Extract as Rules;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub line: usize,
    /// Cleaned text as shown to people (interpolations kept verbatim).
    pub text: String,
    pub key: String,
    /// `text`, or the attribute name (`placeholder`, `aria-label`, ...).
    pub kind: String,
    /// Where the raw string sits in the file (byte offsets of the trimmed run / value).
    pub start: usize,
    pub end: usize,
    /// How the string is embedded: `template` (inside a JS template literal), `jsx`,
    /// `markup` (.html/.vue/.svelte: not rewritten), `string` (inside a plain JS string:
    /// not rewritten).
    pub context: String,
    /// Reason the rewrite leaves it alone, if any (`fragment`, `markup`, `string`).
    pub skip: Option<String>,
}

const EXT: &[&str] = &[
    "tsx", "jsx", "ts", "js", "mjs", "vue", "svelte", "astro", "html",
];
const JS_EXT: &[&str] = &["tsx", "jsx", "ts", "js", "mjs"];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "out",
    "coverage",
    "vendor",
    ".git",
    "target",
    ".claude",
    "public",
    "static",
    "assets",
];
const TEXT_ATTRS: &[&str] = &[
    "placeholder",
    "title",
    "alt",
    "aria-label",
    "aria-description",
    "aria-placeholder",
    "label",
    "aria-roledescription",
];
/// Elements whose text is never UI copy.
const SKIP_ELEMENTS: &[&str] = &[
    "script", "style", "svg", "code", "pre", "kbd", "samp", "math", "noscript", "template",
    "textarea",
];

pub fn scan(root: &Path, rules: &Rules) -> Result<Vec<Hit>> {
    let mut hits = Vec::new();
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for g in &rules.ignore_paths {
        overrides
            .add(&format!("!{g}"))
            .with_context(|| format!("bad ignore_paths glob {g:?}"))?;
    }
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .overrides(overrides.build()?)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&name.as_ref()))
        })
        .build();
    let mut files: Vec<PathBuf> = walker
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| EXT.contains(&x.to_str().unwrap_or("")))
        })
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            !(n.contains(".test.")
                || n.contains(".spec.")
                || n.contains(".stories.")
                || n.ends_with(".d.ts")
                || n.contains(".generated.")
                || n == "i18n.ts"
                || n == "i18n.js")
        })
        .collect();
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scan_text(&rel, &text, &mut hits);
    }
    hits.retain(|h| !rules.ignore_exact.iter().any(|x| x == &h.text));
    hits.retain(|h| {
        let lower = h.text.to_lowercase();
        !rules
            .ignore
            .iter()
            .any(|w| lower.contains(&w.to_lowercase()))
    });
    hits.sort_by(|a, b| (&a.file, a.start).cmp(&(&b.file, b.start)));
    Ok(hits)
}

/// Per-byte context of a JS/TS file: which bytes are inside a template literal, a plain
/// string or a comment. Template literals nest through `${ ... }`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JsCtx {
    Code,
    Template,
    Str,
    Comment,
}

fn js_contexts(text: &str) -> Vec<JsCtx> {
    let b = text.as_bytes();
    let mut out = vec![JsCtx::Code; b.len()];
    // Stack of what a `}` closes back into: Template (from `${`) or a brace in code.
    let mut stack: Vec<JsCtx> = Vec::new();
    let mut state = JsCtx::Code;
    let mut quote = 0u8;
    let mut block_comment = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        out[i] = state;
        match state {
            JsCtx::Code => {
                if c == b'/' && b.get(i + 1) == Some(&b'/') {
                    state = JsCtx::Comment;
                    block_comment = false;
                    out[i] = JsCtx::Comment;
                } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
                    state = JsCtx::Comment;
                    block_comment = true;
                    out[i] = JsCtx::Comment;
                } else if c == b'`' {
                    state = JsCtx::Template;
                } else if c == b'"' || c == b'\'' {
                    state = JsCtx::Str;
                    quote = c;
                } else if c == b'{' {
                    stack.push(JsCtx::Code);
                } else if c == b'}'
                    && let Some(JsCtx::Template) = stack.pop()
                {
                    state = JsCtx::Template;
                }
            }
            JsCtx::Template => {
                if c == b'\\' {
                    i += 1;
                    if i < b.len() {
                        out[i] = JsCtx::Template;
                    }
                } else if c == b'`' {
                    state = JsCtx::Code;
                } else if c == b'$' && b.get(i + 1) == Some(&b'{') {
                    out[i + 1] = JsCtx::Template;
                    i += 1;
                    stack.push(JsCtx::Template);
                    state = JsCtx::Code;
                }
            }
            JsCtx::Str => {
                if c == b'\\' {
                    i += 1;
                    if i < b.len() {
                        out[i] = JsCtx::Str;
                    }
                } else if c == quote || c == b'\n' {
                    state = JsCtx::Code;
                }
            }
            JsCtx::Comment => {
                if block_comment {
                    if c == b'*' && b.get(i + 1) == Some(&b'/') {
                        out[i + 1] = JsCtx::Comment;
                        i += 1;
                        state = JsCtx::Code;
                    }
                } else if c == b'\n' {
                    state = JsCtx::Code;
                }
            }
        }
        i += 1;
    }
    out
}

/// Scan markup-ish text: anything between `>` and `<` that reads like a sentence, plus
/// text attributes. Works on JSX, HTML inside template literals, and plain HTML alike.
pub fn scan_text(file: &str, text: &str, out: &mut Vec<Hit>) {
    let ext = file.rsplit('.').next().unwrap_or("");
    let is_js = JS_EXT.contains(&ext);
    let ctx = if is_js { Some(js_contexts(text)) } else { None };
    if let Some(c) = &ctx {
        scan_properties(file, text, c, out);
    }
    let context_at = |pos: usize| -> &'static str {
        match ctx.as_ref().and_then(|c| c.get(pos)) {
            None => "markup",
            Some(JsCtx::Template) => "template",
            Some(JsCtx::Code) => "jsx",
            Some(JsCtx::Str) => "string",
            Some(JsCtx::Comment) => "comment",
        }
    };
    let b = text.as_bytes();
    let mut i = 0;
    let mut skip_depth: Vec<String> = Vec::new();
    let mut line = 1;
    while i < b.len() {
        if b[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if matches!(context_at(i), "string" | "comment") {
            i += 1;
            continue;
        }
        let Some(end) = tag_end(text, i) else {
            i += 1;
            continue;
        };
        let tag = &text[i + 1..end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        let closing = tag.starts_with('/');
        if name.is_empty() || tag.starts_with('!') || tag.starts_with('?') || !is_tag(tag) {
            i += 1;
            continue;
        }
        // Outside a template literal, a JS/TS file's `<...>` is JSX only when the name
        // is an HTML element or a Capitalized component; `Set<string>` and `<T>` are
        // generics.
        if context_at(i) == "jsx" {
            let raw_name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
                .collect();
            let component = raw_name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
                && raw_name.chars().count() >= 2;
            if !component && !HTML_ELEMENTS.contains(&name.as_str()) {
                i += 1;
                continue;
            }
        }
        if SKIP_ELEMENTS.contains(&name.as_str()) {
            if closing {
                skip_depth.retain(|n| *n != name);
            } else if !tag.ends_with('/') {
                skip_depth.push(name.clone());
            }
        }
        if !closing && skip_depth.is_empty() {
            for (attr, vstart, vend) in attributes(tag) {
                let value = &tag[vstart..vend];
                if TEXT_ATTRS.contains(&attr.as_str()) && looks_like_copy(value) {
                    let abs = i + 1 + vstart;
                    // The tag's own context: to the JS lexer a JSX attribute value looks
                    // like a plain string, but it belongs to the markup.
                    out.push(make_hit(
                        file,
                        line,
                        value,
                        &attr,
                        abs,
                        abs + value.len(),
                        context_at(i),
                        false,
                    ));
                }
            }
        }
        line += tag.matches('\n').count();
        i = end + 1;
        // A block element whose content is one sentence with inline markup inside
        // (`<p>writes a <code>.form</code> file.</p>`) is extracted whole, tags and all,
        // so the translator sees the full sentence.
        if skip_depth.is_empty()
            && !closing
            && !tag.ends_with('/')
            && !INLINE.contains(&name.as_str())
            && let Some((inner_end, close_end)) = find_close(text, i, &name)
        {
            let inner = &text[i..inner_end];
            if is_mixed_sentence(inner) {
                let lead = inner.len() - inner.trim_start().len();
                let trimmed = inner.trim();
                let cleaned = clean(trimmed);
                let start = i + lead;
                out.push(make_hit(
                    file,
                    line,
                    &cleaned,
                    "text",
                    start,
                    start + trimmed.len(),
                    context_at(start),
                    false,
                ));
                line +=
                    inner.matches('\n').count() + text[inner_end..close_end].matches('\n').count();
                i = close_end;
                continue;
            }
        }
        if skip_depth.is_empty() {
            let run_end = text[i..].find('<').map(|k| i + k).unwrap_or(text.len());
            let run = &text[i..run_end];
            let lead = run.len() - run.trim_start().len();
            let trimmed = run.trim();
            let cleaned = clean(trimmed);
            // JSX text never starts like code.
            let code_like = context_at(i) == "jsx"
                && cleaned.chars().next().is_some_and(|c| {
                    matches!(
                        c,
                        '(' | ')' | '=' | ',' | ';' | '[' | ']' | '.' | '&' | '|' | '?'
                    )
                });
            if looks_like_copy(&cleaned) && !code_like {
                let start = i + lead;
                // A continuation of a sentence split by inline markup is a fragment:
                // translate the whole sentence by hand instead.
                // A sentence split by inline markup (`writes a <code>.form</code> file`)
                // is a fragment. The whole content of an inline element
                // (`<a>Write a form</a>`) is not: only a run followed by an *opening*
                // inline tag, or preceded by a *closing* one, counts.
                let next_opens_inline = text[run_end..]
                    .strip_prefix('<')
                    .map(|rest| {
                        let n: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphabetic())
                            .collect();
                        !rest.starts_with('/') && INLINE.contains(&n.to_ascii_lowercase().as_str())
                    })
                    .unwrap_or(false);
                let first = cleaned.chars().next().unwrap();
                let fragment = (closing && INLINE.contains(&name.as_str()))
                    || matches!(first, '.' | ',' | ';' | ':' | ')')
                    || next_opens_inline;
                out.push(make_hit(
                    file,
                    line,
                    &cleaned,
                    "text",
                    start,
                    start + trimmed.len(),
                    context_at(start),
                    fragment,
                ));
            }
            line += run.matches('\n').count();
            i = run_end;
        }
    }
}

/// `(inner_end, close_end)` of the element `name` opened just before `from`, honouring
/// nesting of the same name. `None` when unclosed or when a block tag appears inside.
fn find_close(text: &str, from: usize, name: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut i = from;
    while let Some(k) = text[i..].find('<') {
        let at = i + k;
        let Some(end) = tag_end(text, at) else {
            i = at + 1;
            continue;
        };
        let tag = &text[at + 1..end];
        let n: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        if tag.starts_with('/') {
            if n == name {
                if depth == 0 {
                    return Some((at, end + 1));
                }
                depth -= 1;
            }
        } else if !tag.ends_with('/') && is_tag(tag) {
            if n == name {
                depth += 1;
            } else if !INLINE.contains(&n.as_str()) && !n.is_empty() {
                return None; // a block inside: not one sentence
            }
        }
        i = end + 1;
    }
    None
}

/// Text with at least one inline tag and letters outside the tags, short enough to be
/// a sentence or a heading rather than a whole section.
fn is_mixed_sentence(inner: &str) -> bool {
    if inner.chars().count() > 400 || !inner.contains('<') {
        return false;
    }
    let mut outside = String::new();
    let mut inside = false;
    for c in inner.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => outside.push(c),
            _ => {}
        }
    }
    let outside_ok = looks_like_copy(&clean(&outside));
    outside_ok
        && strip_interpolations(&outside)
            .chars()
            .filter(|c| c.is_alphabetic())
            .count()
            >= 3
}

/// Object properties whose string values are UI copy: `{ id: "rating", label: "Rating" }`.
const TEXT_PROPS: &[&str] = &[
    "label",
    "title",
    "text",
    "description",
    "placeholder",
    "message",
    "heading",
    "subtitle",
    "caption",
    "tooltip",
    "hint",
    "summary",
    "cta",
];

/// `label: "Rating"` / `title: 'Sign in'` in code (not inside strings, templates or
/// comments): the value becomes a hit with `kind = "prop"` and is rewritten to
/// `label: t("Rating")`. JSX props (`label="x"`) are handled by the markup scan.
fn scan_properties(file: &str, text: &str, ctx: &[JsCtx], out: &mut Vec<Hit>) {
    let b = text.as_bytes();
    let mut line = 1;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if ctx[i] != JsCtx::Code || !b[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        // Identifier at a property position: preceded by `{`, `,` or newline/whitespace.
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        let ident = &text[start..i];
        if !TEXT_PROPS.contains(&ident) {
            continue;
        }
        let before = text[..start].trim_end();
        if !(before.ends_with('{') || before.ends_with(',') || before.is_empty()) {
            continue;
        }
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if b.get(j) != Some(&b':') {
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let Some(&q) = b.get(j) else { break };
        if q != b'"' && q != b'\'' {
            continue;
        }
        let vstart = j + 1;
        let Some(vend) = text[vstart..].find(q as char).map(|k| vstart + k) else {
            break;
        };
        let value = &text[vstart..vend];
        if !value.contains('\\') && looks_like_copy(value) {
            out.push(make_hit(
                file, line, value, "prop", vstart, vend, "jsx", false,
            ));
        }
        i = vend + 1;
    }
}

const HTML_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    "svg",
    "path",
    "g",
    "circle",
    "rect",
    "line",
    "polyline",
    "polygon",
    "text",
    "use",
    "defs",
    "symbol",
    "math",
];

const INLINE: &[&str] = &[
    "span", "em", "strong", "b", "i", "a", "code", "kbd", "abbr", "u", "s", "small", "mark", "sub",
    "sup",
];

#[allow(clippy::too_many_arguments)]
fn make_hit(
    file: &str,
    line: usize,
    text: &str,
    kind: &str,
    start: usize,
    end: usize,
    context: &str,
    fragment: bool,
) -> Hit {
    let skip = if fragment {
        Some("fragment".to_string())
    } else if matches!(context, "markup" | "string" | "comment") {
        Some(context.to_string())
    } else {
        None
    };
    Hit {
        file: file.to_string(),
        line,
        text: text.to_string(),
        key: key_for(text, context == "jsx" || context == "markup"),
        kind: kind.to_string(),
        start,
        end,
        context: context.to_string(),
        skip,
    }
}

fn tag_end(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = start + 1;
    let mut depth = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' if depth > 0 || i > start + 1 => quote = Some(c),
                b'{' => depth += 1,
                b'}' => depth = (depth - 1).max(0),
                b'>' if depth == 0 => return Some(i),
                b'<' if depth == 0 => return None,
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Strict-ish tag syntax: `/?name (attr(=value)?)* /?`. Rejects code that merely
/// contains `<` and `>`.
fn is_tag(tag: &str) -> bool {
    let t = tag.trim_end_matches('/').trim();
    let t = t.strip_prefix('/').unwrap_or(t);
    let b = t.as_bytes();
    let mut i = 0;
    if !b.first().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'.' | b':')) {
        i += 1;
    }
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return true;
        }
        if b[i] == b'{' {
            let Some(close) = t[i..].find('}') else {
                return false;
            };
            i += close + 1;
            continue;
        }
        if !(b[i].is_ascii_alphabetic() || matches!(b[i], b'@' | b':' | b'#' | b'_' | b'$')) {
            return false;
        }
        while i < b.len()
            && (b[i].is_ascii_alphanumeric()
                || matches!(b[i], b'-' | b'_' | b':' | b'@' | b'.' | b'$'))
        {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            match b.get(i) {
                Some(&q) if q == b'"' || q == b'\'' => {
                    let Some(close) = t[i + 1..].find(q as char) else {
                        return false;
                    };
                    i += close + 2;
                }
                Some(b'{') => {
                    let mut depth = 0;
                    let mut j = i;
                    let mut closed = false;
                    while j < b.len() {
                        match b[j] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    closed = true;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    if !closed {
                        return false;
                    }
                    i = j + 1;
                }
                Some(b'`') => {
                    let Some(close) = t[i + 1..].find('`') else {
                        return false;
                    };
                    i += close + 2;
                }
                Some(_) => {
                    while i < b.len() && !b[i].is_ascii_whitespace() {
                        i += 1;
                    }
                }
                None => return false,
            }
        }
    }
}

/// `(name, value_start, value_end)` for quoted attributes, offsets within `tag`.
fn attributes(tag: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let b = tag.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && !(b[i].is_ascii_alphabetic() || b[i] == b'@' || b[i] == b':') {
            i += 1;
        }
        let start = i;
        while i < b.len()
            && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'_' | b':' | b'@' | b'.'))
        {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let name = tag[start..i].to_ascii_lowercase();
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if b.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        match b.get(j) {
            Some(&q) if q == b'"' || q == b'\'' => {
                let vstart = j + 1;
                let Some(vend) = tag[vstart..].find(q as char).map(|k| vstart + k) else {
                    break;
                };
                out.push((name, vstart, vend));
                i = vend + 1;
            }
            _ => {
                i = j;
            }
        }
    }
    out
}

fn clean(run: &str) -> String {
    run.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Does this read like something a user sees?
pub fn looks_like_copy(s: &str) -> bool {
    let s = s.trim();
    if s.chars().count() < 2 || !s.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let stripped = strip_interpolations(s);
    if !stripped.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    if s.starts_with('&') && s.ends_with(';') && !s.contains(' ') {
        return false;
    }
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with('/')
        || s.starts_with('#')
    {
        return false;
    }
    if s.contains('`')
        || s.contains("=>")
        || s.contains(");")
        || s.contains("};")
        || s.contains(" ? ")
        || s.contains("/*")
        || s.contains("*/")
    {
        return false;
    }
    let single = !s.contains(' ');
    if single {
        let w = s.trim_matches(|c: char| !c.is_alphanumeric());
        let has_upper_inside = w.chars().skip(1).any(|c| c.is_uppercase());
        return w.chars().all(|c| c.is_alphabetic() || c == '\'')
            && !has_upper_inside
            && !w.chars().all(|c| c.is_uppercase() && w.len() > 4);
    }
    true
}

fn strip_interpolations(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if (b[i] == b'$' && b.get(i + 1) == Some(&b'{')) || b[i] == b'{' {
            depth += 1;
            i += if b[i] == b'$' { 2 } else { 1 };
            continue;
        }
        if b[i] == b'}' && depth > 0 {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        } else {
            i += 1;
        }
    }
    out
}

/// i18next-style natural key: the English text itself, with `${expr}` / `{expr}` turned
/// into `{{0}}` placeholders. Key == English means `t()` needs no catalog for the source
/// language: a missing translation falls back to the key.
pub fn key_for(text: &str, jsx: bool) -> String {
    split_interpolations(text, jsx).0
}

/// `("Signed in as {{0}}.", ["email"])` for `Signed in as ${email}.`
pub fn split_interpolations(text: &str, jsx: bool) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut exprs = Vec::new();
    let mut expr = String::new();
    let mut depth = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // `${expr}` everywhere; a bare `{expr}` only in JSX (in a template literal it is
        // literal text, e.g. a JSON snippet inside <code>).
        if depth == 0 && ((b[i] == b'$' && b.get(i + 1) == Some(&b'{')) || (jsx && b[i] == b'{')) {
            depth = 1;
            out.push_str(&format!("{{{{{}}}}}", exprs.len()));
            i += if b[i] == b'$' { 2 } else { 1 };
            continue;
        }
        if depth > 0 {
            if b[i] == b'{' {
                depth += 1;
            } else if b[i] == b'}' {
                depth -= 1;
                if depth == 0 {
                    exprs.push(expr.trim().to_string());
                    expr.clear();
                    i += 1;
                    continue;
                }
            }
            expr.push(text[i..].chars().next().unwrap());
            i += text[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    (out, exprs)
}

/// Merge hits into an i18next JSON file (existing keys are kept). Returns (added, total).
pub fn write_catalog(path: &Path, hits: &[Hit]) -> Result<(usize, usize)> {
    let mut map: BTreeMap<String, String> = if path.exists() {
        serde_json::from_str(&std::fs::read_to_string(path)?).context("existing catalog JSON")?
    } else {
        BTreeMap::new()
    };
    let before = map.len();
    for h in hits
        .iter()
        .filter(|h| h.skip.as_deref() != Some("fragment"))
    {
        map.entry(h.key.clone()).or_insert_with(|| h.key.clone());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(&map)?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok((map.len() - before, map.len()))
}

// ---- rewrite -----------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub struct RewriteReport {
    pub files: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub i18n_file: Option<String>,
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("{s:?}"))
}

/// Replace every rewritable hit with a `t()` call, add the import, write `i18n.ts`.
pub fn rewrite(root: &Path, hits: &[Hit], catalog: &Path, i18n: &Path) -> Result<RewriteReport> {
    let mut report = RewriteReport::default();
    let mut by_file: BTreeMap<&str, Vec<&Hit>> = BTreeMap::new();
    for h in hits {
        if h.skip.is_some() {
            report.skipped += 1;
            continue;
        }
        by_file.entry(h.file.as_str()).or_default().push(h);
    }
    for (file, hs) in by_file {
        let path = root.join(file);
        let mut text = std::fs::read_to_string(&path)?;
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        for h in hs {
            let raw = &text[h.start..h.end];
            let (key, exprs) = split_interpolations(&clean(raw), h.context == "jsx");
            let args = if exprs.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> = exprs
                    .iter()
                    .enumerate()
                    .map(|(i, e)| format!("{i}: {e}"))
                    .collect();
                format!(", {{ {} }}", pairs.join(", "))
            };
            let call = format!("t({}{args})", js_string(&key));
            let replacement = match (h.context.as_str(), h.kind.as_str()) {
                ("template", "text") => format!("${{{call}}}"),
                ("jsx", "text") => format!("{{{call}}}"),
                ("template", _) => format!("${{{call}}}"),
                ("jsx", "prop") => {
                    // label: "Rating" → label: t("Rating"): swallow the quotes.
                    edits.push((h.start - 1, h.end + 1, call));
                    continue;
                }
                ("jsx", _) => {
                    // placeholder="x" → placeholder={t("x")}: swallow the quotes.
                    let q_before = h.start.checked_sub(1).map(|p| text.as_bytes()[p]);
                    let q_after = text.as_bytes().get(h.end).copied();
                    if let (Some(a), Some(b)) = (q_before, q_after)
                        && (a == b'"' || a == b'\'')
                        && a == b
                    {
                        edits.push((h.start - 1, h.end + 1, format!("{{{call}}}")));
                        continue;
                    }
                    continue;
                }
                _ => continue,
            };
            edits.push((h.start, h.end, replacement));
        }
        if edits.is_empty() {
            continue;
        }
        edits.sort_by_key(|e| std::cmp::Reverse(e.0));
        let n = edits.len();
        for (s, e, r) in edits {
            text.replace_range(s..e, &r);
        }
        // Import.
        let rel = relative_import(file, i18n);
        if !text.contains("/i18n\"") && !text.contains("/i18n'") {
            let import = format!("import {{ t }} from \"{rel}\";\n");
            let at = last_import_end(&text);
            text.insert_str(at, &import);
        }
        std::fs::write(&path, text)?;
        report.files += 1;
        report.replaced += n;
    }
    // Helper module.
    let i18n_path = root.join(i18n);
    if !i18n_path.exists() {
        let catalog_rel = relative_path(i18n, catalog);
        let src = format!(
            "// Generated by `polygo extract --rewrite`. Edit freely; it is yours now.\n\
// Keys are the English text, so nothing needs loading for English. For another\n\
// locale, load its JSON and register it, e.g.\n\
//   addCatalog(\"de\", (await import({})).default);\n\
//   setLocale(\"de\");\n\n\
type Catalog = Record<string, string>;\n\
const catalogs: Record<string, Catalog> = {{}};\n\
let current = \"en\";\n\n\
export function setLocale(locale: string): void {{\n  current = locale;\n}}\n\n\
export function addCatalog(locale: string, catalog: Catalog): void {{\n  catalogs[locale] = catalog;\n}}\n\n\
export function t(key: string, args?: Record<string | number, unknown>): string {{\n\
  const text = catalogs[current]?.[key] ?? key;\n\
  return args ? text.replace(/\\{{\\{{(\\w+)\\}}\\}}/g, (_, k: string) => String(args[k] ?? \"\")) : text;\n\
}}\n",
            js_string(&catalog_rel.replace("en.json", "de.json"))
        );
        if let Some(dir) = i18n_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&i18n_path, src)?;
        report.i18n_file = Some(i18n.to_string_lossy().replace('\\', "/"));
    }
    Ok(report)
}

fn last_import_end(text: &str) -> usize {
    let mut at = 0;
    let mut pos = 0;
    let mut open = false; // inside a multi-line `import {` ... `} from "x";`
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if open {
            if t.contains(" from ") || t.contains("from \"") || t.contains("from '") {
                open = false;
                at = pos + line.len();
            }
        } else if t.starts_with("import ") || (t.starts_with("export ") && t.contains(" from ")) {
            at = pos + line.len();
            let is_side_effect = t.starts_with("import \"") || t.starts_with("import '");
            if !is_side_effect && !t.contains(" from ") {
                open = true;
            }
        } else if !(t.is_empty()
            || t.starts_with("//")
            || t.starts_with("/*")
            || t.starts_with('*')
            || t.starts_with("\"use ")
            || t.starts_with("'use "))
        {
            break;
        }
        pos += line.len();
    }
    at
}

/// `./i18n` / `../../src/i18n` from `file` to the helper (both root-relative).
fn relative_import(file: &str, i18n: &Path) -> String {
    let target = i18n.with_extension("");
    relative_path(Path::new(file), &target)
}

/// Relative path from the directory of `from` to `to` (both root-relative), with `./`.
fn relative_path(from: &Path, to: &Path) -> String {
    let from_dir: Vec<&str> = from
        .parent()
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_str().unwrap_or(""))
                .filter(|c| !c.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let to_parts: Vec<&str> = to
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .filter(|c| !c.is_empty())
        .collect();
    let common = from_dir
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<String> = Vec::new();
    for _ in common..from_dir.len() {
        out.push("..".into());
    }
    for p in &to_parts[common..] {
        out.push((*p).to_string());
    }
    let joined = out.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}
