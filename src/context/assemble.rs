//! Attach usage context and few-shot examples to a request within a token budget.
//!
//! Tokens are approximated as chars / 4. Priority under pressure: the file/ident line
//! first, then examples, then the code snippet (trimmed around the matching line).

use crate::context::usage::Usage;
use crate::provider::Request;

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Approximate token budget for context + examples of one request.
    pub tokens: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { tokens: 600 }
    }
}

const MAX_EXAMPLES: usize = 3;
const MAX_EXAMPLE_CHARS: usize = 240;

pub fn attach(
    req: &mut Request,
    usage: Option<&Usage>,
    examples: &[(String, String)],
    budget: &Budget,
) {
    let mut remaining = budget.tokens * 4;

    // 1. Where it's used (one line) — cheapest and most valuable.
    let mut context = String::new();
    if let Some(u) = usage {
        let head = match &u.ident {
            Some(id) => format!("{}:{} inside `{id}`", u.path, u.line),
            None => format!("{}:{}", u.path, u.line),
        };
        if head.len() <= remaining {
            remaining -= head.len();
            context.push_str(&head);
        }
    }

    // 2. Examples, best first, each capped.
    let mut chosen = Vec::new();
    for (s, t) in examples.iter().take(MAX_EXAMPLES) {
        let cost = s.len().min(MAX_EXAMPLE_CHARS) + t.len().min(MAX_EXAMPLE_CHARS) + 4;
        if cost > remaining {
            break;
        }
        remaining -= cost;
        chosen.push((clip(s, MAX_EXAMPLE_CHARS), clip(t, MAX_EXAMPLE_CHARS)));
    }

    // 3. The snippet, trimmed symmetrically around the matching line to what is left.
    if let Some(u) = usage
        && !context.is_empty()
        && remaining > 40
    {
        let snippet = trim_snippet(&u.snippet, remaining - 10);
        if !snippet.trim().is_empty() {
            context.push_str("\n```\n");
            context.push_str(&snippet);
            context.push_str("\n```");
        }
    }

    req.context = if context.is_empty() {
        None
    } else {
        Some(context)
    };
    req.examples = chosen;
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Keep the middle lines (where the match is) and drop from both ends until it fits.
fn trim_snippet(snippet: &str, max_chars: usize) -> String {
    let lines: Vec<&str> = snippet.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let (mut lo, mut hi) = (0usize, lines.len());
    let total = |lo: usize, hi: usize| lines[lo..hi].iter().map(|l| l.len() + 1).sum::<usize>();
    let mid = lines.len() / 2;
    while lo < hi && total(lo, hi) > max_chars {
        // Drop the line farther from the middle.
        if mid.saturating_sub(lo) >= hi.saturating_sub(mid + 1) {
            lo += 1;
        } else {
            hi -= 1;
        }
    }
    lines[lo..hi].join("\n")
}
