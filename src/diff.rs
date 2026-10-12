//! Parser of `git show -p` output into rows ready for drawing. Pure, no egui and no I/O.

use crate::highlight::{highlight_line, lang_for_path, Lang, LineState, Span, TokenKind};

const TAB_WIDTH: usize = 4;
/// Last line of a patch cut by `git::cap_patch`.
pub const TRUNCATED_MARKER: &str = "… truncated";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// One row per file: its path, and what happened to it.
    File,
    Hunk,
    Added,
    Removed,
    Context,
    /// `\ No newline at end of file`, binary files, truncation.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Line text without the `+`/`-`/` ` column, tabs expanded.
    pub text: String,
    /// Syntax tokens of `text`; empty for rows that are not code.
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffView {
    pub rows: Vec<DiffRow>,
    /// Widest `text` in characters, for the horizontal scroll extent.
    pub max_chars: usize,
    /// Largest line number, for the gutter width.
    pub max_line_no: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Binary,
}

#[derive(Clone, Debug)]
struct FileHeader {
    old: Option<String>,
    new: Option<String>,
    status: FileStatus,
}

#[derive(Clone, Copy, Debug)]
struct Hunk {
    old_no: Option<u32>,
    new_no: u32,
    /// Number of `+`/`-` columns: 1 for a plain diff, the parent count for `diff --cc`.
    columns: usize,
    old_state: LineState,
    new_state: LineState,
}

/// Builds rows from the patch text. Lines it does not understand are kept as notes.
pub fn parse_diff(text: &str) -> DiffView {
    let mut parser = Parser::default();
    for line in text.lines() {
        parser.line(line.strip_suffix('\r').unwrap_or(line));
    }
    parser.finish()
}

#[derive(Default)]
struct Parser {
    rows: Vec<DiffRow>,
    header: Option<FileHeader>,
    lang: Option<Lang>,
    hunk: Option<Hunk>,
}

impl Parser {
    fn line(&mut self, line: &str) {
        if line == TRUNCATED_MARKER {
            self.note(line);
        } else if line.starts_with("diff ") {
            self.start_file(line);
        } else if line.starts_with("@@") {
            self.start_hunk(line);
        } else if self.hunk.is_some() {
            self.hunk_line(line);
        } else if self.header.is_some() {
            self.header_line(line);
        } else if !line.trim().is_empty() {
            self.note(line);
        }
    }

    fn finish(mut self) -> DiffView {
        self.flush_header();
        let max_chars = self
            .rows
            .iter()
            .map(|row| row.text.chars().count())
            .max()
            .unwrap_or(0);
        let max_line_no = self
            .rows
            .iter()
            .flat_map(|row| [row.old_no, row.new_no])
            .flatten()
            .max()
            .unwrap_or(0);
        DiffView {
            rows: self.rows,
            max_chars,
            max_line_no,
        }
    }

    fn start_file(&mut self, line: &str) {
        self.flush_header();
        self.hunk = None;
        self.lang = None;
        let (old, new) = paths_from_diff_line(line);
        self.header = Some(FileHeader {
            old,
            new,
            status: FileStatus::Modified,
        });
    }

    fn header_line(&mut self, line: &str) {
        let Some(header) = self.header.as_mut() else {
            return;
        };
        if let Some(path) = line.strip_prefix("--- ") {
            header.old = side_path(path, "a/");
        } else if let Some(path) = line.strip_prefix("+++ ") {
            header.new = side_path(path, "b/");
        } else if line.starts_with("new file mode") {
            header.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode") {
            header.status = FileStatus::Deleted;
        } else if let Some(path) = line.strip_prefix("rename from ") {
            header.old = Some(unquote(path));
            header.status = FileStatus::Renamed;
        } else if let Some(path) = line.strip_prefix("rename to ") {
            header.new = Some(unquote(path));
            header.status = FileStatus::Renamed;
        } else if line.starts_with("Binary files ") {
            header.status = FileStatus::Binary;
        }
    }

    /// Emits the file row once its header lines are read.
    fn flush_header(&mut self) {
        let Some(header) = self.header.take() else {
            return;
        };
        let path = header
            .new
            .clone()
            .or_else(|| header.old.clone())
            .unwrap_or_default();
        self.lang = Some(lang_for_path(&path));
        self.push(RowKind::File, None, None, file_title(&header), Vec::new());
        if header.status == FileStatus::Binary {
            self.note("Binary file not shown");
        }
    }

    fn start_hunk(&mut self, line: &str) {
        self.flush_header();
        let columns = line
            .chars()
            .take_while(|c| *c == '@')
            .count()
            .saturating_sub(1);
        let (old_no, new_no) = parse_hunk_header(line);
        self.hunk = Some(Hunk {
            old_no: old_no.filter(|_| columns == 1),
            new_no,
            columns: columns.max(1),
            old_state: LineState::Normal,
            new_state: LineState::Normal,
        });
        self.push(RowKind::Hunk, None, None, expand_tabs(line), Vec::new());
    }

    fn hunk_line(&mut self, line: &str) {
        let Some(hunk) = self.hunk else {
            return;
        };
        if line.starts_with('\\') {
            self.note(line);
            return;
        }
        let split = line
            .char_indices()
            .nth(hunk.columns)
            .map_or(line.len(), |(i, _)| i);
        let (marks, body) = line.split_at(split);
        let kind = if marks.contains('-') {
            RowKind::Removed
        } else if marks.contains('+') {
            RowKind::Added
        } else {
            RowKind::Context
        };
        let text = expand_tabs(body);
        let lang = self.lang.unwrap_or(Lang::Plain);
        let state_in = match kind {
            RowKind::Removed => hunk.old_state,
            _ => hunk.new_state,
        };
        let (spans, state_out) = highlight_line(lang, &text, state_in);
        let (old_no, new_no) = numbers(&hunk, kind);
        self.hunk = Some(advance(hunk, kind, state_out));
        self.push(kind, old_no, new_no, text, spans);
    }

    fn note(&mut self, line: &str) {
        self.push(RowKind::Note, None, None, expand_tabs(line), Vec::new());
    }

    fn push(
        &mut self,
        kind: RowKind,
        old_no: Option<u32>,
        new_no: Option<u32>,
        text: String,
        spans: Vec<Span>,
    ) {
        self.rows.push(DiffRow {
            kind,
            old_no,
            new_no,
            text,
            spans,
        });
    }
}

fn numbers(hunk: &Hunk, kind: RowKind) -> (Option<u32>, Option<u32>) {
    match kind {
        RowKind::Removed => (hunk.old_no, None),
        RowKind::Added => (None, Some(hunk.new_no)),
        _ => (hunk.old_no, Some(hunk.new_no)),
    }
}

/// Next line numbers and lexer states. A context line belongs to both sides.
fn advance(hunk: Hunk, kind: RowKind, state: LineState) -> Hunk {
    let next_old = hunk.old_no.map(|n| n.saturating_add(1));
    match kind {
        RowKind::Removed => Hunk {
            old_no: next_old,
            old_state: state,
            ..hunk
        },
        RowKind::Added => Hunk {
            new_no: hunk.new_no.saturating_add(1),
            new_state: state,
            ..hunk
        },
        _ => Hunk {
            old_no: next_old,
            new_no: hunk.new_no.saturating_add(1),
            old_state: state,
            new_state: state,
            ..hunk
        },
    }
}

/// `@@ -12,3 +14,5 @@ fn x()` gives (12, 14). `@@@ -1 -1 +1 @@@` gives the last range as new.
fn parse_hunk_header(line: &str) -> (Option<u32>, u32) {
    let ranges = line
        .trim_start_matches('@')
        .split(" @@")
        .next()
        .unwrap_or("");
    let start = |item: &str| item.split(',').next().and_then(|n| n.parse::<u32>().ok());
    let old = ranges
        .split_whitespace()
        .find_map(|item| item.strip_prefix('-'))
        .and_then(start);
    let new = ranges
        .split_whitespace()
        .find_map(|item| item.strip_prefix('+'))
        .and_then(start)
        .unwrap_or(1);
    (old, new)
}

fn file_title(header: &FileHeader) -> String {
    let old = header.old.as_deref().unwrap_or("");
    let new = header.new.as_deref().unwrap_or("");
    match header.status {
        FileStatus::Added => format!("{new}  (new file)"),
        FileStatus::Deleted => format!("{old}  (deleted)"),
        FileStatus::Renamed if old != new => format!("{old} → {new}"),
        FileStatus::Binary => format!("{}  (binary)", if new.is_empty() { old } else { new }),
        _ if new.is_empty() => old.to_string(),
        _ => new.to_string(),
    }
}

/// Paths from `diff --git a/x b/y` or `diff --cc x`. Exact paths come later from `---`/`+++`.
fn paths_from_diff_line(line: &str) -> (Option<String>, Option<String>) {
    for prefix in ["diff --cc ", "diff --combined "] {
        if let Some(path) = line.strip_prefix(prefix) {
            let path = unquote(path);
            return (Some(path.clone()), Some(path));
        }
    }
    let rest = line.strip_prefix("diff --git ").unwrap_or("");
    if rest.starts_with('"') {
        let mut parts = rest.splitn(2, "\" ");
        let old = parts.next().map(|p| format!("{p}\""));
        let new = parts.next();
        return (
            old.and_then(|p| side_path(&p, "a/")),
            new.and_then(|p| side_path(p, "b/")),
        );
    }
    match rest.split_once(" b/") {
        Some((old, new)) => (
            old.strip_prefix("a/").map(str::to_string),
            Some(new.to_string()),
        ),
        None => (None, None),
    }
}

/// Path from a `---`/`+++` line without the `a/`/`b/` prefix. `/dev/null` is no path.
fn side_path(raw: &str, prefix: &str) -> Option<String> {
    let path = unquote(raw.trim_end_matches('\t'));
    if path == "/dev/null" {
        return None;
    }
    Some(path.strip_prefix(prefix).unwrap_or(&path).to_string())
}

/// Git quotes paths with special or non-ASCII bytes: `"a/\320\277.txt"`.
pub fn unquote(raw: &str) -> String {
    let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return raw.to_string();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let (byte, used) = unescape(&bytes[i + 1..]);
        out.push(byte);
        i += 1 + used;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One escape after a backslash: the byte and how many input bytes it used.
fn unescape(rest: &[u8]) -> (u8, usize) {
    let octal = rest
        .iter()
        .take(3)
        .take_while(|b| (b'0'..=b'7').contains(*b))
        .count();
    if octal == 3 {
        let value = rest[..3]
            .iter()
            .fold(0u32, |acc, b| acc * 8 + u32::from(b - b'0'));
        return (u8::try_from(value).unwrap_or(b'?'), 3);
    }
    let byte = match rest[0] {
        b'n' => b'\n',
        b't' => b'\t',
        b'a' => 0x07,
        b'b' => 0x08,
        b'f' => 0x0c,
        b'r' => b'\r',
        b'v' => 0x0b,
        other => other,
    };
    (byte, 1)
}

fn expand_tabs(text: &str) -> String {
    if !text.contains('\t') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let width = TAB_WIDTH - column % TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// True when a row has syntax colours worth drawing token by token.
pub fn has_tokens(row: &DiffRow) -> bool {
    row.spans.iter().any(|span| span.kind != TokenKind::Plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,4 +10,5 @@ pub mod app;
 pub mod git;
-pub mod old;
+pub mod diff;
+pub mod highlight;
 pub mod theme;
\\ No newline at end of file
diff --git a/notes.txt b/notes.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/notes.txt
@@ -0,0 +1 @@
+hello
diff --git a/img.png b/img.png
index 4444444..5555555 100644
Binary files a/img.png and b/img.png differ
";

    fn kinds(view: &DiffView) -> Vec<RowKind> {
        view.rows.iter().map(|row| row.kind).collect()
    }

    #[test]
    fn rows_and_line_numbers() {
        let view = parse_diff(PATCH);
        use RowKind::*;
        assert_eq!(
            kinds(&view),
            vec![
                File, Hunk, Context, Removed, Added, Added, Context, Note, File, Hunk, Added, File,
                Note
            ]
        );
        let rows = &view.rows;
        assert_eq!(rows[0].text, "src/lib.rs");
        assert_eq!((rows[2].old_no, rows[2].new_no), (Some(10), Some(10)));
        assert_eq!((rows[3].old_no, rows[3].new_no), (Some(11), None));
        assert_eq!((rows[4].old_no, rows[4].new_no), (None, Some(11)));
        assert_eq!((rows[5].old_no, rows[5].new_no), (None, Some(12)));
        assert_eq!((rows[6].old_no, rows[6].new_no), (Some(12), Some(13)));
        assert_eq!(rows[4].text, "pub mod diff;");
        assert_eq!(rows[8].text, "notes.txt  (new file)");
        assert_eq!(rows[11].text, "img.png  (binary)");
        assert_eq!(view.max_line_no, 13);
    }

    #[test]
    fn code_rows_are_highlighted_by_file_language() {
        let view = parse_diff(PATCH);
        assert!(has_tokens(&view.rows[4]), "rust keyword in an added line");
        assert!(!has_tokens(&view.rows[10]), "plain text file");
        assert!(view.rows[1].spans.is_empty(), "hunk header has no tokens");
    }

    #[test]
    fn removed_and_added_sides_keep_separate_comment_state() {
        let patch = "diff --git a/a.go b/a.go\n--- a/a.go\n+++ b/a.go\n@@ -1,2 +1,2 @@\n-/* open\n+x := 1\n inside\n";
        let view = parse_diff(patch);
        let last = view.rows.last().expect("row");
        // The context line follows the new side, where no comment is open.
        assert!(last.spans.iter().all(|s| s.kind != TokenKind::Comment));
    }

    #[test]
    fn combined_diff_of_a_merge() {
        let patch = "diff --cc a.txt\nindex 1,2..3\n--- a/a.txt\n+++ b/a.txt\n@@@ -1,1 -1,1 +1,2 @@@\n- one\n +two\n++both\n  same\n";
        let view = parse_diff(patch);
        use RowKind::*;
        assert_eq!(
            kinds(&view),
            vec![File, Hunk, Removed, Added, Added, Context]
        );
        assert_eq!(view.rows[3].text, "two");
        assert_eq!(view.rows[4].new_no, Some(2));
        assert_eq!(view.rows[5].old_no, None);
    }

    #[test]
    fn renames_and_quoted_paths() {
        let patch = "diff --git a/old.rs b/new.rs\nsimilarity index 90%\nrename from old.rs\nrename to new.rs\n";
        assert_eq!(parse_diff(patch).rows[0].text, "old.rs → new.rs");
        assert_eq!(unquote("\"a/\\320\\277.txt\""), "a/п.txt");
        assert_eq!(unquote("\"tab\\there\""), "tab\there");
        assert_eq!(unquote("plain"), "plain");
        let deleted = "diff --git a/x.py b/x.py\ndeleted file mode 100644\n--- a/x.py\n+++ /dev/null\n@@ -1 +0,0 @@\n-x = 1\n";
        let view = parse_diff(deleted);
        assert_eq!(view.rows[0].text, "x.py  (deleted)");
        assert!(has_tokens(&view.rows[2]));
    }

    #[test]
    fn tabs_are_expanded_and_text_without_header_is_a_note() {
        let view = parse_diff("… truncated\n");
        assert_eq!(kinds(&view), vec![RowKind::Note]);
        assert_eq!(expand_tabs("\ta\tb"), "    a   b");
        assert!(parse_diff("").rows.is_empty());
    }

    #[test]
    fn truncation_inside_a_hunk_is_a_note() {
        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n+x\n… truncated\n";
        let last = parse_diff(patch).rows.pop().expect("row");
        assert_eq!(last.kind, RowKind::Note);
        assert_eq!((last.old_no, last.new_no), (None, None));
    }

    #[test]
    fn huge_line_numbers_do_not_overflow() {
        let patch = "diff --git a/a b/a\n@@ -4294967295 +4294967295 @@\n x\n y\n";
        assert_eq!(parse_diff(patch).max_line_no, u32::MAX);
    }

    #[test]
    fn hunk_header_numbers() {
        assert_eq!(
            parse_hunk_header("@@ -12,3 +14,5 @@ fn x()"),
            (Some(12), 14)
        );
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), (Some(1), 1));
        assert_eq!(parse_hunk_header("@@@ -1,2 -3,4 +5,6 @@@"), (Some(1), 5));
    }
}
