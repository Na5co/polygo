//! Every kind of finding `check` can raise, with its default severity and the words that
//! explain it. The one place these live: the text output, `--json`, `--github`, SARIF
//! rules, `--explain` and the README table all read from here.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Code {
    Placeholders,
    Plural,
    Markup,
    Escape,
    Array,
    Duplicate,
    Glossary,
    Empty,
    Identical,
    Fragment,
    Length,
    Whitespace,
    Punctuation,
    Orphan,
    Fuzzy,
    State,
    Encoding,
    Invisible,
    Link,
    Brackets,
    Entities,
    Inconsistent,
}

impl Code {
    pub const ALL: [Code; 22] = [
        Code::Placeholders,
        Code::Plural,
        Code::Markup,
        Code::Escape,
        Code::Array,
        Code::Duplicate,
        Code::Glossary,
        Code::Empty,
        Code::Identical,
        Code::Fragment,
        Code::Length,
        Code::Whitespace,
        Code::Punctuation,
        Code::Orphan,
        Code::Fuzzy,
        Code::State,
        Code::Encoding,
        Code::Invisible,
        Code::Link,
        Code::Brackets,
        Code::Entities,
        Code::Inconsistent,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Code::Placeholders => "placeholders",
            Code::Plural => "plural",
            Code::Markup => "markup",
            Code::Escape => "escape",
            Code::Array => "array",
            Code::Duplicate => "duplicate",
            Code::Glossary => "glossary",
            Code::Empty => "empty",
            Code::Identical => "identical",
            Code::Fragment => "fragment",
            Code::Length => "length",
            Code::Whitespace => "whitespace",
            Code::Punctuation => "punctuation",
            Code::Orphan => "orphan",
            Code::Fuzzy => "fuzzy",
            Code::State => "state",
            Code::Encoding => "encoding",
            Code::Invisible => "invisible",
            Code::Link => "link",
            Code::Brackets => "brackets",
            Code::Entities => "entities",
            Code::Inconsistent => "inconsistent",
        }
    }

    pub fn parse(s: &str) -> Option<Code> {
        Code::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// The severity a finding of this code has unless the check says otherwise
    /// (`escape` and `plural` have a warning-level variant, `length` an error-level one).
    pub fn severity(self) -> Severity {
        match self {
            Code::Placeholders
            | Code::Plural
            | Code::Markup
            | Code::Escape
            | Code::Array
            | Code::Duplicate
            | Code::Glossary
            | Code::Empty => Severity::Error,
            _ => Severity::Warning,
        }
    }

    /// Whether `check --fix` can do anything about it: re-translating the key helps for
    /// what the model wrote, not for a duplicate key, a short array or a source-file escape.
    pub fn fixable(self) -> bool {
        matches!(
            self,
            Code::Placeholders | Code::Markup | Code::Empty | Code::Glossary | Code::Length
        )
    }

    pub fn title(self) -> &'static str {
        match self {
            Code::Placeholders => "Placeholder mismatch",
            Code::Plural => "Missing plural form",
            Code::Markup => "Markup mismatch",
            Code::Escape => "Android escape",
            Code::Array => "Array length",
            Code::Duplicate => "Duplicate key",
            Code::Glossary => "Glossary",
            Code::Empty => "Empty translation",
            Code::Identical => "Identical to source",
            Code::Fragment => "Half translated",
            Code::Length => "Length",
            Code::Whitespace => "Edge whitespace",
            Code::Punctuation => "Punctuation dropped",
            Code::Orphan => "Orphan key",
            Code::Fuzzy => "Fuzzy",
            Code::State => "Xcode state",
            Code::Encoding => "Wrong encoding",
            Code::Invisible => "Invisible character",
            Code::Link => "Link changed",
            Code::Brackets => "Unbalanced brackets",
            Code::Entities => "Double-escaped entity",
            Code::Inconsistent => "Inconsistent term",
        }
    }

    /// What it means and what to do, for `--explain` and the SARIF rule.
    pub fn explain(self) -> &'static str {
        match self {
            Code::Placeholders => {
                "A format placeholder (%1$@, {{name}}, %(count)s, {0}, {n, plural, …}) is missing, added, retyped or reordered in the translation. At runtime the value lands in the wrong place, shows raw, or crashes (Android: %d where %s was expected). A placeholder whose name was translated ({{ models }} → {{ modelli }}, %(count)s → %(anzahl)s) is the most common placeholder bug of all; it is named as such, and `polygo check --fix` puts the source name back with no model involved. Unnumbered printf arguments are numbered by position, so reordering them needs explicit %2$s %1$s. In zero/one/two plural forms the count may be left out where those are exact counts; not where `one` also covers 21, 31 (ru, uk, be, hr, sr, bs, lt, lv)."
            }
            Code::Plural => {
                "The locale needs a CLDR plural category the translation does not provide: few/many for Polish, Russian and Czech, six forms for Arabic, one form for Japanese. Without it the wrong form shows for some counts. Checked in .xcstrings variations and substitutions, .stringsdict, Android <plurals>, gettext msgstr[n], i18next _one/_few keys and ICU {n, plural, …}."
            }
            Code::Markup => {
                "A tag the source has (<b>, </a>, <br>) is missing from the translation, or the translation adds one. Attributes are ignored and self-closing tags collapse to their name; `a < b` is not a tag. A dropped closing tag breaks the rendering of everything after it."
            }
            Code::Escape => {
                "Android resource strings that aapt rejects or mangles: an apostrophe must be \\' (or the whole value wrapped in double quotes), a leading @ or ? that is not a @string/name reference is read as a resource reference, and a stray unbalanced double quote is silently dropped from the displayed text (a warning). Quotes inside tag attributes and CDATA are fine. The source file is checked too."
            }
            Code::Array => {
                "An Android <string-array> has a different number of items than the source. Arrays are positional: fewer items is an IndexOutOfBoundsException at runtime, more items show entries the source never had."
            }
            Code::Duplicate => {
                "The same key appears more than once in one file. The last one wins silently, whichever the translator meant. For Android the pair (resource type, name) counts, so a <string> and a <plurals> may share a name."
            }
            Code::Glossary => {
                "glossary.toml is not respected: a do_not_translate term was translated, or a term was not rendered as [terms.<locale>] requires. `polygo check --fix` re-translates it with the glossary in the prompt."
            }
            Code::Empty => {
                "The translation is blank. Most frameworks then show an empty string, not the source."
            }
            Code::Identical => {
                "The translation is exactly the source text. Acronyms, very short tokens and markup-only strings are not reported, nor is a string the model confirmed on a second ask (recorded in polygo.lock). If it is right as it is: polygo:ignore=identical in its comment."
            }
            Code::Fragment => {
                "Words of the source language remain in a translation into a non-Latin-script language (`ようこそ back!`). Handles, paths, HTML entities and hyphenated tokens are not counted."
            }
            Code::Length => {
                "The translation is longer than length_ratio (default 2.5) times the source plus 8 characters, or far shorter, or exceeds a polygo:max=N limit in the key's comment (that one is an error and is bounced back to the model)."
            }
            Code::Whitespace => {
                "A leading or trailing space or newline the source has is missing from the translation, or the translation adds one. Strings glued together in the UI lose their gap."
            }
            Code::Punctuation => {
                "The source ends with `:` `.` `!` `?` `…` and the translation ends with a letter: a menu item lost its ellipsis, a label its colon. Any script's marks (。 ؟ ।) and closing quotes count as punctuation; short labels and placeholder endings are not reported."
            }
            Code::Orphan => {
                "The locale file has a key the source file no longer has: a dead translation. Delete it, or restore the source key. i18next plural forms the locale needs (photos_few next to a source photos_other) are not orphans."
            }
            Code::Fuzzy => {
                "A gettext entry marked `#, fuzzy`: the msgstr is present but runtime shows the source text. Review the translation and drop the flag."
            }
            Code::State => {
                "A String Catalog unit Xcode marked needs_review, stale or new, shipped as is; or a key whose extractionState is stale (Xcode no longer finds it in the code)."
            }
            Code::Encoding => {
                "Text that looks like UTF-8 read as Latin-1/CP1252 and saved again: Ã© for é, â€™ for ’, Â for a non-breaking space. A file was re-encoded somewhere on the way."
            }
            Code::Invisible => {
                "A character that does not show but changes behaviour: a zero-width space inside a word, a byte-order mark in the middle of a string, U+2028/U+2029 line separators, bidi embedding controls left by an editor, C0 control characters. ZWNJ, ZWJ, LRM and RLM are left alone: Persian, Arabic and Indic text need them."
            }
            Code::Link => {
                "A URL or email address in the source is missing from the translation or replaced by another one. Xcode %#@var@ substitutions and markdown link syntax are not addresses; a source that is a key rather than text is not compared."
            }
            Code::Brackets => {
                "A bracket pair the source keeps balanced is unbalanced in the translation: `(` opened and never closed, `）` without `（`. Guillemets are not checked, since German writes »…« and French «…»."
            }
            Code::Entities => {
                "An HTML entity escaped twice (&amp;amp;, &amp;lt;): the screen shows &amp; instead of &."
            }
            Code::Inconsistent => {
                "The same short source term is translated differently across keys in one locale (`Settings` → `Réglages` in three keys, `Paramètres` in one); the minority is reported. Case and inflection (aucun/aucune) are the same word, untranslated copies do not vote, a tie is a choice. Noun/verb pairs (`Post` as a title vs a button) are worth a glance, then polygo:ignore=inconsistent."
            }
        }
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.as_str())
    }
}
