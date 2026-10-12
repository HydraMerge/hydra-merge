//! Line-based syntax highlighting for diff text. Pure functions, no egui and no I/O.
//!
//! The lexer knows comments, strings, numbers, keywords, literals, type names and config keys.
//! It keeps a small state between lines so block comments and triple-quoted strings span lines.

use std::ops::Range;

mod langs;

use langs::Syntax;
pub use langs::{lang_for_path, Lang};

/// Lines longer than this are shown plain (minified files, lock files with long hashes).
pub const MAX_HIGHLIGHT_BYTES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Type,
    String,
    Number,
    Comment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: TokenKind,
}

/// What an unfinished construct at the end of a line carries into the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineState {
    #[default]
    Normal,
    BlockComment(&'static str),
    MultiString(&'static str),
}

/// Tokens of one line. Spans are contiguous, non-empty and cover the whole line.
pub fn highlight_line(lang: Lang, line: &str, state: LineState) -> (Vec<Span>, LineState) {
    if lang == Lang::Plain {
        return (plain(line), LineState::Normal);
    }
    // An over-long line stays plain but does not close an open comment or string.
    if line.len() > MAX_HIGHLIGHT_BYTES {
        return (plain(line), state);
    }
    let mut lexer = Lexer {
        syntax: lang.syntax(),
        line,
        pos: 0,
        spans: Vec::new(),
        words_seen: 0,
    };
    let end_state = lexer.run(state);
    (merge(lexer.spans), end_state)
}

fn plain(line: &str) -> Vec<Span> {
    if line.is_empty() {
        return Vec::new();
    }
    vec![Span {
        range: 0..line.len(),
        kind: TokenKind::Plain,
    }]
}

/// Joins neighbouring spans of the same kind.
fn merge(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .fold(Vec::new(), |mut out: Vec<Span>, span| {
            match out.last_mut() {
                Some(last) if last.kind == span.kind && last.range.end == span.range.start => {
                    last.range.end = span.range.end;
                }
                _ => out.push(span),
            }
            out
        })
}

struct Lexer<'a> {
    syntax: &'static Syntax,
    line: &'a str,
    pos: usize,
    spans: Vec<Span>,
    /// Identifiers and strings already met on this line (keys are only the first one).
    words_seen: usize,
}

impl Lexer<'_> {
    fn run(&mut self, state: LineState) -> LineState {
        let mut state = state;
        while self.pos < self.line.len() {
            state = match state {
                LineState::Normal => self.step(),
                LineState::BlockComment(end) => self.until(end, TokenKind::Comment, state),
                LineState::MultiString(end) => self.until(end, TokenKind::String, state),
            };
        }
        state
    }

    fn rest(&self) -> &str {
        &self.line[self.pos..]
    }

    fn push(&mut self, len: usize, kind: TokenKind) {
        let end = (self.pos + len).min(self.line.len());
        if end > self.pos {
            self.spans.push(Span {
                range: self.pos..end,
                kind,
            });
        }
        self.pos = end;
    }

    /// Consumes up to and including `end`, or the whole line while still inside.
    fn until(&mut self, end: &'static str, kind: TokenKind, open: LineState) -> LineState {
        match self.rest().find(end) {
            Some(at) => {
                self.push(at + end.len(), kind);
                LineState::Normal
            }
            None => {
                self.push(self.rest().len(), kind);
                open
            }
        }
    }

    fn step(&mut self) -> LineState {
        let syntax = self.syntax;
        let rest = self.rest();
        if let Some(prefix) = syntax.line_comments.iter().find(|p| rest.starts_with(**p)) {
            if self.comment_allowed(prefix) {
                self.push(rest.len(), TokenKind::Comment);
                return LineState::Normal;
            }
        }
        if let Some((open, close)) = syntax.block_comment {
            if rest.starts_with(open) {
                self.push(open.len(), TokenKind::Comment);
                return LineState::BlockComment(close);
            }
        }
        if let Some(delim) = syntax.multi_strings.iter().find(|d| rest.starts_with(**d)) {
            self.push(delim.len(), TokenKind::String);
            return LineState::MultiString(delim);
        }
        let Some(c) = rest.chars().next() else {
            return LineState::Normal;
        };
        if syntax.quotes.contains(&c) {
            self.string(c);
        } else if c.is_ascii_digit() && !self.after_ident() {
            self.number();
        } else if syntax.tags && c == '<' {
            self.tag();
        } else if self.ident_start(c) {
            self.word();
        } else {
            self.push(c.len_utf8(), TokenKind::Plain);
        }
        LineState::Normal
    }

    /// `#` starts a comment only at line start or after whitespace (`$#`, `a#b`, `#{x}`).
    fn comment_allowed(&self, prefix: &str) -> bool {
        if prefix != "#" {
            return true;
        }
        let before = self.line[..self.pos].chars().next_back();
        let next = self.rest()[1..].chars().next();
        before.is_none_or(char::is_whitespace) && next != Some('{')
    }

    fn after_ident(&self) -> bool {
        self.line[..self.pos]
            .chars()
            .next_back()
            .is_some_and(|c| self.ident_char(c))
    }

    fn ident_start(&self, c: char) -> bool {
        c.is_alphabetic() || c == '_' || ((c == '@' || c == '#' || c == '!') && self.prefixed())
    }

    /// `@media`, `#include`, `!important` are listed as keywords with their sigil.
    fn prefixed(&self) -> bool {
        let rest = self.rest();
        self.syntax
            .keywords
            .split_whitespace()
            .any(|k| k.len() > 1 && !k.as_bytes()[0].is_ascii_alphanumeric() && rest.starts_with(k))
    }

    fn ident_char(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || self.syntax.ident_extra.contains(c)
    }

    fn string(&mut self, quote: char) {
        let rest = self.rest();
        let mut escaped = false;
        let mut len = rest.len();
        for (i, c) in rest.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if c == '\\' && quote != '`' {
                escaped = true;
            } else if c == quote {
                len = i + c.len_utf8();
                break;
            }
        }
        let is_key = self.words_seen == 0 && self.followed_by_key_sep(len);
        let kind = if is_key {
            TokenKind::Keyword
        } else {
            TokenKind::String
        };
        self.words_seen += 1;
        self.push(len, kind);
    }

    fn number(&mut self) {
        let len = self
            .rest()
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '.'))
            .map_or(self.rest().len(), |(i, _)| i);
        self.push(len, TokenKind::Number);
    }

    fn tag(&mut self) {
        let rest = self.rest();
        let skip = if rest.starts_with("</") { 2 } else { 1 };
        let name_len = rest[skip..]
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            .map_or(rest.len() - skip, |(i, _)| i);
        if name_len == 0 {
            self.push(1, TokenKind::Plain);
            return;
        }
        self.push(skip, TokenKind::Plain);
        self.push(name_len, TokenKind::Keyword);
    }

    fn word(&mut self) {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .skip(1)
            .find(|(_, c)| !self.ident_char(*c))
            .map_or(rest.len(), |(i, _)| i);
        let word = &rest[..len];
        let kind = self.word_kind(word, len);
        self.words_seen += 1;
        self.push(len, kind);
    }

    fn word_kind(&self, word: &str, len: usize) -> TokenKind {
        let syntax = self.syntax;
        let matches = |list: &str| {
            list.split_whitespace().any(|k| {
                if syntax.case_insensitive {
                    k.eq_ignore_ascii_case(word)
                } else {
                    k == word
                }
            })
        };
        let is_key = self.words_seen == 0 && self.followed_by_key_sep(len);
        if is_key || matches(syntax.keywords) {
            TokenKind::Keyword
        } else if matches(syntax.literals) {
            TokenKind::Number
        } else if syntax.types && is_type_name(word) {
            TokenKind::Type
        } else {
            TokenKind::Plain
        }
    }

    /// The token of `len` bytes at `pos` is followed by optional spaces and the key separator.
    fn followed_by_key_sep(&self, len: usize) -> bool {
        let Some(sep) = self.syntax.key_sep else {
            return false;
        };
        let mut after = self.rest()[len..].trim_start_matches([' ', '\t']).chars();
        // `a::b` and `a == b` are not keys.
        after.next() == Some(sep) && after.next() != Some(sep)
    }
}

/// `CamelCase` names; a single capital or `ALL_CAPS` constants are not types.
fn is_type_name(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.any(|c| c.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: Lang, line: &str) -> Vec<(String, TokenKind)> {
        let (spans, _) = highlight_line(lang, line, LineState::Normal);
        spans
            .into_iter()
            .map(|s| (line[s.range].to_string(), s.kind))
            .collect()
    }

    fn kind_of(lang: Lang, line: &str, needle: &str) -> Option<TokenKind> {
        kinds(lang, line)
            .into_iter()
            .find(|(text, _)| text == needle)
            .map(|(_, kind)| kind)
    }

    fn has_kind(lang: Lang, line: &str, kind: TokenKind) -> bool {
        kinds(lang, line).iter().any(|(_, k)| *k == kind)
    }

    #[test]
    fn spans_cover_the_whole_line() {
        let line = "let s = \"héllo\"; // привет";
        let (spans, _) = highlight_line(Lang::Rust, line, LineState::Normal);
        let mut at = 0;
        for span in &spans {
            assert_eq!(span.range.start, at);
            assert!(span.range.end > span.range.start);
            at = span.range.end;
        }
        assert_eq!(at, line.len());
    }

    #[test]
    fn rust_tokens() {
        let line = "pub fn run(x: u32) -> Option<String> { 42 } // done";
        assert_eq!(kind_of(Lang::Rust, line, "pub"), Some(TokenKind::Keyword));
        assert_eq!(kind_of(Lang::Rust, line, "Option"), Some(TokenKind::Type));
        assert_eq!(kind_of(Lang::Rust, line, "42"), Some(TokenKind::Number));
        assert_eq!(
            kind_of(Lang::Rust, line, "// done"),
            Some(TokenKind::Comment)
        );
        assert_eq!(
            kind_of(Lang::Rust, "a(\"x\\\"y\")", "\"x\\\"y\""),
            Some(TokenKind::String)
        );
    }

    #[test]
    fn identifiers_with_digits_are_not_numbers() {
        let numbers: Vec<String> = kinds(Lang::Rust, "let v2 = 1;")
            .into_iter()
            .filter(|(_, kind)| *kind == TokenKind::Number)
            .map(|(text, _)| text)
            .collect();
        assert_eq!(numbers, vec!["1".to_string()]);
    }

    #[test]
    fn block_comment_spans_lines() {
        let (first, state) = highlight_line(Lang::Go, "x := 1 /* start", LineState::Normal);
        assert_eq!(first.last().map(|s| s.kind), Some(TokenKind::Comment));
        assert_eq!(state, LineState::BlockComment("*/"));
        let (middle, state) = highlight_line(Lang::Go, "still comment", state);
        assert_eq!(middle.len(), 1);
        assert_eq!(middle[0].kind, TokenKind::Comment);
        let (last, state) = highlight_line(Lang::Go, "end */ return", state);
        assert_eq!(state, LineState::Normal);
        assert_eq!(last[0].kind, TokenKind::Comment);
        assert_eq!(last.last().map(|s| s.kind), Some(TokenKind::Keyword));
    }

    #[test]
    fn python_triple_quote_spans_lines() {
        let (_, state) = highlight_line(Lang::Python, "doc = \"\"\"first", LineState::Normal);
        assert_eq!(state, LineState::MultiString("\"\"\""));
        let (spans, state) = highlight_line(Lang::Python, "last\"\"\" + x", state);
        assert_eq!(state, LineState::Normal);
        assert_eq!(spans[0].kind, TokenKind::String);
    }

    #[test]
    fn yaml_keys_comments_and_literals() {
        let line = "  - name: install nginx # task";
        assert_eq!(kind_of(Lang::Yaml, line, "name"), Some(TokenKind::Keyword));
        assert_eq!(
            kind_of(Lang::Yaml, line, "# task"),
            Some(TokenKind::Comment)
        );
        assert_eq!(
            kind_of(Lang::Yaml, "become: yes", "yes"),
            Some(TokenKind::Number)
        );
        assert_eq!(
            kind_of(Lang::Yaml, "ansible-lint: x", "ansible-lint"),
            Some(TokenKind::Keyword)
        );
        assert!(!has_kind(Lang::Yaml, "url: http://h#x", TokenKind::Comment));
    }

    #[test]
    fn json_and_hcl_keys() {
        let line = "  \"name\": \"value\",";
        assert_eq!(
            kind_of(Lang::Json, line, "\"name\""),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::Json, line, "\"value\""),
            Some(TokenKind::String)
        );
        let hcl = "  instance_type = \"t3.micro\"";
        assert_eq!(
            kind_of(Lang::Hcl, hcl, "instance_type"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::Hcl, "resource \"aws\" \"x\" {", "resource"),
            Some(TokenKind::Keyword)
        );
        assert!(!has_kind(Lang::Hcl, "a == b", TokenKind::Keyword));
    }

    #[test]
    fn shell_hash_is_not_always_a_comment() {
        assert!(!has_kind(Lang::Shell, "echo $# args", TokenKind::Comment));
        assert_eq!(
            kind_of(Lang::Shell, "x=1 # note", "# note"),
            Some(TokenKind::Comment)
        );
    }

    #[test]
    fn sql_is_case_insensitive() {
        assert_eq!(
            kind_of(Lang::Sql, "SELECT id FROM t", "SELECT"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::Sql, "select 1 -- c", "-- c"),
            Some(TokenKind::Comment)
        );
    }

    #[test]
    fn markup_tags_and_sigil_keywords() {
        assert_eq!(
            kind_of(Lang::Markup, "<div class=\"a\">", "div"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::Markup, "</div>", "div"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::CLike, "#include <x.h>", "#include"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(
            kind_of(Lang::Css, "@media screen {", "@media"),
            Some(TokenKind::Keyword)
        );
    }

    #[test]
    fn long_and_plain_lines_are_not_tokenized() {
        let long = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        let (spans, state) = highlight_line(Lang::Rust, &long, LineState::BlockComment("*/"));
        assert_eq!(spans.len(), 1);
        assert_eq!(state, LineState::BlockComment("*/"));
        assert!(highlight_line(Lang::Plain, "", LineState::Normal)
            .0
            .is_empty());
    }

    #[test]
    fn unterminated_string_ends_at_line_end() {
        let line = "s = \"open";
        let (spans, state) = highlight_line(Lang::Python, line, LineState::Normal);
        assert_eq!(state, LineState::Normal);
        assert_eq!(spans.last().map(|s| s.kind), Some(TokenKind::String));
    }
}
