//! Structured compiler diagnostics for editor/front-end integrations.

use std::sync::Arc;

/// A source span in 1-based line/column coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    /// Exclusive end column when the diagnostic is on a single line.
    pub end_col: u32,
    /// Which source the position refers to: `None` is the user's own input;
    /// `Some` names a `copy` include (as written in the source) or a loaded
    /// library (`"circuits"`). Positions are always relative to their own
    /// source, never to a concatenated stream.
    pub file: Option<Arc<str>>,
}

impl Span {
    pub fn new(line: u32, col: u32, end_col: u32) -> Self {
        Self {
            line,
            col,
            end_col,
            file: None,
        }
    }

    pub fn in_file(mut self, file: Option<Arc<str>>) -> Self {
        self.file = file;
        self
    }
}

/// A structured diagnostic that can be consumed by live editors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub end_col: Option<u32>,
    /// See [`Span::file`]: `None` = the user's input, `Some` = an include or
    /// library name the position is relative to.
    pub file: Option<Arc<str>>,
    pub kind: String,
    pub found: Option<String>,
    pub expected: Option<String>,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            col: None,
            end_col: None,
            file: None,
            kind: kind.into(),
            found: None,
            expected: None,
            hint: None,
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.line = Some(span.line);
        self.col = Some(span.col);
        self.end_col = Some(span.end_col);
        self.file = span.file;
        self
    }

    pub fn expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    pub fn found(mut self, found: impl Into<String>) -> Self {
        self.found = Some(found.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// A failed compilation: the flat human-readable message (what the `Err(String)`
/// entry points return) plus the structured [`Diagnostic`] behind it, for
/// bindings that surface rich errors (editor integrations, exceptions with
/// attached position data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    /// Boxed to keep `Result<_, CompileError>` small (clippy result_large_err).
    pub info: Box<Diagnostic>,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}

/// The candidate closest to `word` within edit distance 2 (ties broken
/// alphabetically), for "did you mean …?" hints.
pub(crate) fn closest(word: &str, candidates: &'static [&'static str]) -> Option<&'static str> {
    candidates
        .iter()
        .copied()
        .filter_map(|candidate| {
            let dist = edit_distance(word, candidate);
            (dist <= 2).then_some((dist, candidate))
        })
        .min_by_key(|(dist, candidate)| (*dist, *candidate))
        .map(|(_, candidate)| candidate)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb {
                prev[j]
            } else {
                1 + prev[j].min(prev[j + 1]).min(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
