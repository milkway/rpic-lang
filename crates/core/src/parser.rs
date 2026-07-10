//! Recursive-descent parser for the pic drawing core.
//!
//! Follows dpic's `grammar.txt`. Implemented: pictures (`.PS … .PE`), primitives
//! with the full attribute set, positions (pairs, places, corners, ordinals,
//! `between`, `± shifts`), expressions with proper precedence, `[ … ]` blocks,
//! `{ … }` groups, labels, assignments, macros, includes, conditionals, loops,
//! `print` and `exec`.

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{LexError, Spanned, lex, lex_named};
use crate::token::*;

/// A parse error with source location. `file` (via [`ParseError::span`]) is
/// `None` for the user's own input, or the `copy` include / library name the
/// position is relative to.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    file: Option<std::sync::Arc<str>>,
    detail: Box<ParseErrorDetail>,
}

#[derive(Debug, Clone, PartialEq)]
struct ParseErrorDetail {
    kind: String,
    found: Option<String>,
    expected: Option<String>,
    hint: Option<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{}:{}:{}: {}", file, self.line, self.col, self.msg),
            None => write!(f, "{}:{}: {}", self.line, self.col, self.msg),
        }
    }
}

impl ParseError {
    fn new(msg: impl Into<String>, span: Span) -> Self {
        Self {
            msg: msg.into(),
            line: span.line,
            col: span.col,
            end_col: span.end_col,
            file: span.file,
            detail: Box::new(ParseErrorDetail {
                kind: "parse".into(),
                found: None,
                expected: None,
                hint: None,
            }),
        }
    }

    fn expected(expected: impl Into<String>, found: impl Into<String>, span: Span) -> Self {
        let expected = expected.into();
        let found = found.into();
        Self {
            msg: format!("expected {expected}, found {found}"),
            line: span.line,
            col: span.col,
            end_col: span.end_col,
            file: span.file,
            detail: Box::new(ParseErrorDetail {
                kind: "expected_token".into(),
                found: Some(found),
                expected: Some(expected),
                hint: None,
            }),
        }
    }

    /// Override the diagnostic kind (crate-internal builder).
    pub(crate) fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.detail.kind = kind.into();
        self
    }

    /// The error's source span, including which source it refers to.
    pub fn span(&self) -> Span {
        Span::new(self.line, self.col, self.end_col).in_file(self.file.clone())
    }

    pub fn diagnostic(&self) -> Diagnostic {
        let mut d = Diagnostic::new(self.detail.kind.clone(), self.msg.clone()).at(self.span());
        d.found = self.detail.found.clone();
        d.expected = self.detail.expected.clone();
        d.hint = self.detail.hint.clone();
        d
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            msg: e.msg,
            line: e.line,
            col: e.col,
            end_col: e.end_col,
            file: e.file,
            detail: Box::new(ParseErrorDetail {
                kind: e.kind,
                found: None,
                expected: None,
                hint: None,
            }),
        }
    }
}

/// Parse a full source string into a [`Picture`] with no filesystem context.
/// `copy "file"` includes are unavailable (they require a base directory).
pub fn parse(src: &str) -> Result<Picture, ParseError> {
    parse_in_dir(src, None)
}

/// Parse pic source, resolving `copy "file"` includes relative to `base`
/// with the default (unrestricted) include policy.
pub fn parse_in_dir(src: &str, base: Option<&Path>) -> Result<Picture, ParseError> {
    parse_with_prelude(
        src,
        IncludeCtx::unrestricted(base.map(|p| p.to_path_buf())),
        false,
        false,
    )
}

/// Parse pic source with optional preludes: `circuits` loads the embedded
/// circuit-element library and `texlabels` injects `texlabels = 1` — the
/// library equivalents of the CLI `-c` / `-t` flags. Each prelude is lexed
/// as its own named source unit (not text glued in front of `src`), so every
/// diagnostic position stays relative to the source it belongs to: the user's
/// own input reports user lines, and library problems name the library.
pub fn parse_with_prelude(
    src: &str,
    includes: IncludeCtx,
    circuits: bool,
    texlabels: bool,
) -> Result<Picture, ParseError> {
    let mut toks: Vec<Spanned> = Vec::new();
    if circuits {
        splice_unit(&mut toks, lex_named(crate::CIRCUITS, "circuits")?);
    }
    if texlabels {
        // Initializer only — the source stays sovereign (`texlabels = 0` wins).
        splice_unit(&mut toks, lex_named("texlabels = 1\n", "<texlabels>")?);
    }
    let src = strip_backend_preamble(src);
    toks.extend(lex(&src)?);
    let (toks, macros) = preprocess(toks, &includes)?;
    let mut pic = Parser::new(toks).parse_picture()?;
    pic.macros = macros;
    pic.includes = includes;
    Ok(pic)
}

/// Append a lexed prelude unit ahead of the tokens that follow: drop its
/// `Eof` and guarantee a trailing `Newline` so the units stay statement-
/// separated (equivalent to the `\n` the old string-prepending inserted).
fn splice_unit(out: &mut Vec<Spanned>, mut unit: Vec<Spanned>) {
    if matches!(unit.last().map(|s| &s.tok), Some(Token::Eof)) {
        unit.pop();
    }
    if !matches!(unit.last().map(|s| &s.tok), Some(Token::Newline)) {
        let (line, file) = unit
            .last()
            .map(|s| (s.line, s.file.clone()))
            .unwrap_or((1, None));
        unit.push(Spanned::new(Token::Newline, line, 1).with_file(file));
    }
    out.extend(unit);
}

/// Parse a deferred body (the raw tokens of an `if`/`for` block) with the macro
/// table in scope, expanding macro calls (and `copy` includes) along this
/// executed path. Used by the evaluator so dead branches and recursive macros
/// are never parsed.
pub fn parse_body_tokens(
    toks: &[Spanned],
    macros: &mut Macros,
    includes: &IncludeCtx,
) -> Result<Vec<Stmt>, ParseError> {
    let before = body_macro_frame(toks).unwrap_or_else(|| macros.clone());
    let mut m = before.clone();
    let mut input = toks.to_vec();
    input.push(Spanned::new(Token::Eof, 0, 0));
    let expanded = expand(&input, &mut m, 0, includes)?;
    propagate_macro_changes(macros, &before, &m);
    let mut p = Parser::new(expanded);
    p.parse_elementlist(&[])
}

fn body_macro_frame(toks: &[Spanned]) -> Option<Macros> {
    toks.iter()
        .find_map(|s| s.macro_frame.as_ref().map(|m| m.as_ref().clone()))
}

fn propagate_macro_changes(macros: &mut Macros, before: &Macros, after: &Macros) {
    for name in before.keys() {
        if !after.contains_key(name) {
            macros.remove(name);
        }
    }
    for (name, body) in after {
        if before.get(name) != Some(body) {
            macros.insert(name.clone(), body.clone());
        }
    }
}

/// Parse pic source produced by `exec`, applying the caller's macro argument
/// frame before normal macro expansion.
pub(crate) fn parse_exec_source(
    src: &str,
    macros: &mut Macros,
    includes: &IncludeCtx,
    arg_frame: Option<&[Vec<Spanned>]>,
) -> Result<Vec<Stmt>, ParseError> {
    let mut toks = lex(src)?;
    if let Some(args) = arg_frame {
        toks = substitute(&toks, args);
    }
    // Expand against the caller's live macro table: a `define` inside the
    // exec'd text must persist after it, like dpic's — that's how the dpic
    // suite's `DefineRGBColor` registers colour macros through `case`/`exec`.
    let expanded = expand(&toks, macros, 0, includes)?;
    let mut p = Parser::new(expanded);
    p.parse_elementlist(&[])
}

pub(crate) fn parse_stringexpr_tokens(
    toks: &[Spanned],
    macros: &mut Macros,
    includes: &IncludeCtx,
) -> Result<StringExpr, ParseError> {
    let mut input = toks.to_vec();
    input.push(Spanned::new(Token::Eof, 0, 0));
    let expanded = expand(&input, macros, 0, includes)?;
    let mut p = Parser::new(expanded);
    p.skip_newlines();
    let expr = p.parse_stringexpr()?;
    p.skip_newlines();
    if !p.at(&Token::Eof) {
        return p.err(format!("unexpected {:?} after string expression", p.cur()));
    }
    Ok(expr)
}

// ---- backend preamble filter ----------------------------------------------

/// Drop non-SVG backend snippets commonly embedded in dpic examples.
///
/// These TeX/PSTricks preambles are meaningful to other backends, but for rpic's
/// SVG output they should be tolerated as no-ops. Replacing ignored lines with
/// empty lines keeps subsequent diagnostics on the original line numbers.
fn strip_backend_preamble(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_verbatimtex = false;
    let mut in_string = false;
    let mut in_raw_sh = false;

    for line in src.lines() {
        let trimmed = line.trim_start();
        if in_verbatimtex {
            out.push('\n');
            if starts_word(trimmed, "etex") {
                in_verbatimtex = false;
            }
            continue;
        }

        if in_raw_sh {
            out.push_str(line);
            out.push('\n');
            in_raw_sh = line_continues(line);
            continue;
        }

        if !in_string && starts_word(trimmed, "verbatimtex") {
            in_verbatimtex = true;
            out.push('\n');
        } else if !in_string && is_ignored_backend_line(trimmed) {
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
            if starts_word(trimmed, "sh") {
                in_raw_sh = line_continues(line);
            } else {
                in_string = update_string_state(line, in_string);
            }
        }
    }
    out
}

fn line_continues(line: &str) -> bool {
    line.trim_end_matches([' ', '\t', '\r']).ends_with('\\')
}

fn update_string_state(line: &str, mut in_string: bool) -> bool {
    let mut slashes = 0usize;
    for c in line.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' && slashes.is_multiple_of(2) {
            in_string = !in_string;
        }
        slashes = 0;
    }
    in_string
}

fn is_ignored_backend_line(trimmed: &str) -> bool {
    trimmed.starts_with("\\global") || trimmed.starts_with("\\psset")
}

fn starts_word(s: &str, word: &str) -> bool {
    let Some(rest) = s.strip_prefix(word) else {
        return false;
    };
    !rest
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

// ---- macro preprocessor ----------------------------------------------------
//
// Handles `define name { body }` (brace-delimited) with `$1..$9` argument
// substitution at the token level, before parsing. Invocations `name(a, b)` (or
// bare `name`) are replaced by the body with arguments spliced in; the result is
// re-expanded so macros may call macros. `undef name` removes a definition.

use std::collections::HashMap;
use std::path::Path;

fn preprocess(
    input: Vec<Spanned>,
    includes: &IncludeCtx,
) -> Result<(Vec<Spanned>, Macros), ParseError> {
    let mut macros: Macros = builtin_unit_macros();
    let out = expand(&input, &mut macros, 0, includes)?;
    Ok((out, macros))
}

/// dpic's absolute-unit suffix macros (`11bp__` → `11*(scale/72)`), predefined so
/// examples that use them without `copy`ing dpictools still work. A user
/// `define` of the same name overrides these.
fn builtin_unit_macros() -> Macros {
    let defs = [
        ("bp__", "*(scale/72)"),       // Adobe big point
        ("pt__", "*(scale/72.27)"),    // TeX point
        ("pc__", "*(12*scale/72.27)"), // pica
        ("in__", "*scale"),            // inch
        ("cm__", "*(scale/2.54)"),     // centimetre
        ("mm__", "*(scale/25.4)"),     // millimetre
        ("px__", "*(scale/96)"),       // pixel (96 dpi)
    ];
    let mut m = Macros::new();
    for (name, body) in defs {
        if let Ok(toks) = lex(body) {
            let body_toks: Vec<Spanned> =
                toks.into_iter().filter(|s| s.tok != Token::Eof).collect();
            m.insert(name.to_string(), body_toks);
        }
    }
    m
}

fn loc(toks: &[Spanned], i: usize) -> Span {
    toks.get(i)
        .map(|s| s.span())
        .unwrap_or_else(|| Span::new(0, 0, 0))
}

fn expand(
    toks: &[Spanned],
    macros: &mut HashMap<String, Vec<Spanned>>,
    depth: usize,
    includes: &IncludeCtx,
) -> Result<Vec<Spanned>, ParseError> {
    if depth > 64 {
        return Err(ParseError::new(
            "macro expansion too deep (recursive define?)",
            Span::new(0, 0, 0),
        ));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i].tok {
            Token::Kw(Kw::Define) => {
                let span = loc(toks, i);
                i += 1;
                let name = match toks.get(i).map(|s| &s.tok) {
                    Some(Token::Name(n)) | Some(Token::Label(n)) => n.clone(),
                    _ => {
                        return Err(ParseError::new(
                            "define: expected a macro name",
                            span.clone(),
                        ));
                    }
                };
                i += 1;
                // the macro body delimiter may begin on a following line
                while toks.get(i).map(|s| &s.tok) == Some(&Token::Newline) {
                    i += 1;
                }
                let Some(delim) = toks.get(i).map(|s| s.tok.clone()) else {
                    return Err(ParseError::new(
                        "define: expected a body delimiter",
                        span.clone(),
                    ));
                };
                let body = if delim == Token::LeftBrace {
                    i += 1; // past `{`
                    let start = i;
                    let mut bd = 1;
                    while i < toks.len() && bd > 0 {
                        match &toks[i].tok {
                            Token::LeftBrace => bd += 1,
                            Token::RightBrace => {
                                bd -= 1;
                                if bd == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    if bd != 0 {
                        return Err(ParseError::new(
                            "define: unterminated `{` body",
                            span.clone(),
                        ));
                    }
                    let body = trim_edge_newlines(&toks[start..i]);
                    i += 1; // past `}`
                    body
                } else {
                    if matches!(delim, Token::Eof | Token::Newline) {
                        let span = loc(toks, i);
                        return Err(ParseError::new(
                            "define: expected a body delimiter",
                            span.clone(),
                        ));
                    }
                    i += 1; // past delimiter
                    let start = i;
                    while i < toks.len() && toks[i].tok != delim {
                        i += 1;
                    }
                    if i >= toks.len() {
                        return Err(ParseError::new(
                            "define: unterminated delimited body",
                            span.clone(),
                        ));
                    }
                    let body = trim_edge_newlines(&toks[start..i]);
                    i += 1; // past closing delimiter
                    body
                };
                macros.insert(name, body);
            }
            Token::Kw(Kw::Undef) => {
                i += 1;
                if let Some(Token::Name(n)) | Some(Token::Label(n)) = toks.get(i).map(|s| &s.tok) {
                    macros.remove(n);
                }
                i += 1;
            }
            // `if`/`for` bodies are copied verbatim (macro calls inside are not
            // expanded here): they are expanded lazily, by the evaluator, only
            // along the branch/iteration that actually runs. The condition/range
            // is still expanded so macros there work.
            Token::Kw(Kw::If) => {
                let if_tok = toks[i].clone();
                out.push(toks[i].clone());
                i += 1;
                let Some(te) = find_kw_depth0(toks, i, Kw::Then) else {
                    continue; // malformed; let the parser report it
                };
                let cond = expand(&toks[i..te], macros, depth + 1, includes)?;
                if let Some((then_body, after_then)) = read_braced_body(toks, te + 1)? {
                    let mut j = after_then;
                    while matches!(toks.get(j).map(|s| &s.tok), Some(Token::Newline)) {
                        j += 1;
                    }
                    let else_body =
                        if matches!(toks.get(j).map(|s| &s.tok), Some(Token::Kw(Kw::Else))) {
                            read_braced_body(toks, j + 1)?
                        } else {
                            None
                        };
                    if let Some(take_then) = static_truth(&cond) {
                        let after_static = else_body
                            .as_ref()
                            .map(|(_, after)| *after)
                            .unwrap_or(after_then);
                        out.pop(); // discard the speculative `if`
                        if take_then {
                            out.extend(expand(&then_body, macros, depth + 1, includes)?);
                            i = after_static;
                        } else if let Some((body, after)) = else_body {
                            out.extend(expand(&body, macros, depth + 1, includes)?);
                            i = after;
                        } else {
                            i = after_then;
                        }
                        continue;
                    }
                }
                *out.last_mut().unwrap() = if_tok;
                out.extend(cond);
                out.push(toks[te].clone()); // `then`
                i = copy_braced(toks, te + 1, &mut out, macros)?;
                // optional `else { … }` (possibly across newlines)
                let mut j = i;
                while matches!(toks.get(j).map(|s| &s.tok), Some(Token::Newline)) {
                    j += 1;
                }
                if matches!(toks.get(j).map(|s| &s.tok), Some(Token::Kw(Kw::Else))) {
                    out.push(toks[j].clone());
                    i = copy_braced(toks, j + 1, &mut out, macros)?;
                }
            }
            Token::Kw(Kw::For) => {
                out.push(toks[i].clone());
                i += 1;
                let Some(de) = find_kw_depth0(toks, i, Kw::Do) else {
                    continue;
                };
                out.extend(expand(&toks[i..de], macros, depth + 1, includes)?);
                out.push(toks[de].clone()); // `do`
                i = copy_braced(toks, de + 1, &mut out, macros)?;
            }
            Token::Name(n) | Token::Label(n) if macros.contains_key(n) => {
                let body = macros.get(n).unwrap().clone();
                i += 1;
                let args = if toks.get(i).map(|s| &s.tok) == Some(&Token::Lparen) {
                    i += 1;
                    let (a, ni) = read_args(toks, i)?;
                    i = ni;
                    a
                } else {
                    Vec::new()
                };
                let sub = substitute(&body, &args);
                let expanded = expand(&sub, macros, depth + 1, includes)?;
                out.extend(expanded);
            }
            // `copy "file"` splices another pic file's (expanded) tokens inline.
            Token::Kw(Kw::Copy) => {
                let span = loc(toks, i);
                i += 1;
                let Some(Token::Str(fname)) = toks.get(i).map(|s| &s.tok) else {
                    return Err(ParseError::new(
                        "copy: expected a quoted file name (only `copy \"file\"` is supported)",
                        span.clone(),
                    ));
                };
                let fname = fname.clone();
                i += 1;
                let inc = include_file(includes, &fname, macros, depth, span)?;
                out.extend(inc);
            }
            _ => {
                out.push(toks[i].clone());
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Read comma-separated argument token-lists after a `(` (index `i` is just past
/// it), returning the args and the index after the matching `)`.
fn read_args(toks: &[Spanned], mut i: usize) -> Result<(Vec<Vec<Spanned>>, usize), ParseError> {
    let mut args: Vec<Vec<Spanned>> = Vec::new();
    let mut cur: Vec<Spanned> = Vec::new();
    let mut depth = 0i32;
    loop {
        let Some(s) = toks.get(i) else {
            return Err(ParseError::new(
                "unterminated macro arguments",
                Span::new(0, 0, 0),
            ));
        };
        match &s.tok {
            Token::Lparen | Token::LeftBrack | Token::LeftBrace => {
                depth += 1;
                cur.push(s.clone());
                i += 1;
            }
            Token::Rparen if depth == 0 => {
                i += 1;
                trim_trailing_newlines(&mut cur);
                if !cur.is_empty() || !args.is_empty() {
                    args.push(cur);
                }
                break;
            }
            Token::Rparen | Token::RightBrack | Token::RightBrace => {
                depth -= 1;
                cur.push(s.clone());
                i += 1;
            }
            Token::Comma if depth == 0 => {
                trim_trailing_newlines(&mut cur);
                args.push(std::mem::take(&mut cur));
                i += 1;
            }
            Token::Newline if depth == 0 && cur.is_empty() => {
                i += 1;
            }
            _ => {
                cur.push(s.clone());
                i += 1;
            }
        }
    }
    Ok((args, i))
}

fn trim_trailing_newlines(toks: &mut Vec<Spanned>) {
    while matches!(toks.last().map(|s| &s.tok), Some(Token::Newline)) {
        toks.pop();
    }
}

/// Drop leading and trailing `Newline` tokens (used for macro bodies, where the
/// newlines around a multi-line `{ … }` are formatting, not structure).
fn trim_edge_newlines(toks: &[Spanned]) -> Vec<Spanned> {
    let mut start = 0;
    let mut end = toks.len();
    while start < end && toks[start].tok == Token::Newline {
        start += 1;
    }
    while end > start && toks[end - 1].tok == Token::Newline {
        end -= 1;
    }
    toks[start..end].to_vec()
}

/// Replace `$k` argument tokens in a macro body with the k-th argument's tokens.
fn substitute(body: &[Spanned], args: &[Vec<Spanned>]) -> Vec<Spanned> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        if matches!(body[i].tok, Token::Kw(Kw::Define))
            && let Some((define, next)) = copy_define_verbatim(body, i)
        {
            out.extend(define);
            i = next;
            continue;
        }

        if let Some((pasted, next)) = paste_adjacent_args(body, args, i) {
            out.push(pasted.with_arg_frame(args));
            i = next;
            continue;
        }

        let s = &body[i];
        match &s.tok {
            Token::Arg(k) => {
                if let Some(a) = args.get((*k as usize).wrapping_sub(1)) {
                    out.extend(a.iter().cloned().map(|s| s.with_arg_frame(args)));
                }
            }
            // `$+` is the number of arguments passed to this macro
            Token::ArgCount => out.push(
                Spanned::new(Token::Float(args.len() as f64), s.line, s.col).with_arg_frame(args),
            ),
            // `$n` is also substituted inside string literals (the `"$1"==""`
            // default-argument idiom, sprintf templates like `"$2%g"`, …).
            Token::Str(text) if text.contains('$') => {
                out.push(
                    Spanned::new(Token::Str(subst_in_string(text, args)), s.line, s.col)
                        .with_arg_frame(args),
                );
            }
            _ => out.push(s.clone().with_arg_frame(args)),
        }
        i += 1;
    }
    out
}

fn copy_define_verbatim(body: &[Spanned], start: usize) -> Option<(Vec<Spanned>, usize)> {
    let mut name_idx = start + 1;
    while matches!(body.get(name_idx).map(|s| &s.tok), Some(Token::Newline)) {
        name_idx += 1;
    }
    if !matches!(
        body.get(name_idx).map(|s| &s.tok),
        Some(Token::Name(_)) | Some(Token::Label(_))
    ) {
        return None;
    }

    let mut out = Vec::new();
    let mut i = start;
    while let Some(s) = body.get(i) {
        out.push(s.clone());
        i += 1;
        if matches!(s.tok, Token::LeftBrace) {
            break;
        }
    }

    if !matches!(out.last().map(|s| &s.tok), Some(Token::LeftBrace)) {
        return None;
    }

    let mut depth = 1i32;
    while let Some(s) = body.get(i) {
        match &s.tok {
            Token::LeftBrace => depth += 1,
            Token::RightBrace => depth -= 1,
            _ => {}
        }
        out.push(s.clone());
        i += 1;
        if depth == 0 {
            return Some((out, i));
        }
    }
    None
}

fn paste_adjacent_args(
    body: &[Spanned],
    args: &[Vec<Spanned>],
    start: usize,
) -> Option<(Spanned, usize)> {
    let first = body.get(start)?;
    let Token::Arg(k) = &first.tok else {
        return None;
    };

    let mut text = arg_text(*k, args);
    let mut count = 1usize;
    let mut end = start + 1;
    let mut prev = first;
    while let Some(next) = body.get(end) {
        let Token::Arg(k) = &next.tok else {
            break;
        };
        if !adjacent_arg_tokens(prev, next) {
            break;
        }
        text.push_str(&arg_text(*k, args));
        count += 1;
        prev = next;
        end += 1;
    }

    if count < 2 || text.is_empty() {
        return None;
    }

    Some((tokenize_pasted_arg_text(&text, first.line, first.col), end))
}

fn arg_text(k: u32, args: &[Vec<Spanned>]) -> String {
    args.get((k as usize).wrapping_sub(1))
        .map(|a| tokens_to_text(a))
        .unwrap_or_default()
}

fn adjacent_arg_tokens(left: &Spanned, right: &Spanned) -> bool {
    left.line == right.line && arg_end_col(left) == Some(right.col)
}

fn arg_end_col(s: &Spanned) -> Option<u32> {
    let Token::Arg(k) = &s.tok else {
        return None;
    };
    Some(s.col + 1 + k.to_string().len() as u32)
}

fn tokenize_pasted_arg_text(text: &str, line: u32, col: u32) -> Spanned {
    if let Ok(toks) = lex(text)
        && toks.len() == 2
        && matches!(toks[1].tok, Token::Eof)
    {
        return Spanned::new(toks[0].tok.clone(), line, col);
    }

    let tok = if text.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        Token::Label(text.to_string())
    } else {
        Token::Name(text.to_string())
    };
    Spanned::new(tok, line, col)
}

/// Replace `$n` references inside a string literal with the textual form of the
/// n-th macro argument (empty if missing).
fn subst_in_string(text: &str, args: &[Vec<Spanned>]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && chars.get(i + 1) == Some(&'+') {
            out.push_str(&args.len().to_string());
            i += 2;
        } else if chars[i] == '$' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
            let mut j = i + 1;
            let mut num = String::new();
            while j < chars.len() && chars[j].is_ascii_digit() {
                num.push(chars[j]);
                j += 1;
            }
            if let Ok(k) = num.parse::<usize>()
                && k >= 1
                && let Some(a) = args.get(k - 1)
            {
                out.push_str(&tokens_to_text(a));
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Best-effort textual rendering of an argument token list, for `$n` splicing
/// inside string literals. dpic treats macro arguments as raw source text, so
/// the splice must reproduce it: a single string keeps splicing as bare
/// content (the `box "$1"` label idiom), but a multi-token argument — exec'd
/// statements like `box shaded "#00ff00"` — keeps its inner quotes (bare `#…`
/// would start a comment) and the source's own spacing, where it used to glue
/// everything into `boxshaded#00ff00` (#280). Spacing comes from the token
/// spans: a gap in the source becomes a space, and source-adjacent tokens
/// (`2L` = `2`+`L`) stay glued — re-gluing what the lexer split apart is safe
/// by construction. The word-boundary heuristic only decides for tokens of
/// mixed provenance (nested expansions), where spans aren't comparable.
fn tokens_to_text(toks: &[Spanned]) -> String {
    let material: Vec<&Spanned> = toks
        .iter()
        .filter(|s| !matches!(s.tok, Token::Eof))
        .collect();
    if let [one] = material.as_slice()
        && let Token::Str(t) = &one.tok
    {
        // a lone string argument splices as its content, quote-free — the
        // classic quote-at-use-site idiom (`box "$1"` with a quoted label)
        return t.clone();
    }
    let mut s = String::new();
    let mut prev: Option<(u32, u32)> = None; // previous token's source line, end_col
    for t in material {
        let piece = arg_token_text(&t.tok);
        if piece.is_empty() {
            continue;
        }
        if let Some((pline, pend)) = prev {
            // Spacing comes from the SOURCE spans, not the rendered lengths:
            // a token that renders shorter/longer than its source (a
            // normalized float `1.50`→`1.5`, an escaped string) must not skew
            // the gap test (#282 follow-up).
            let sep = if t.line != pline {
                true
            } else if t.col >= pend {
                t.col > pend
            } else {
                // spans not comparable (mixed provenance): separate only
                // where gluing could merge word-like tokens
                matches!((s.chars().last(), piece.chars().next()),
                    (Some(a), Some(b)) if wordish(a) && wordish(b))
            };
            if sep {
                s.push(' ');
            }
        }
        s.push_str(&piece);
        prev = Some((t.line, t.end_col));
    }
    s
}

/// Would these two characters merge into (or corrupt) a token if adjacent?
fn wordish(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '"' | '$')
}

/// The source text of one token, for argument splicing.
fn arg_token_text(tok: &Token) -> String {
    match tok {
        Token::Float(v) => format!("{v}"),
        // inner quotes escaped so the splice survives an exec round-trip
        // (`unescape_exec_source` folds them back before re-lexing)
        Token::Str(t) => format!("\"{}\"", t.replace('"', "\\\"")),
        Token::Name(n) | Token::Label(n) => n.clone(),
        Token::Arg(k) => format!("${k}"),
        Token::ArgCount => "$+".into(),
        Token::Dollar => "$".into(),
        Token::Backslash => "\\".into(),
        Token::Newline => ";".into(),
        Token::DotPS => ".PS".into(),
        Token::DotPE => ".PE".into(),
        Token::Eof => String::new(),
        Token::Lt => "<".into(),
        Token::Gt => ">".into(),
        Token::Le => "<=".into(),
        Token::Ge => ">=".into(),
        Token::EqEq => "==".into(),
        Token::Neq => "!=".into(),
        Token::Eq => "=".into(),
        Token::ColonEq => ":=".into(),
        Token::PlusEq => "+=".into(),
        Token::MinusEq => "-=".into(),
        Token::MultEq => "*=".into(),
        Token::DivEq => "/=".into(),
        Token::RemEq => "%=".into(),
        Token::Not => "!".into(),
        Token::AndAnd => "&&".into(),
        Token::OrOr => "||".into(),
        Token::Ampersand => "&".into(),
        Token::Caret => "^".into(),
        Token::Lparen => "(".into(),
        Token::Rparen => ")".into(),
        Token::LeftBrack => "[".into(),
        Token::RightBrack => "]".into(),
        Token::LeftBrace => "{".into(),
        Token::RightBrace => "}".into(),
        Token::Block => "[]".into(),
        Token::LeftQuote => "`".into(),
        Token::RightQuote => "'".into(),
        Token::Comma => ",".into(),
        Token::Colon => ":".into(),
        Token::Dot => ".".into(),
        Token::Plus => "+".into(),
        Token::Minus => "-".into(),
        Token::Mult => "*".into(),
        Token::Div => "/".into(),
        Token::Percent => "%".into(),
        Token::DotX => ".x".into(),
        Token::DotY => ".y".into(),
        Token::Corner(c) => corner_text(*c).into(),
        Token::Param(p) => param_text(*p).into(),
        Token::LineType(l) => line_type_text(*l).into(),
        Token::TextPos(p) => text_pos_text(*p).into(),
        Token::Arrow(a) => arrow_text(*a).into(),
        Token::Dir(d) => dir_text(*d).into(),
        Token::Prim(p) => prim_text(*p).into(),
        Token::Color(c) => color_text(*c).into(),
        Token::Kw(k) => kw_text(*k).into(),
        Token::Func1(f) => format!("{f:?}").to_ascii_lowercase(),
        Token::Func2(f) => format!("{f:?}").to_ascii_lowercase(),
        Token::EnvVar(e) => format!("{e:?}").to_ascii_lowercase(),
    }
}

fn corner_text(c: Corner) -> &'static str {
    match c {
        Corner::N => ".n",
        Corner::S => ".s",
        Corner::E => ".e",
        Corner::W => ".w",
        Corner::Ne => ".ne",
        Corner::Se => ".se",
        Corner::Nw => ".nw",
        Corner::Sw => ".sw",
        Corner::Start => ".start",
        Corner::End => ".end",
        Corner::Center => ".c",
    }
}

fn param_text(p: Param) -> &'static str {
    match p {
        Param::Height => ".ht",
        Param::Width => ".wid",
        Param::Radius => ".rad",
        Param::Diameter => ".diam",
        Param::Thickness => ".thick",
        Param::Length => ".len",
    }
}

fn line_type_text(l: LineType) -> &'static str {
    match l {
        LineType::Solid => "solid",
        LineType::Dotted => "dotted",
        LineType::Dashed => "dashed",
        LineType::Invis => "invis",
    }
}

fn text_pos_text(p: TextPos) -> &'static str {
    match p {
        TextPos::Center => "center",
        TextPos::Ljust => "ljust",
        TextPos::Rjust => "rjust",
        TextPos::Above => "above",
        TextPos::Below => "below",
    }
}

fn arrow_text(a: Arrow) -> &'static str {
    match a {
        Arrow::Left => "<-",
        Arrow::Right => "->",
        Arrow::Double => "<->",
    }
}

fn dir_text(d: Dir) -> &'static str {
    match d {
        Dir::Up => "up",
        Dir::Down => "down",
        Dir::Right => "right",
        Dir::Left => "left",
    }
}

fn prim_text(p: Prim) -> &'static str {
    match p {
        Prim::Box => "box",
        Prim::Circle => "circle",
        Prim::Ellipse => "ellipse",
        Prim::Arc => "arc",
        Prim::Line => "line",
        Prim::Arrow => "arrow",
        Prim::Move => "move",
        Prim::Spline => "spline",
    }
}

fn color_text(c: Color) -> &'static str {
    match c {
        Color::Colored => "color",
        Color::Outlined => "outlined",
        Color::Shaded => "shaded",
    }
}

fn kw_text(k: Kw) -> &'static str {
    match k {
        Kw::Ht => "ht",
        Kw::Wid => "wid",
        Kw::Rad => "rad",
        Kw::Diam => "diam",
        Kw::Thick => "thick",
        Kw::Thin => "thin",
        Kw::Scaled => "scaled",
        Kw::From => "from",
        Kw::To => "to",
        Kw::At => "at",
        Kw::With => "with",
        Kw::By => "by",
        Kw::Then => "then",
        Kw::Continue => "continue",
        Kw::Chop => "chop",
        Kw::Same => "same",
        Kw::Cw => "cw",
        Kw::Ccw => "ccw",
        Kw::Of => "of",
        Kw::The => "the",
        Kw::Way => "way",
        Kw::Between => "between",
        Kw::And => "and",
        Kw::Here => "Here",
        Kw::Last => "last",
        Kw::Fill => "fill",
        Kw::Nth => "ordinal suffix",
        Kw::Print => "print",
        Kw::Copy => "copy",
        Kw::Reset => "reset",
        Kw::Exec => "exec",
        Kw::Sh => "sh",
        Kw::Command => "command",
        Kw::Define => "define",
        Kw::Undef => "undef",
        Kw::Rand => "rand",
        Kw::If => "if",
        Kw::Else => "else",
        Kw::For => "for",
        Kw::Do => "do",
        Kw::Sprintf => "sprintf",
        Kw::Animate => "animate",
        Kw::After => "after",
        Kw::Delay => "delay",
        Kw::Repeat => "repeat",
        Kw::Yoyo => "yoyo",
        Kw::Ease => "ease",
        Kw::Along => "along",
        Kw::Stagger => "stagger",
        Kw::Out => "out",
        Kw::Scroll => "scroll",
        Kw::Into => "into",
    }
}

fn token_text(t: &Token) -> String {
    match t {
        Token::Float(v) => fmt_float(*v),
        Token::Str(s) => format!("\"{s}\""),
        Token::Name(s) | Token::Label(s) => format!("`{s}`"),
        Token::Arg(n) => format!("${n}"),
        Token::ArgCount => "$+".into(),
        Token::Dollar => "$".into(),
        Token::Backslash => "\\".into(),
        Token::Newline => "end of line".into(),
        Token::DotPS => ".PS".into(),
        Token::DotPE => ".PE".into(),
        Token::Eof => "end of input".into(),
        Token::Lt => "<".into(),
        Token::Lparen => "(".into(),
        Token::Rparen => ")".into(),
        Token::Mult => "*".into(),
        Token::Plus => "+".into(),
        Token::Minus => "-".into(),
        Token::Div => "/".into(),
        Token::Percent => "%".into(),
        Token::Caret => "^".into(),
        Token::Not => "!".into(),
        Token::AndAnd => "&&".into(),
        Token::OrOr => "||".into(),
        Token::Ampersand => "&".into(),
        Token::Comma => ",".into(),
        Token::Colon => ":".into(),
        Token::LeftBrack => "[".into(),
        Token::RightBrack => "]".into(),
        Token::LeftBrace => "{".into(),
        Token::RightBrace => "}".into(),
        Token::Dot => ".".into(),
        Token::Block => "[]".into(),
        Token::LeftQuote => "`".into(),
        Token::RightQuote => "'".into(),
        Token::Eq => "=".into(),
        Token::ColonEq => ":=".into(),
        Token::PlusEq => "+=".into(),
        Token::MinusEq => "-=".into(),
        Token::MultEq => "*=".into(),
        Token::DivEq => "/=".into(),
        Token::RemEq => "%=".into(),
        Token::EqEq => "==".into(),
        Token::Neq => "!=".into(),
        Token::Ge => ">=".into(),
        Token::Le => "<=".into(),
        Token::Gt => ">".into(),
        Token::DotX => ".x".into(),
        Token::DotY => ".y".into(),
        Token::Kw(k) => kw_text(*k).into(),
        Token::Corner(c) => corner_text(*c).into(),
        Token::Param(p) => format!(".{}", param_text(*p)),
        Token::Func1(f) => format!("{f:?}").to_ascii_lowercase(),
        Token::Func2(f) => format!("{f:?}").to_ascii_lowercase(),
        Token::LineType(l) => line_type_text(*l).into(),
        Token::TextPos(p) => text_pos_text(*p).into(),
        Token::Arrow(a) => arrow_text(*a).into(),
        Token::Dir(d) => dir_text(*d).into(),
        Token::Prim(p) => prim_text(*p).into(),
        Token::Color(c) => color_text(*c).into(),
        Token::EnvVar(e) => format!("{e:?}").to_ascii_lowercase(),
    }
}

fn fmt_float(v: f64) -> String {
    let mut s = v.to_string();
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    s
}

fn suggest_object(word: &str) -> Option<&'static str> {
    const WORDS: &[&str] = &[
        "arc", "arrow", "box", "brace", "circle", "dot", "ellipse", "line", "move", "spline",
    ];
    crate::diagnostic::closest(word, WORDS)
}

/// Read and tokenize a `copy "file"` include, returning its expanded tokens
/// (with the trailing `Eof` removed so it splices cleanly mid-stream). The
/// included file resolves nested `copy`s relative to its own directory.
fn include_file(
    includes: &IncludeCtx,
    fname: &str,
    macros: &mut HashMap<String, Vec<Spanned>>,
    depth: usize,
    span: Span,
) -> Result<Vec<Spanned>, ParseError> {
    let mkerr = |msg: String| ParseError::new(msg, span.clone());
    let denied = |why: &str| {
        ParseError::new(format!("copy \"{fname}\": {why}"), span.clone())
            .with_kind("include_denied")
    };
    // `copy "circuits"` is a reserved target: it loads the embedded native
    // circuit-element library — the in-source spelling of `-c`, usable even
    // where file includes are not (wasm, compile_json with no base dir). It
    // shadows any real file literally named `circuits`. Skipped when the
    // library is already loaded (`-c` plus an explicit copy): `__resistor`
    // is one of its own defines.
    if fname == "circuits" {
        if macros.contains_key("__resistor") {
            return Ok(Vec::new());
        }
        let toks = lex_named(crate::CIRCUITS, "circuits")?;
        let mut expanded = expand(&toks, macros, depth + 1, includes)?;
        if matches!(expanded.last().map(|s| &s.tok), Some(Token::Eof)) {
            expanded.pop();
        }
        return Ok(expanded);
    }
    if includes.policy == IncludePolicy::Deny {
        return Err(denied(
            "filesystem includes are disabled by the include policy",
        ));
    }
    let p = Path::new(fname);
    if p.is_absolute() && includes.policy == IncludePolicy::SandboxedToBase {
        return Err(denied(
            "absolute paths are not allowed by the include policy",
        ));
    }
    let path = if p.is_absolute() {
        p.to_path_buf()
    } else {
        match includes.dir.as_deref() {
            Some(b) => b.join(p),
            None => {
                return Err(mkerr(format!(
                    "copy \"{fname}\": file includes require a file path (unavailable here)"
                )));
            }
        }
    };
    if includes.policy == IncludePolicy::SandboxedToBase {
        // Canonicalize (resolving `..` and symlinks) and require the result
        // to stay inside the fence root. A fence that failed to resolve at
        // setup fails closed. The error names the path as written, never the
        // resolved location.
        let inside = includes
            .fence
            .as_deref()
            .is_some_and(|fence| std::fs::canonicalize(&path).is_ok_and(|c| c.starts_with(fence)));
        if !inside {
            return Err(denied("path resolves outside the include base directory"));
        }
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| mkerr(format!("copy \"{fname}\": {e}")))?;
    let toks = lex_named(&content, fname)?;
    let inc_base = path.parent().map(|d| d.to_path_buf());
    let inc_ctx = includes.child(inc_base);
    let mut expanded = expand(&toks, macros, depth + 1, &inc_ctx)?;
    if matches!(expanded.last().map(|s| &s.tok), Some(Token::Eof)) {
        expanded.pop();
    }
    Ok(expanded)
}

/// Find the next occurrence of keyword `kw` at bracket-depth 0 from `start`.
fn find_kw_depth0(toks: &[Spanned], start: usize, kw: Kw) -> Option<usize> {
    let mut depth = 0i32;
    for (off, s) in toks[start..].iter().enumerate() {
        match &s.tok {
            Token::Lparen | Token::LeftBrace | Token::LeftBrack => depth += 1,
            Token::Rparen | Token::RightBrace | Token::RightBrack => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            Token::Kw(k) if *k == kw && depth == 0 => return Some(start + off),
            _ => {}
        }
    }
    None
}

/// Copy a brace-delimited block verbatim into `out` (including any nested
/// braces), skipping/copying leading newlines. Returns the index past the `}`.
fn copy_braced(
    toks: &[Spanned],
    mut i: usize,
    out: &mut Vec<Spanned>,
    macros: &HashMap<String, Vec<Spanned>>,
) -> Result<usize, ParseError> {
    while matches!(toks.get(i).map(|s| &s.tok), Some(Token::Newline)) {
        out.push(toks[i].clone());
        i += 1;
    }
    if !matches!(toks.get(i).map(|s| &s.tok), Some(Token::LeftBrace)) {
        return Ok(i); // no body; the parser will report the problem
    }
    let mut depth = 0i32;
    let mut tagged_body = false;
    while let Some(s) = toks.get(i) {
        let mut s = s.clone();
        let outer_left_brace = depth == 0 && matches!(s.tok, Token::LeftBrace);
        match &s.tok {
            Token::LeftBrace => depth += 1,
            Token::RightBrace => {
                out.push(s);
                i += 1;
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
                continue;
            }
            _ => {}
        }
        if depth == 1 && !outer_left_brace && !tagged_body {
            s = s.with_macro_frame(macros);
            tagged_body = true;
        }
        out.push(s);
        i += 1;
    }
    Err(ParseError::new("unterminated `{` body", Span::new(0, 0, 0)))
}

fn read_braced_body(
    toks: &[Spanned],
    mut i: usize,
) -> Result<Option<(Vec<Spanned>, usize)>, ParseError> {
    while matches!(toks.get(i).map(|s| &s.tok), Some(Token::Newline)) {
        i += 1;
    }
    if !matches!(toks.get(i).map(|s| &s.tok), Some(Token::LeftBrace)) {
        return Ok(None);
    }
    i += 1;
    let start = i;
    let mut depth = 1i32;
    while let Some(s) = toks.get(i) {
        match &s.tok {
            Token::LeftBrace => depth += 1,
            Token::RightBrace => {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some((toks[start..i].to_vec(), i + 1)));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(ParseError::new("unterminated `{` body", Span::new(0, 0, 0)))
}

fn static_truth(toks: &[Spanned]) -> Option<bool> {
    let toks = trim_trailing_eof(toks);
    match toks {
        [
            Spanned {
                tok: Token::Float(v),
                ..
            },
        ] => Some(*v != 0.0),
        [
            Spanned {
                tok: Token::Not, ..
            },
            Spanned {
                tok: Token::Float(v),
                ..
            },
        ] => Some(*v == 0.0),
        [a, op, b] => match op.tok {
            Token::EqEq | Token::Neq => {
                if let (Some(lhs), Some(rhs)) = (static_string(a), static_string(b)) {
                    return Some(if matches!(op.tok, Token::EqEq) {
                        lhs == rhs
                    } else {
                        lhs != rhs
                    });
                }
                if let (Some(lhs), Some(rhs)) = (static_number(a), static_number(b)) {
                    return Some(if matches!(op.tok, Token::EqEq) {
                        (lhs - rhs).abs() < f64::EPSILON
                    } else {
                        (lhs - rhs).abs() >= f64::EPSILON
                    });
                }
                None
            }
            _ => None,
        },
        [a, op, b, op2, c] if matches!(op2.tok, Token::Plus) => {
            let lhs = static_string(a)?;
            let mut rhs = static_string(b)?;
            rhs.push_str(&static_string(c)?);
            match op.tok {
                Token::EqEq => Some(lhs == rhs),
                Token::Neq => Some(lhs != rhs),
                _ => None,
            }
        }
        _ => None,
    }
}

fn trim_trailing_eof(toks: &[Spanned]) -> &[Spanned] {
    if matches!(toks.last().map(|s| &s.tok), Some(Token::Eof)) {
        &toks[..toks.len() - 1]
    } else {
        toks
    }
}

fn static_string(s: &Spanned) -> Option<String> {
    match &s.tok {
        Token::Str(v) => Some(v.clone()),
        _ => None,
    }
}

fn static_number(s: &Spanned) -> Option<f64> {
    match &s.tok {
        Token::Float(v) => Some(*v),
        Token::Name(n) | Token::Label(n) => dpic_backend_constant(n),
        _ => None,
    }
}

fn dpic_backend_constant(name: &str) -> Option<f64> {
    // ONE-based, oracle-checked against dpic (`dpic -v` prints optMFpic=1 …
    // optSVG=9 … optxfig=12); keep in sync with `install_dpic_compat_vars`.
    match name {
        "optMFpic" => Some(1.0),
        "optMpost" => Some(2.0),
        "optPDF" => Some(3.0),
        "optPGF" => Some(4.0),
        "optPict2e" => Some(5.0),
        "optPS" => Some(6.0),
        "optPSfrag" => Some(7.0),
        "optPSTricks" => Some(8.0),
        "optSVG" | "dpicopt" => Some(9.0),
        "optTeX" => Some(10.0),
        "opttTeX" => Some(11.0),
        "optxfig" => Some(12.0),
        _ => None,
    }
}

type PResult<T> = Result<T, ParseError>;

fn is_assign_op(t: &Token) -> bool {
    matches!(
        t,
        Token::Eq
            | Token::ColonEq
            | Token::PlusEq
            | Token::MinusEq
            | Token::MultEq
            | Token::DivEq
            | Token::RemEq
    )
}

struct Parser {
    toks: Vec<Spanned>,
    idx: usize,
    depth: u32,
}

/// Recursive-descent nesting limit. The macro expander already caps its own
/// depth (`expand`, `depth > 64`), but parentheses and `[…]` blocks pass
/// through it untouched and only recurse in the descent parser — so without
/// this guard, pathological input like `((((…))))` overflows the stack and
/// aborts the process instead of returning a normal error (#283). Chosen for a
/// comfortable margin on wasm's ~1 MB stack (each level costs ~1.7 KB of native
/// stack across the precedence-climbing chain, more on wasm) while dwarfing any
/// real figure — the entire committed corpus nests at most 4 deep.
const MAX_PARSE_DEPTH: u32 = 128;
/// Flat operator chains build left-deep ASTs without going through
/// `descend`, so cap them explicitly as well (#306).
const MAX_EXPR_CHAIN_OPS: u32 = MAX_PARSE_DEPTH;

impl Parser {
    fn new(toks: Vec<Spanned>) -> Self {
        Parser {
            toks,
            idx: 0,
            depth: 0,
        }
    }

    /// Run `f` one recursion level deeper, failing cleanly past
    /// `MAX_PARSE_DEPTH` instead of overflowing the stack. Wrap the mutually
    /// recursive entry points (expressions, positions, objects/blocks).
    fn descend<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return self.err("expression or block nested too deeply".to_string());
        }
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn check_expr_chain(&self, ops: u32) -> PResult<()> {
        if ops > MAX_EXPR_CHAIN_OPS {
            return self.err(format!(
                "expression has too many chained operators (maximum {MAX_EXPR_CHAIN_OPS})"
            ));
        }
        Ok(())
    }

    // ---- cursor helpers ----------------------------------------------------

    fn cur(&self) -> &Token {
        &self.toks[self.idx].tok
    }
    fn cur_span(&self) -> Span {
        self.toks[self.idx].span()
    }
    fn peek(&self, n: usize) -> &Token {
        self.toks
            .get(self.idx + n)
            .map(|s| &s.tok)
            .unwrap_or(&Token::Eof)
    }
    fn at(&self, t: &Token) -> bool {
        self.cur() == t
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.idx].tok.clone();
        if self.idx + 1 < self.toks.len() {
            self.idx += 1;
        }
        t
    }
    fn eat(&mut self, t: &Token) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Token) -> PResult<()> {
        if self.eat(t) {
            Ok(())
        } else {
            self.expected_here(token_text(t))
        }
    }
    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        let s = &self.toks[self.idx];
        Err(ParseError::new(msg, s.span()))
    }
    fn expected_here<T>(&self, expected: impl Into<String>) -> PResult<T> {
        let s = &self.toks[self.idx];
        Err(ParseError::expected(expected, token_text(&s.tok), s.span()))
    }
    fn expected_object<T>(&self) -> PResult<T> {
        let s = &self.toks[self.idx];
        let mut e = ParseError::expected("an object", token_text(&s.tok), s.span());
        if let Token::Name(name) | Token::Label(name) = &s.tok
            && let Some(hint) = suggest_object(name)
        {
            e.detail.hint = Some(format!("did you mean `{hint}`?"));
        }
        Err(e)
    }
    fn at_kw(&self, k: Kw) -> bool {
        matches!(self.cur(), Token::Kw(x) if *x == k)
    }
    fn eat_kw(&mut self, k: Kw) -> bool {
        if self.at_kw(k) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn skip_newlines(&mut self) {
        while self.at(&Token::Newline) {
            self.bump();
        }
    }

    // ---- top level ---------------------------------------------------------

    fn parse_picture(&mut self) -> PResult<Picture> {
        // `.PS`/`.PE` are treated as markers that may appear anywhere; statements
        // (including `animate`) are collected across them up to EOF. The first
        // `.PS` may carry optional width/height.
        let (mut width, mut height) = (None, None);
        let mut seen_ps = false;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            match self.cur() {
                Token::Eof => break,
                Token::DotPS => {
                    self.bump();
                    if !seen_ps && self.starts_scalar() {
                        width = Some(self.parse_expr()?);
                        if self.starts_scalar() {
                            height = Some(self.parse_expr()?);
                        }
                    }
                    seen_ps = true;
                    while !self.at(&Token::Newline) && !self.at(&Token::Eof) {
                        self.bump();
                    }
                    continue;
                }
                Token::DotPE => {
                    self.bump();
                    continue;
                }
                _ => {}
            }
            stmts.push(self.parse_element()?);
            if !self.at(&Token::Newline)
                && !self.at(&Token::Eof)
                && !self.at(&Token::DotPE)
                && !self.at(&Token::DotPS)
            {
                return self.err(format!("unexpected {:?} after statement", self.cur()));
            }
        }
        Ok(Picture {
            width,
            height,
            stmts,
            macros: HashMap::new(),
            includes: IncludeCtx::default(),
        })
    }

    /// Parse elements until one of `terminators` (or EOF) is the current token.
    fn parse_elementlist(&mut self, terminators: &[Token]) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Token::Eof) || terminators.iter().any(|t| self.at(t)) {
                break;
            }
            let s = self.parse_element()?;
            stmts.push(s);
            // a statement must end at a newline, a terminator, or EOF
            if !self.at(&Token::Newline)
                && !self.at(&Token::Eof)
                && !terminators.iter().any(|t| self.at(t))
            {
                return self.err(format!("unexpected {:?} after statement", self.cur()));
            }
        }
        Ok(stmts)
    }

    // ---- statements --------------------------------------------------------

    fn parse_element(&mut self) -> PResult<Stmt> {
        // A `%`-led line is a comment convention in some source documents (pic
        // proper uses `#`). `%` is never valid at statement start, so skip the
        // line as a no-op.
        if self.at(&Token::Percent) {
            while !self.at(&Token::Newline) && !self.at(&Token::Eof) {
                self.bump();
            }
            return Ok(Stmt::Print(PrintItem::Str(StringExpr::Lit(String::new()))));
        }

        // rpic animation directive.
        if self.at_kw(Kw::Animate) {
            // `animate scroll` is a timeline-level directive, not an object
            // animation — dispatch it before the `animate <place> …` form.
            if matches!(self.peek(1), Token::Kw(Kw::Scroll)) {
                self.bump();
                self.bump();
                return Ok(Stmt::AnimateScroll);
            }
            return Ok(Stmt::Animate(Box::new(self.parse_animate()?)));
        }

        // rpic `class <place> "name"` statement (extension). Contextual:
        // `class = 2` stays an assignment and `class` remains usable as a
        // variable, mirroring how `animate` targets are referenced.
        if matches!(self.cur(), Token::Name(n) if n == "class")
            && !is_assign_op(self.peek(1))
            && !matches!(self.peek(1), Token::LeftBrack)
        {
            self.bump();
            let target = self.parse_place()?;
            let class = self.parse_stringexpr()?;
            return Ok(Stmt::Class { target, class });
        }

        // rpic `draggable <place> [inertia] [bounds <place>] [x|y]` statement
        // (extension). Contextual like `class`: `draggable = 1` stays an
        // assignment and `draggable` remains usable as a variable.
        if matches!(self.cur(), Token::Name(n) if n == "draggable")
            && !is_assign_op(self.peek(1))
            && !matches!(self.peek(1), Token::LeftBrack)
        {
            return Ok(Stmt::Draggable(self.parse_draggable()?));
        }

        // rpic `canvas from <pos> to <pos>` statement (extension). Contextual
        // and stricter than `class`: only the exact `canvas from …` spelling
        // triggers, so `canvas = 2`, a `canvas(…)` macro and a plain variable
        // named canvas all keep their classic meaning.
        if matches!(self.cur(), Token::Name(n) if n == "canvas")
            && matches!(self.peek(1), Token::Kw(Kw::From))
        {
            self.bump();
            self.bump();
            let from = self.parse_position()?;
            self.expect(&Token::Kw(Kw::To))?;
            let to = self.parse_position()?;
            return Ok(Stmt::Canvas { from, to });
        }

        // control constructs
        match self.cur() {
            Token::Kw(Kw::If) => return self.parse_if(),
            Token::Kw(Kw::For) => return self.parse_for(),
            Token::Kw(Kw::Print) => return self.parse_print(),
            Token::Kw(Kw::Exec) => return self.parse_exec(),
            Token::Kw(Kw::Reset) => return self.parse_reset(),
            _ => {}
        }

        // `define`/`undef` are handled by the macro preprocessor before parsing;
        // reaching here means a non-brace form we don't support.
        if let Token::Kw(k) = self.cur() {
            match k {
                Kw::Define | Kw::Undef => {
                    return self.err("only the `define name { body }` macro form is supported");
                }
                // Policy (docs/raw-backend-policy in dpic-compat-audit.md):
                // `command` raw backend text is never injected into the SVG
                // output, and `sh` is never executed — both are tolerated as
                // true no-ops so dpic sources keep compiling. The lexer already
                // skipped their raw argument text.
                Kw::Command | Kw::Sh => {
                    self.bump();
                    return Ok(Stmt::Group(Vec::new()));
                }
                Kw::Copy => {
                    return self.err("`copy` is not supported yet (planned milestone)");
                }
                _ => {}
            }
        }

        // `{ … }` grouping. Nesting recurses parse_element → parse_elementlist
        // → parse_element, so bound it through `descend` like the other block
        // forms (a deeply nested `{{{…}}}` would otherwise overflow the stack).
        if self.eat(&Token::LeftBrace) {
            let stmts = self.descend(|p| p.parse_elementlist(&[Token::RightBrace]))?;
            self.expect(&Token::RightBrace)?;
            return Ok(Stmt::Group(stmts));
        }

        // Labelled element: `Label [suffix] : (object | position)`
        if matches!(self.cur(), Token::Label(_)) && self.label_colon_ahead() {
            let label = self.parse_label()?;
            self.expect(&Token::Colon)?;
            if self.at_object_start() {
                let object = self.parse_object()?;
                return Ok(Stmt::Object {
                    label: Some(label),
                    object,
                });
            } else {
                let pos = self.parse_label_position()?;
                return Ok(Stmt::Place { label, pos });
            }
        }

        // Assignment: `name [suffix] op …` or `envvar op …`
        if self.at_assignment_start() {
            return Ok(Stmt::Assign(self.parse_assignlist()?));
        }

        // Bare direction change.
        if let Token::Dir(d) = self.cur() {
            let d = *d;
            // Only a standalone direction (next token ends the statement).
            if matches!(self.peek(1), Token::Newline | Token::Eof) {
                self.bump();
                return Ok(Stmt::Direction(d));
            }
        }

        // Otherwise: an unlabelled object.
        let object = self.parse_object()?;
        Ok(Stmt::Object {
            label: None,
            object,
        })
    }

    fn parse_if(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::If)?;
        let cond = self.parse_expr()?;
        self.expect_kw(Kw::Then)?;
        let then_body = self.capture_braced()?;
        // optional `else { … }`, possibly across newlines (which otherwise end
        // the statement)
        let save = self.idx;
        self.skip_newlines();
        let else_body = if self.eat_kw(Kw::Else) {
            Some(self.capture_braced()?)
        } else {
            self.idx = save;
            None
        };
        Ok(Stmt::If {
            cond,
            then_body,
            else_body,
        })
    }

    fn parse_for(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::For)?;
        let var = match self.bump() {
            Token::Name(s) | Token::Label(s) => s,
            other => return self.err(format!("expected loop variable, found {other:?}")),
        };
        let subscript = if self.eat(&Token::LeftBrack) {
            let e = self.parse_subscript()?;
            self.expect(&Token::RightBrack)?;
            Some(e)
        } else {
            None
        };
        match self.bump() {
            Token::Eq | Token::ColonEq => {}
            other => return self.err(format!("expected `=` in for, found {other:?}")),
        }
        let from = self.parse_expr()?;
        self.expect_kw(Kw::To)?;
        let to = self.parse_expr()?;
        let mut by = Expr::Num(1.0);
        let mut mult = false;
        if self.eat_kw(Kw::By) {
            mult = self.eat(&Token::Mult);
            by = self.parse_expr()?;
        }
        self.expect_kw(Kw::Do)?;
        let body = self.capture_braced()?;
        Ok(Stmt::For {
            var,
            subscript,
            from,
            to,
            by,
            mult,
            body,
        })
    }

    /// Capture a brace-delimited block as raw tokens (excluding the braces),
    /// for deferred parsing by the evaluator. Assumes the body follows.
    fn capture_braced(&mut self) -> PResult<Body> {
        self.skip_newlines();
        self.expect(&Token::LeftBrace)?;
        let start = self.idx;
        let mut depth = 1i32;
        loop {
            match self.cur() {
                Token::LeftBrace => depth += 1,
                Token::RightBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Token::Eof => return self.err("unterminated `{` body"),
                _ => {}
            }
            self.bump();
        }
        let body = self.toks[start..self.idx].to_vec();
        self.expect(&Token::RightBrace)?;
        Ok(body)
    }

    fn parse_print(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::Print)?;
        let item = if self.at_string_start() {
            PrintItem::Str(self.parse_stringexpr()?)
        } else {
            PrintItem::Expr(self.parse_expr()?)
        };
        Ok(Stmt::Print(item))
    }

    fn parse_exec(&mut self) -> PResult<Stmt> {
        let arg_frame = self.toks[self.idx].arg_frame.clone();
        self.expect_kw(Kw::Exec)?;
        let command = self.parse_stringexpr()?;
        Ok(Stmt::Exec { command, arg_frame })
    }

    fn parse_reset(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::Reset)?;
        let mut list = Vec::new();
        if let Token::EnvVar(v) = self.cur() {
            list.push(*v);
            self.bump();
            while self.eat(&Token::Comma) {
                match self.cur() {
                    Token::EnvVar(v) => {
                        list.push(*v);
                        self.bump();
                    }
                    other => {
                        return self.err(format!("expected environment variable, found {other:?}"));
                    }
                }
            }
        }
        Ok(Stmt::Reset(list))
    }

    fn at_string_start(&self) -> bool {
        self.token_starts_string_at(0)
    }

    fn parse_animate(&mut self) -> PResult<Animate> {
        self.expect_kw(Kw::Animate)?;
        let target = self.parse_place()?;
        self.expect_kw(Kw::With)?;
        let effect_span = Some(self.cur_span());
        let effect = self.parse_stringexpr()?;
        // `to`/`from` are overloaded: `to <colour>` for `highlight`, `from <dir>`
        // for `slide`, but a *reveal fraction* for `draw` (`draw from 40% to 60%`).
        // A literal `"draw"` effect switches those two clauses over; a dynamic
        // effect string keeps the colour/direction reading.
        let is_draw = matches!(&effect, StringExpr::Lit(s) if s == "draw");
        let mut duration = None;
        let mut timing = Timing::Sequential;
        let mut delay = None;
        let mut repeat = None;
        let mut yoyo = false;
        let mut ease = None;
        let mut along = None;
        let mut color = None;
        let mut stagger = None;
        let mut out = false;
        let mut slide_from = None;
        let mut morph_into = None;
        let mut type_unit = None;
        let mut scramble_chars = None;
        let mut wiggles = None;
        let mut draw_from = None;
        let mut draw_to = None;
        loop {
            if self.eat_kw(Kw::For) {
                duration = Some(self.parse_expr()?);
            } else if self.eat_kw(Kw::At) {
                timing = Timing::At(self.parse_expr()?);
            } else if self.eat_kw(Kw::After) {
                timing = Timing::After(self.parse_place()?);
            } else if self.eat_kw(Kw::Delay) {
                delay = Some(self.parse_expr()?);
            } else if self.eat_kw(Kw::Repeat) {
                repeat = Some(self.parse_expr()?);
            } else if self.eat_kw(Kw::Yoyo) {
                yoyo = true;
            } else if self.eat_kw(Kw::Ease) {
                ease = Some(self.parse_stringexpr()?);
            } else if self.eat_kw(Kw::Along) {
                along = Some(self.parse_place()?);
            } else if self.eat_kw(Kw::To) {
                if is_draw {
                    draw_to = Some(self.parse_draw_amount()?);
                } else {
                    color = Some(self.parse_color_like()?);
                }
            } else if self.eat_kw(Kw::Stagger) {
                stagger = Some(self.parse_expr()?);
            } else if self.eat_kw(Kw::Out) {
                out = true;
            } else if self.eat_kw(Kw::From) {
                if is_draw {
                    draw_from = Some(self.parse_draw_amount()?);
                } else {
                    slide_from = Some(self.parse_dir()?);
                }
            } else if self.eat_kw(Kw::Into) {
                morph_into = Some(self.parse_place()?);
            } else if self.eat_kw(Kw::By) {
                // `by "…"` is the scramble charset; `by word`/`by char` is the
                // type unit — a quoted string vs. a bareword disambiguates.
                if self.at_string_start() {
                    scramble_chars = Some(self.parse_stringexpr()?);
                } else {
                    type_unit = Some(self.parse_type_unit()?);
                }
            } else if matches!(self.cur(), Token::Name(n) if n == "wiggles") {
                // contextual keyword (like `class`/`animate`): stays a usable
                // variable name outside an animate clause
                self.bump();
                wiggles = Some(self.parse_expr()?);
            } else {
                break;
            }
        }
        Ok(Animate {
            target,
            effect,
            effect_span,
            duration,
            timing,
            delay,
            repeat,
            yoyo,
            ease,
            along,
            color,
            stagger,
            out,
            slide_from,
            morph_into,
            type_unit,
            scramble_chars,
            wiggles,
            draw_from,
            draw_to,
        })
    }

    /// A `draw` reveal fraction: a multiplicative term (no bare `%`, which is
    /// reserved as the percent suffix here) with an optional trailing `%` that
    /// divides by 100 — so `60%` and `0.6` both mean 0.6 of the stroke.
    fn parse_draw_amount(&mut self) -> PResult<Expr> {
        let mut e = self.parse_unary()?;
        loop {
            let op = match self.cur() {
                Token::Mult => BinOp::Mul,
                Token::Div => BinOp::Div,
                _ => break,
            };
            self.bump();
            let r = self.parse_unary()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        if matches!(self.cur(), Token::Percent) {
            self.bump();
            e = Expr::Bin(BinOp::Div, Box::new(e), Box::new(Expr::Num(100.0)));
        }
        Ok(e)
    }

    /// Consume the `type` effect's split unit after `by`: `word` or `char`.
    fn parse_type_unit(&mut self) -> PResult<TypeUnit> {
        match self.cur() {
            Token::Name(n) if n == "word" => {
                self.bump();
                Ok(TypeUnit::Word)
            }
            Token::Name(n) if n == "char" => {
                self.bump();
                Ok(TypeUnit::Char)
            }
            _ => self.err("expected `word` or `char` after `by`"),
        }
    }

    /// `draggable <place> [inertia] [bounds <place>] [x|y]`.
    fn parse_draggable(&mut self) -> PResult<Draggable> {
        self.bump(); // `draggable`
        let target = self.parse_place()?;
        let mut inertia = false;
        let mut bounds = None;
        let mut axis = None;
        loop {
            match self.cur() {
                Token::Name(n) if n == "inertia" => {
                    self.bump();
                    inertia = true;
                }
                Token::Name(n) if n == "bounds" => {
                    self.bump();
                    bounds = Some(self.parse_place()?);
                }
                Token::Name(n) if n == "x" => {
                    self.bump();
                    axis = Some(DragAxis::X);
                }
                Token::Name(n) if n == "y" => {
                    self.bump();
                    axis = Some(DragAxis::Y);
                }
                _ => break,
            }
        }
        Ok(Draggable {
            target,
            inertia,
            bounds,
            axis,
        })
    }

    /// Consume a bare compass direction token (`up`/`down`/`left`/`right`).
    fn parse_dir(&mut self) -> PResult<Dir> {
        if let Token::Dir(d) = self.cur() {
            let d = *d;
            self.bump();
            Ok(d)
        } else {
            self.expected_here("a direction (up, down, left, or right)")
        }
    }

    /// True if the current `Label` is followed by `:` (allowing a `[suffix]`).
    fn label_colon_ahead(&self) -> bool {
        match self.peek(1) {
            Token::Colon => true,
            Token::LeftBrack => {
                // scan past a balanced [ … ] suffix to find ':'
                let mut depth = 0;
                let mut i = self.idx + 1;
                while i < self.toks.len() {
                    match &self.toks[i].tok {
                        Token::LeftBrack => depth += 1,
                        Token::RightBrack => {
                            depth -= 1;
                            if depth == 0 {
                                return matches!(
                                    self.toks.get(i + 1).map(|s| &s.tok),
                                    Some(Token::Colon)
                                );
                            }
                        }
                        Token::Newline | Token::Eof => return false,
                        _ => {}
                    }
                    i += 1;
                }
                false
            }
            _ => false,
        }
    }

    fn at_object_start(&self) -> bool {
        matches!(
            self.cur(),
            Token::Prim(_)
                | Token::LeftBrack
                | Token::Block
                | Token::Str(_)
                | Token::Arg(_)
                | Token::Kw(Kw::Sprintf)
        ) || matches!(self.cur(), Token::Name(n) if n == "brace" || n == "dot")
    }

    fn at_assignment_start(&self) -> bool {
        match self.cur() {
            Token::Name(_) | Token::Label(_) => self.assignment_op_after_var_ref(),
            Token::EnvVar(_) => is_assign_op(self.peek(1)),
            _ => false,
        }
    }

    fn assignment_op_after_var_ref(&self) -> bool {
        let mut i = self.idx + 1;
        if matches!(self.toks.get(i).map(|s| &s.tok), Some(Token::LeftBrack)) {
            i += 1;
            let mut depth = 1i32;
            while let Some(tok) = self.toks.get(i).map(|s| &s.tok) {
                match tok {
                    Token::LeftBrack => depth += 1,
                    Token::RightBrack => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    Token::Eof | Token::Newline if depth > 0 => return false,
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                return false;
            }
        }
        self.toks.get(i).is_some_and(|s| is_assign_op(&s.tok))
    }

    fn parse_label(&mut self) -> PResult<Label> {
        let name = match self.bump() {
            Token::Label(s) => s,
            other => return self.err(format!("expected label, found {other:?}")),
        };
        let subscript = if self.eat(&Token::LeftBrack) {
            let e = self.parse_subscript()?;
            self.expect(&Token::RightBrack)?;
            Some(e)
        } else {
            None
        };
        Ok(Label { name, subscript })
    }

    fn parse_assignlist(&mut self) -> PResult<Vec<Assignment>> {
        let mut list = vec![self.parse_assignment()?];
        while self.eat(&Token::Comma) {
            list.push(self.parse_assignment()?);
        }
        Ok(list)
    }

    fn parse_assignment(&mut self) -> PResult<Assignment> {
        let target = match self.cur().clone() {
            Token::Name(name) | Token::Label(name) => {
                self.bump();
                let sub = if self.eat(&Token::LeftBrack) {
                    let e = self.parse_subscript()?;
                    self.expect(&Token::RightBrack)?;
                    Some(e)
                } else {
                    None
                };
                AssignTarget::Var(name, sub)
            }
            Token::EnvVar(v) => {
                self.bump();
                AssignTarget::Env(v)
            }
            other => return self.err(format!("expected assignment target, found {other:?}")),
        };
        let op = match self.bump() {
            Token::Eq => AssignOp::Set,
            Token::ColonEq => AssignOp::ColonSet,
            Token::PlusEq => AssignOp::Add,
            Token::MinusEq => AssignOp::Sub,
            Token::MultEq => AssignOp::Mul,
            Token::DivEq => AssignOp::Div,
            Token::RemEq => AssignOp::Rem,
            other => return self.err(format!("expected assignment operator, found {other:?}")),
        };
        let value = self.parse_expr()?;
        Ok(Assignment { target, op, value })
    }

    fn parse_subscript(&mut self) -> PResult<Expr> {
        let mut items = vec![self.parse_expr()?];
        while self.eat(&Token::Comma) {
            items.push(self.parse_expr()?);
        }
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(Expr::Index(items))
        }
    }

    // ---- objects & attributes ---------------------------------------------

    fn parse_object(&mut self) -> PResult<Object> {
        self.descend(Self::parse_object_inner)
    }

    fn parse_object_inner(&mut self) -> PResult<Object> {
        let span = Some(self.cur_span());
        let mut attrs = Vec::new();
        // a bare string expression (literal, `$arg`, sprintf, concatenation)
        // places a text-only object.
        if self.at_string_start() {
            attrs.push(Attr::Text(self.parse_stringexpr()?));
            while let Some(a) = self.parse_attr(false, false, false, false)? {
                attrs.push(a);
            }
            return Ok(Object {
                span,
                kind: ObjectKind::Text,
                attrs,
            });
        }
        let kind = match self.cur().clone() {
            Token::Prim(p) => {
                self.bump();
                ObjectKind::Primitive(p)
            }
            Token::Block => {
                self.bump();
                ObjectKind::Empty
            }
            Token::Name(n) if n == "brace" => {
                self.bump();
                ObjectKind::Brace
            }
            Token::Name(n) if n == "dot" => {
                self.bump();
                ObjectKind::Dot
            }
            Token::LeftBrack => {
                self.bump();
                let stmts = self.parse_elementlist(&[Token::RightBrack])?;
                self.expect(&Token::RightBrack)?;
                ObjectKind::Block(stmts)
            }
            Token::Str(s) => {
                self.bump();
                attrs.push(Attr::Text(self.continue_string(StringExpr::Lit(s))?));
                ObjectKind::Text
            }
            Token::Kw(Kw::Continue) => {
                self.bump();
                ObjectKind::Continue
            }
            Token::Name(_) | Token::Label(_) => return self.expected_object(),
            _ => return self.expected_here("an object"),
        };
        // `spline <expr> <linespec>`: dpic's documented exception to the bare
        // distance rule — the expression right after `spline` is a tension
        // parameter, not a length. Parse it here so the attribute loop below
        // doesn't read it as `Attr::Dist`.
        if matches!(kind, ObjectKind::Primitive(Prim::Spline)) && self.spline_tension_ahead() {
            attrs.push(Attr::SplineTension(self.parse_expr()?));
        }
        let allow_fit = matches!(
            kind,
            ObjectKind::Primitive(Prim::Box | Prim::Circle | Prim::Ellipse)
        );
        let allow_brace = matches!(kind, ObjectKind::Brace);
        let allow_dot_fill = matches!(kind, ObjectKind::Dot);
        let allow_hatch = matches!(
            kind,
            ObjectKind::Primitive(
                Prim::Box
                    | Prim::Circle
                    | Prim::Ellipse
                    | Prim::Line
                    | Prim::Arrow
                    | Prim::Spline
                    | Prim::Arc
            )
        );
        let allow_close = matches!(kind, ObjectKind::Primitive(Prim::Line));
        while let Some(a) = self.parse_attr(
            allow_fit,
            allow_brace,
            allow_hatch || allow_dot_fill,
            allow_close,
        )? {
            attrs.push(a);
        }
        Ok(Object { kind, attrs, span })
    }

    /// True if the next token begins a bare scalar expression — the leading
    /// tension argument of `spline <expr>` — rather than a linespec keyword
    /// (`from`/`to`/`up`/`then`/…) or another attribute.
    fn spline_tension_ahead(&self) -> bool {
        matches!(
            self.cur(),
            Token::Float(_)
                | Token::Lparen
                | Token::EnvVar(_)
                | Token::Func1(_)
                | Token::Func2(_)
                | Token::Name(_)
                | Token::Minus
                | Token::Plus
                | Token::Kw(Kw::Rand)
        )
    }

    fn parse_attr(
        &mut self,
        allow_fit: bool,
        allow_brace: bool,
        allow_hatch: bool,
        allow_close: bool,
    ) -> PResult<Option<Attr>> {
        // any string expression (literal, sprintf, $arg, concatenation) is text
        if self.at_string_start() {
            return Ok(Some(Attr::Text(self.parse_stringexpr()?)));
        }
        let attr = match self.cur().clone() {
            Token::Kw(Kw::Ht) => {
                self.bump();
                Attr::Dim(DimKind::Ht, self.parse_expr()?)
            }
            Token::Kw(Kw::Wid) => {
                self.bump();
                Attr::Dim(DimKind::Wid, self.parse_expr()?)
            }
            Token::Kw(Kw::Rad) => {
                self.bump();
                Attr::Dim(DimKind::Rad, self.parse_expr()?)
            }
            Token::Kw(Kw::Diam) => {
                self.bump();
                Attr::Dim(DimKind::Diam, self.parse_expr()?)
            }
            Token::Kw(Kw::Thick) => {
                self.bump();
                Attr::Dim(DimKind::Thick, self.parse_expr()?)
            }
            Token::Kw(Kw::Thin) => {
                self.bump();
                Attr::Thin
            }
            Token::Kw(Kw::Scaled) => {
                self.bump();
                Attr::Dim(DimKind::Scaled, self.parse_expr()?)
            }
            Token::Dir(d) => {
                self.bump();
                Attr::Direction(
                    d,
                    self.opt_attr_expr(allow_fit, allow_brace, allow_hatch, allow_close)?,
                )
            }
            Token::LineType(lt) => {
                self.bump();
                Attr::LineStyle(
                    lt,
                    self.opt_attr_expr(allow_fit, allow_brace, allow_hatch, allow_close)?,
                )
            }
            Token::Kw(Kw::Chop) => {
                self.bump();
                Attr::Chop(self.opt_attr_expr(allow_fit, allow_brace, allow_hatch, allow_close)?)
            }
            Token::Kw(Kw::Fill) => {
                self.bump();
                Attr::Fill(self.opt_attr_expr(allow_fit, allow_brace, allow_hatch, allow_close)?)
            }
            Token::Arrow(a) => {
                self.bump();
                Attr::Arrowhead(
                    a,
                    self.opt_attr_expr(allow_fit, allow_brace, allow_hatch, allow_close)?,
                )
            }
            Token::Kw(Kw::Then) => {
                self.bump();
                Attr::Then
            }
            Token::Kw(Kw::Cw) => {
                self.bump();
                Attr::Cw
            }
            Token::Kw(Kw::Ccw) => {
                self.bump();
                Attr::Ccw
            }
            Token::Kw(Kw::Same) => {
                self.bump();
                Attr::Same
            }
            Token::Kw(Kw::Continue) => {
                self.bump();
                Attr::Continue
            }
            Token::Kw(Kw::From) => {
                self.bump();
                Attr::From(self.parse_position()?)
            }
            Token::Kw(Kw::To) => {
                self.bump();
                Attr::To(self.parse_position()?)
            }
            Token::Kw(Kw::At) => {
                self.bump();
                Attr::At(self.parse_position()?)
            }
            Token::Kw(Kw::By) => {
                self.bump();
                Attr::By(self.parse_position()?)
            }
            Token::Kw(Kw::With) => {
                self.bump();
                let anchor = if self.eat(&Token::Dot) {
                    WithAnchor::Place(self.parse_place()?)
                } else if let Token::Corner(c) = self.cur() {
                    let c = *c;
                    self.bump();
                    WithAnchor::Corner(c)
                } else if self.at(&Token::Lparen) {
                    self.bump();
                    let x = self.parse_expr()?;
                    self.expect(&Token::Comma)?;
                    let y = self.parse_expr()?;
                    self.expect(&Token::Rparen)?;
                    WithAnchor::Pair(x, y)
                } else {
                    WithAnchor::Plain
                };
                self.expect_kw(Kw::At)?;
                Attr::With {
                    anchor,
                    at: self.parse_position()?,
                }
            }
            Token::TextPos(tp) => {
                self.bump();
                Attr::TextPos(tp)
            }
            Token::Color(c) => {
                self.bump();
                // a colour may be a quoted string, a bareword name (`shaded
                // Custom`, `outlined red`), `rgb(r,g,b)` or a 0x hex literal.
                let span = Some(self.cur_span());
                Attr::Color(c, self.parse_color_like()?, span)
            }
            Token::Name(n) if allow_fit && n == "fit" => {
                self.bump();
                Attr::Fit
            }
            Token::Name(n) if allow_hatch && n == "hatch" => {
                self.bump();
                Attr::Hatch(HatchKind::Single)
            }
            Token::Name(n) if allow_hatch && n == "crosshatch" => {
                self.bump();
                Attr::Hatch(HatchKind::Cross)
            }
            Token::Name(n) if allow_hatch && n == "hatchangle" => {
                self.bump();
                Attr::HatchAngle(self.parse_expr()?)
            }
            Token::Name(n) if allow_hatch && n == "hatchsep" => {
                self.bump();
                Attr::HatchSep(self.parse_expr()?)
            }
            Token::Name(n) if allow_hatch && (n == "hatchwid" || n == "hatchwidth") => {
                self.bump();
                Attr::HatchWidth(self.parse_expr()?)
            }
            Token::Name(n) if allow_hatch && n == "hatchcolor" => {
                self.bump();
                let span = Some(self.cur_span());
                Attr::HatchColor(self.parse_color_like()?, span)
            }
            Token::Name(n) if allow_hatch && n == "gradient" => {
                self.bump();
                let from_span = Some(self.cur_span());
                let from = self.parse_color_like()?;
                let to_span = Some(self.cur_span());
                let to = self.parse_color_like()?;
                Attr::Gradient(from, from_span, to, to_span)
            }
            Token::Name(n) if allow_hatch && n == "gradientangle" => {
                self.bump();
                Attr::GradientAngle(self.parse_expr()?)
            }
            Token::Name(n) if n == "opacity" => {
                self.bump();
                Attr::Opacity(self.parse_expr()?)
            }
            Token::Name(n) if allow_close && n == "close" => {
                self.bump();
                Attr::Close
            }
            Token::Name(n) if allow_brace && n == "bracepos" => {
                self.bump();
                Attr::BracePos(self.parse_expr()?)
            }
            Token::Name(n) if allow_brace && n == "labeloffset" => {
                self.bump();
                Attr::BraceLabelOffset(self.parse_expr()?)
            }
            Token::Name(n) if n == "bold" => {
                self.bump();
                Attr::Bold
            }
            Token::Name(n) if n == "italic" => {
                self.bump();
                Attr::Italic
            }
            Token::Name(n) if n == "mono" => {
                self.bump();
                Attr::Mono
            }
            Token::Name(n) if n == "font" => {
                self.bump();
                // a family may be a quoted string or a bareword name, like
                // colours (`font "IBM Plex Mono"`, `font serif`)
                let s = match self.cur().clone() {
                    Token::Name(n) | Token::Label(n) => {
                        self.bump();
                        StringExpr::Lit(n)
                    }
                    _ => self.parse_stringexpr()?,
                };
                Attr::Font(s)
            }
            Token::Name(n) if n == "fontsize" => {
                self.bump();
                Attr::FontSize(self.parse_expr()?)
            }
            Token::Name(n) if n == "rotated" => {
                self.bump();
                Attr::Rotated(self.parse_expr()?)
            }
            Token::Name(n) if n == "aligned" => {
                self.bump();
                Attr::Aligned
            }
            Token::Name(n) if n == "big" => {
                self.bump();
                Attr::Sized(true)
            }
            Token::Name(n) if n == "small" => {
                self.bump();
                Attr::Sized(false)
            }
            Token::Name(n) if n == "behind" => {
                self.bump();
                Attr::Behind(self.parse_place()?)
            }
            Token::Name(n) if n == "class" => {
                self.bump();
                Attr::Class(self.parse_stringexpr()?)
            }
            // a bare expression distance with no direction word, e.g. `move 1`,
            // `move -0.1`, `spline x` (length in the prevailing direction)
            Token::Float(_)
            | Token::Lparen
            | Token::EnvVar(_)
            | Token::Func1(_)
            | Token::Func2(_)
            | Token::Name(_)
            | Token::Minus
            | Token::Plus
            | Token::Kw(Kw::Rand) => {
                let span = self.cur_span();
                Attr::Dist(self.parse_expr()?, Some(span))
            }
            _ if self.place_is_scalar_ahead() => {
                let span = self.cur_span();
                Attr::Dist(self.parse_expr()?, Some(span))
            }
            _ => return Ok(None),
        };
        Ok(Some(attr))
    }

    fn expect_kw(&mut self, k: Kw) -> PResult<()> {
        if self.eat_kw(k) {
            Ok(())
        } else {
            self.expected_here(kw_text(k))
        }
    }

    // ---- string expressions ------------------------------------------------

    fn parse_stringexpr(&mut self) -> PResult<StringExpr> {
        let first = self.parse_string_atom()?;
        self.continue_string(first)
    }

    /// Continue a string expression with trailing `+ string` parts.
    fn continue_string(&mut self, first: StringExpr) -> PResult<StringExpr> {
        let mut e = first;
        // Iterative parse, but a left-deep `StringExpr::Concat` recurses on Drop
        // and evaluation — cap the chain length like numeric operator chains.
        let mut ops = 0u32;
        while self.at(&Token::Plus) && self.string_after_plus() {
            self.bump();
            let rhs = self.parse_string_atom()?;
            e = StringExpr::Concat(Box::new(e), Box::new(rhs));
            ops += 1;
            self.check_expr_chain(ops)?;
        }
        Ok(e)
    }

    fn string_after_plus(&self) -> bool {
        self.token_starts_string_at(1)
    }

    fn token_starts_string_at(&self, offset: usize) -> bool {
        match self.toks.get(self.idx + offset).map(|s| &s.tok) {
            Some(Token::Str(_) | Token::Arg(_) | Token::Kw(Kw::Sprintf)) => true,
            Some(Token::Name(n)) if n == "svg_font" => {
                matches!(
                    self.toks.get(self.idx + offset + 1).map(|s| &s.tok),
                    Some(Token::Lparen)
                )
            }
            _ => false,
        }
    }

    fn parse_string_atom(&mut self) -> PResult<StringExpr> {
        match self.cur().clone() {
            Token::Str(s) => {
                self.bump();
                Ok(StringExpr::Lit(s))
            }
            Token::Arg(n) => {
                self.bump();
                Ok(StringExpr::Arg(n))
            }
            Token::Kw(Kw::Sprintf) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let fmt = self.parse_stringexpr()?;
                let mut args = Vec::new();
                while self.eat(&Token::Comma) {
                    args.push(self.parse_expr()?);
                }
                self.expect(&Token::Rparen)?;
                Ok(StringExpr::Sprintf(Box::new(fmt), args))
            }
            Token::Name(n) if n == "svg_font" && matches!(self.peek(1), Token::Lparen) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let mut args = Vec::new();
                if !self.at(&Token::Rparen) {
                    args.push(self.parse_expr()?);
                    while self.eat(&Token::Comma) {
                        args.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::Rparen)?;
                Ok(StringExpr::SvgFont(args))
            }
            other => self.err(format!("expected a string, found {other:?}")),
        }
    }

    // ---- positions ---------------------------------------------------------

    fn parse_label_position(&mut self) -> PResult<Position> {
        let save = self.idx;
        if let Ok(x) = self.parse_expr()
            && self.eat(&Token::Comma)
        {
            let y = self.parse_expr()?;
            return Ok(Position::Pair(x, y));
        }
        self.idx = save;
        self.parse_position()
    }

    /// Positions support vector arithmetic; `+`/`-` are the lowest precedence.
    fn parse_position(&mut self) -> PResult<Position> {
        self.descend(Self::parse_position_inner)
    }

    fn parse_position_inner(&mut self) -> PResult<Position> {
        let mut left = self.parse_pos_mul()?;
        loop {
            let sign = if self.at(&Token::Plus) {
                Sign::Plus
            } else if self.at(&Token::Minus) {
                Sign::Minus
            } else {
                break;
            };
            self.bump();
            let right = self.parse_pos_mul()?;
            left = Position::Sum(sign, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Scaling a position by a scalar: `p * s`, `p / s` (binds tighter than ±).
    fn parse_pos_mul(&mut self) -> PResult<Position> {
        let mut left = self.parse_pos_primary()?;
        loop {
            if self.eat(&Token::Mult) {
                left = Position::Scale(Box::new(left), self.parse_unary()?, false);
            } else if self.eat(&Token::Div) {
                left = Position::Scale(Box::new(left), self.parse_unary()?, true);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_pos_primary(&mut self) -> PResult<Position> {
        // A fraction-led interpolation — `frac between A and B`, `frac <p,p>`, or
        // `frac of the way between A and B`. The fraction can be parenthesised
        // (e.g. `(X/Y) between A and B`), so try it before the `(`/place branches.
        if let Some(p) = self.try_fraction()? {
            return Ok(p);
        }
        // `( … )`: coordinate pair `(x,y)` of scalar expressions, a
        // parenthesised position, or `(pos, pos)` — x of the first, y of the
        // second. Try a scalar pair first so components that are themselves
        // parenthesised scalars (e.g. `((a*g)*cos(t), …)`) parse correctly.
        if self.eat(&Token::Lparen) {
            let save = self.idx;
            // Prefer parsing the contents as position(s) — handles `(A, B.c)`,
            // `(pos, pos)`, `(2,3)`, `(0.5 between A and B)`. If that fails, the
            // contents are scalar coordinate expressions that the position
            // grammar can't represent alone (e.g. `((a*g)*cos t, (a*g)*sin t)`).
            if let Ok(p1) = self.parse_position() {
                let p = if self.eat(&Token::Comma) {
                    let p2 = self.parse_position()?;
                    Position::Place(Location::ParenPair(Box::new(p1), Box::new(p2)))
                } else {
                    p1 // drop the redundant parentheses
                };
                self.expect(&Token::Rparen)?;
                return Ok(p);
            }
            self.idx = save;
            let e1 = self.parse_add()?;
            self.expect(&Token::Comma)?;
            let e2 = self.parse_add()?;
            self.expect(&Token::Rparen)?;
            return Ok(Position::Pair(e1, e2));
        }
        // A leading place is point-valued UNLESS it is a scalar accessor
        // (`place.x` / `.y` / `.attr`), which begins an `(expr, expr)` pair.
        if self.at_place_start() && !self.place_is_scalar_ahead() {
            return Ok(Position::Place(Location::Place(self.parse_place()?)));
        }
        // expression-led coordinate pair `x, y` (interpolation handled above)
        let e1 = self.parse_add()?;
        if self.eat(&Token::Comma) {
            let e2 = self.parse_add()?;
            return Ok(Position::Pair(e1, e2));
        }
        self.err("expected `,`, `between`, or `of the way between` in position")
    }

    /// Try to parse `frac (between | <p,p> | of the way between)`; if the leading
    /// expression isn't followed by an interpolation, backtrack and return `None`
    /// so the caller can parse a plain place / pair / parenthesised position.
    fn try_fraction(&mut self) -> PResult<Option<Position>> {
        let save = self.idx;
        let Ok(frac) = self.parse_add() else {
            self.idx = save;
            return Ok(None);
        };
        let mk = |frac, a, b, of_the_way| {
            Ok(Some(Position::Between {
                frac: Box::new(frac),
                a: Box::new(a),
                b: Box::new(b),
                of_the_way,
            }))
        };
        if self.eat(&Token::Lt) {
            let a = self.parse_position()?;
            self.expect(&Token::Comma)?;
            let b = self.parse_position()?;
            self.expect(&Token::Gt)?;
            return mk(frac, a, b, false);
        }
        let of_the_way = if self.at_kw(Kw::Of) {
            self.eat_kw(Kw::Of);
            if !(self.eat_kw(Kw::The) && self.eat_kw(Kw::Way) && self.eat_kw(Kw::Between)) {
                self.idx = save;
                return Ok(None);
            }
            true
        } else if self.eat_kw(Kw::Between) {
            false
        } else {
            self.idx = save;
            return Ok(None);
        };
        let a = self.parse_position()?;
        self.expect_kw(Kw::And)?;
        let b = self.parse_position()?;
        mk(frac, a, b, of_the_way)
    }

    /// Lookahead: does the upcoming place end in a scalar accessor
    /// (`.x` / `.y` / `.attr`)? If so it is a number, not a point. Non-consuming.
    fn place_is_scalar_ahead(&mut self) -> bool {
        let save = self.idx;
        let parsed = self.parse_place().is_ok();
        let scalar = parsed && matches!(self.cur(), Token::DotX | Token::DotY | Token::Param(_));
        self.idx = save;
        scalar
    }

    fn at_place_start(&self) -> bool {
        match self.cur() {
            Token::Label(_) | Token::Block | Token::Corner(_) => true,
            Token::Kw(Kw::Last) | Token::Kw(Kw::Here) => true,
            Token::Float(_) => matches!(self.peek(1), Token::Kw(Kw::Nth)),
            // `{expr}th …` / `` `expr`th … `` ordinal counts (only valid as a
            // place in position/expression context, never a group here)
            Token::LeftBrace | Token::LeftQuote => true,
            _ => false,
        }
    }

    fn parse_place(&mut self) -> PResult<Place> {
        // `corner [of] placename` — recurses on a leading corner chain
        // (`.n.n.n…`), so bound it through `descend` to avoid a stack overflow.
        if let Token::Corner(c) = self.cur() {
            let c = *c;
            self.bump();
            self.eat_kw(Kw::Of);
            let inner = self.descend(Self::parse_place)?;
            return Ok(Place::CornerOf(c, Box::new(inner)));
        }

        let mut place = self.parse_place_base()?;

        // trailing `.corner`, `.label`, `.nth primobj`. The loop parses
        // iteratively but builds a left-deep `Place::Corner`/`Place::Member`
        // AST whose Drop and evaluation recurse, so cap the chain length.
        let mut accessors = 0u32;
        loop {
            if let Token::Corner(c) = self.cur() {
                let c = *c;
                self.bump();
                place = Place::Corner(Box::new(place), c);
            } else if self.at(&Token::Dot) {
                self.bump();
                let rhs = self.parse_place_base()?;
                place = Place::Member(Box::new(place), Box::new(rhs));
            } else {
                break;
            }
            accessors += 1;
            self.check_expr_chain(accessors)?;
        }
        Ok(place)
    }

    fn parse_place_base(&mut self) -> PResult<Place> {
        match self.cur().clone() {
            Token::Kw(Kw::Here) => {
                self.bump();
                Ok(Place::Here)
            }
            Token::Label(name) => {
                let span = Some(self.cur_span());
                self.bump();
                let subscript = if self.eat(&Token::LeftBrack) {
                    let e = self.parse_subscript()?;
                    self.expect(&Token::RightBrack)?;
                    Some(Box::new(e))
                } else {
                    None
                };
                Ok(Place::Name {
                    name,
                    subscript,
                    span,
                })
            }
            Token::Kw(Kw::Last) | Token::Float(_) | Token::LeftBrace | Token::LeftQuote => {
                let span = Some(self.cur_span());
                let count = self.parse_nth()?;
                // A type keyword may follow (`last box`); without one, this is an
                // untyped reference to the most recent object of any kind
                // (`last`, `last.c`, `2nd last.n`).
                let obj = if self.at_primobj() {
                    self.parse_primobj()?
                } else {
                    PrimObj::Any
                };
                Ok(Place::Nth { count, obj, span })
            }
            other => self.err(format!("expected a place, found {other:?}")),
        }
    }

    fn parse_nth(&mut self) -> PResult<Nth> {
        if self.eat_kw(Kw::Last) {
            return Ok(Nth::Last);
        }
        // ncount ordinal [last]
        let e = self.parse_ncount()?;
        self.expect_kw(Kw::Nth)?;
        let from_last = self.eat_kw(Kw::Last);
        Ok(Nth::Count(Box::new(e), from_last))
    }

    /// An ordinal count: a number, `` `expr' ``, or `{ expr }` (grammar:
    /// `ncount`). Must NOT recurse into the general expression grammar on a
    /// bare number, or `2nd` would re-enter place parsing.
    fn parse_ncount(&mut self) -> PResult<Expr> {
        match self.cur().clone() {
            Token::Float(v) => {
                self.bump();
                Ok(Expr::Num(v))
            }
            Token::LeftBrace => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(&Token::RightBrace)?;
                Ok(e)
            }
            Token::LeftQuote => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(&Token::RightQuote)?;
                Ok(e)
            }
            other => self.err(format!("expected an ordinal count, found {other:?}")),
        }
    }

    /// Whether the current token can begin a primitive-object type keyword
    /// (`box`, `[`, a string, …) — i.e. an explicit type after `last`/ordinal.
    fn at_primobj(&self) -> bool {
        matches!(
            self.cur(),
            Token::Prim(_) | Token::Block | Token::Str(_) | Token::LeftBrack
        ) || matches!(self.cur(), Token::Name(n) if n == "brace")
    }

    fn parse_primobj(&mut self) -> PResult<PrimObj> {
        match self.cur().clone() {
            Token::Prim(p) => {
                self.bump();
                Ok(PrimObj::Prim(p))
            }
            Token::Name(n) if n == "brace" => {
                self.bump();
                Ok(PrimObj::Brace)
            }
            Token::Block => {
                self.bump();
                Ok(PrimObj::Block)
            }
            Token::Str(s) => {
                self.bump();
                Ok(PrimObj::Str(s))
            }
            Token::LeftBrack => {
                self.bump();
                self.expect(&Token::RightBrack)?;
                Ok(PrimObj::EmptyBrack)
            }
            other => self.err(format!("expected a primitive object, found {other:?}")),
        }
    }

    // ---- expressions -------------------------------------------------------

    fn opt_expr(&mut self) -> PResult<Option<Expr>> {
        if self.starts_scalar() || self.place_is_scalar_ahead() {
            Ok(Some(self.parse_expr()?))
        } else {
            Ok(None)
        }
    }

    /// A color argument: a bare name (`red`), a label-cased name, or any
    /// string expression — the same grammar `hatchcolor` accepts.
    fn parse_color_like(&mut self) -> PResult<StringExpr> {
        match self.cur().clone() {
            // rpic extension: `rgb(r,g,b)` colour literal
            Token::Name(n) if n == "rgb" && matches!(self.peek(1), Token::Lparen) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let r = self.parse_expr()?;
                self.expect(&Token::Comma)?;
                let g = self.parse_expr()?;
                self.expect(&Token::Comma)?;
                let b = self.parse_expr()?;
                self.expect(&Token::Rparen)?;
                Ok(StringExpr::Rgb(Box::new([r, g, b])))
            }
            Token::Name(n) | Token::Label(n) => {
                self.bump();
                Ok(StringExpr::Lit(n))
            }
            // rpic extension (pikchr-style): a numeric colour — typically a
            // `0xRRGGBB` hex literal (`shaded 0x1b5e20`), or a parenthesised
            // expression in colour position (`colored (base + 0x10)`).
            Token::Float(_) | Token::Lparen => {
                Ok(StringExpr::ColorNum(Box::new(self.parse_expr()?)))
            }
            _ => self.parse_stringexpr(),
        }
    }

    fn opt_attr_expr(
        &mut self,
        allow_fit: bool,
        allow_brace: bool,
        allow_hatch: bool,
        allow_close: bool,
    ) -> PResult<Option<Expr>> {
        if self.contextual_attr_ahead(allow_fit, allow_brace, allow_hatch, allow_close) {
            Ok(None)
        } else {
            self.opt_expr()
        }
    }

    fn contextual_attr_ahead(
        &self,
        allow_fit: bool,
        allow_brace: bool,
        allow_hatch: bool,
        allow_close: bool,
    ) -> bool {
        matches!(
            self.cur(),
            Token::Name(n)
                if (allow_fit && n == "fit")
                    || (allow_hatch
                        && matches!(
                            n.as_str(),
                            "hatch"
                                | "crosshatch"
                                | "hatchangle"
                                | "hatchsep"
                                | "hatchwid"
                                | "hatchwidth"
                                | "hatchcolor"
                                | "gradient"
                                | "gradientangle"
                        ))
                    || n == "opacity"
                    || (allow_brace && matches!(n.as_str(), "bracepos" | "labeloffset"))
                    || n == "behind"
                    || n == "class"
                    || (allow_close && n == "close")
        )
    }

    fn starts_scalar(&self) -> bool {
        matches!(
            self.cur(),
            Token::Float(_)
                | Token::Name(_)
                | Token::EnvVar(_)
                | Token::Lparen
                | Token::Minus
                | Token::Plus
                | Token::Not
                | Token::Func1(_)
                | Token::Func2(_)
                | Token::Kw(Kw::Rand)
                | Token::ArgCount
        )
    }

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.descend(Self::parse_or)
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut e = self.parse_and()?;
        let mut ops = 0;
        while self.eat(&Token::OrOr) {
            ops += 1;
            self.check_expr_chain(ops)?;
            let r = self.parse_and()?;
            e = Expr::Bin(BinOp::Or, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut e = self.parse_cmp()?;
        let mut ops = 0;
        while self.eat(&Token::AndAnd) {
            ops += 1;
            self.check_expr_chain(ops)?;
            let r = self.parse_cmp()?;
            e = Expr::Bin(BinOp::And, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_cmp(&mut self) -> PResult<Expr> {
        let mut e = self.parse_add()?;
        let mut ops = 0;
        loop {
            let op = match self.cur() {
                Token::EqEq => BinOp::Eq,
                Token::Neq => BinOp::Ne,
                Token::Lt => BinOp::Lt,
                Token::Le => BinOp::Le,
                Token::Gt => BinOp::Gt,
                Token::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            ops += 1;
            self.check_expr_chain(ops)?;
            let r = self.parse_add()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_add(&mut self) -> PResult<Expr> {
        let mut e = self.parse_mul()?;
        let mut ops = 0;
        loop {
            let op = match self.cur() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            ops += 1;
            self.check_expr_chain(ops)?;
            let r = self.parse_mul()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_mul(&mut self) -> PResult<Expr> {
        let mut e = self.parse_unary()?;
        let mut ops = 0;
        loop {
            let op = match self.cur() {
                Token::Mult => BinOp::Mul,
                Token::Div => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            ops += 1;
            self.check_expr_chain(ops)?;
            let r = self.parse_unary()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let mut ops = Vec::new();
        loop {
            let op = match self.cur() {
                Token::Minus => Some(UnOp::Neg),
                Token::Plus => Some(UnOp::Pos),
                Token::Not => Some(UnOp::Not),
                _ => None,
            };
            let Some(op) = op else {
                break;
            };
            self.bump();
            ops.push(op);
            self.check_expr_chain(ops.len() as u32)?;
        }
        let mut e = self.parse_pow()?;
        for op in ops.into_iter().rev() {
            e = Expr::Unary(op, Box::new(e));
        }
        Ok(e)
    }

    fn parse_pow(&mut self) -> PResult<Expr> {
        let base = self.parse_primary()?;
        if self.eat(&Token::Caret) {
            let exp = self.descend(Self::parse_unary)?; // right-associative
            Ok(Expr::Bin(BinOp::Pow, Box::new(base), Box::new(exp)))
        } else {
            Ok(base)
        }
    }

    /// Lookahead from just inside a `(`: is the matching `)` immediately followed
    /// by `.x` or `.y`? (Used to read `( position ).x` as a coordinate.)
    fn paren_followed_by_dot_xy(&self) -> bool {
        let mut depth = 1i32;
        let mut i = self.idx;
        while let Some(s) = self.toks.get(i) {
            match &s.tok {
                Token::Lparen => depth += 1,
                Token::Rparen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.toks.get(i + 1).map(|t| &t.tok),
                            Some(Token::DotX | Token::DotY)
                        );
                    }
                }
                Token::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        // place-derived scalars: location.x / location.y / place.attr
        if self.at_place_start() && self.place_is_scalar_ahead() {
            return self.parse_place_scalar();
        }
        // a string operand (only meaningful as an `==`/`!=` operand)
        if self.at_string_start() {
            return Ok(Expr::Str(self.parse_stringexpr()?));
        }
        match self.cur().clone() {
            Token::Float(v) => {
                self.bump();
                Ok(Expr::Num(v))
            }
            Token::Name(name) | Token::Label(name) => {
                self.bump();
                let subscript = if self.eat(&Token::LeftBrack) {
                    let e = self.parse_subscript()?;
                    self.expect(&Token::RightBrack)?;
                    Some(Box::new(e))
                } else {
                    None
                };
                Ok(Expr::Var(name, subscript))
            }
            Token::EnvVar(v) => {
                self.bump();
                Ok(Expr::Env(v))
            }
            Token::Lparen => {
                self.bump();
                // embedded assignment `( name = expr )` yields the assigned value
                if matches!(self.cur(), Token::Name(_) | Token::Label(_))
                    && self.assignment_op_after_var_ref()
                {
                    let name = match self.bump() {
                        Token::Name(n) | Token::Label(n) => n,
                        _ => unreachable!(),
                    };
                    let subscript = if self.eat(&Token::LeftBrack) {
                        let e = self.parse_subscript()?;
                        self.expect(&Token::RightBrack)?;
                        Some(Box::new(e))
                    } else {
                        None
                    };
                    self.bump(); // `=`
                    let v = self.parse_expr()?;
                    self.expect(&Token::Rparen)?;
                    return Ok(Expr::Assign(name, subscript, Box::new(v)));
                }
                // `( position ).x` / `.y` — a coordinate of a parenthesised
                // position (e.g. `(A - B).x`, `($1-($2)).y`). Chosen by lookahead
                // for a trailing `.x`/`.y`, since `(A - B)` alone parses as scalar
                // (labels read as variables).
                if self.paren_followed_by_dot_xy() {
                    let pos = self.parse_position()?;
                    self.expect(&Token::Rparen)?;
                    let loc = Location::Paren(Box::new(pos));
                    return Ok(if self.eat(&Token::DotX) {
                        Expr::DotX(loc)
                    } else {
                        self.expect(&Token::DotY)?;
                        Expr::DotY(loc)
                    });
                }
                // a plain scalar group `( expr )`
                let e = self.parse_expr()?;
                self.expect(&Token::Rparen)?;
                Ok(e)
            }
            // `$+` outside any macro invocation: zero arguments
            Token::ArgCount => {
                self.bump();
                Ok(Expr::Num(0.0))
            }
            Token::Func1(f) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let e = self.parse_expr()?;
                self.expect(&Token::Rparen)?;
                Ok(Expr::Func1(f, Box::new(e)))
            }
            Token::Func2(f) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let a = self.parse_expr()?;
                self.expect(&Token::Comma)?;
                let b = self.parse_expr()?;
                self.expect(&Token::Rparen)?;
                Ok(Expr::Func2(f, Box::new(a), Box::new(b)))
            }
            Token::Kw(Kw::Rand) => {
                self.bump();
                self.expect(&Token::Lparen)?;
                let arg = if self.at(&Token::Rparen) {
                    None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };
                self.expect(&Token::Rparen)?;
                Ok(Expr::Rand(arg))
            }
            other => self.err(format!("expected an expression, found {other:?}")),
        }
    }

    /// Parse a place followed by `.x` / `.y` / `.attr` to yield a scalar.
    fn parse_place_scalar(&mut self) -> PResult<Expr> {
        let place = self.parse_place()?;
        match self.cur().clone() {
            Token::DotX => {
                self.bump();
                Ok(Expr::DotX(Location::Place(place)))
            }
            Token::DotY => {
                self.bump();
                Ok(Expr::DotY(Location::Place(place)))
            }
            Token::Param(p) => {
                self.bump();
                Ok(Expr::PlaceAttr(place, p))
            }
            other => self.err(format!(
                "a place is not a number here; expected `.x`, `.y`, or an attribute, found {other:?}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(src: &str) -> Picture {
        parse(src).unwrap_or_else(|e| panic!("parse error: {e}"))
    }

    #[test]
    fn kernighan_pipeline() {
        let p = pic(r#".PS
ellipse "document"
arrow
box "PIC"
arrow
box "TBL/EQN" "(optional)" dashed
arrow
box "TROFF"
arrow
ellipse "typesetter"
.PE
"#);
        assert_eq!(p.stmts.len(), 9);
        // the dashed box with two strings
        if let Stmt::Object { object, .. } = &p.stmts[4] {
            assert_eq!(object.kind, ObjectKind::Primitive(Prim::Box));
            let texts = object
                .attrs
                .iter()
                .filter(|a| matches!(a, Attr::Text(_)))
                .count();
            assert_eq!(texts, 2);
            assert!(
                object
                    .attrs
                    .iter()
                    .any(|a| matches!(a, Attr::LineStyle(LineType::Dashed, _)))
            );
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn box_with_dims_and_at() {
        let p = pic("box ht 0.3 wid 0.5 at 0.25,0.15");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Ht, _)));
        assert!(matches!(object.attrs[1], Attr::Dim(DimKind::Wid, _)));
        assert!(matches!(object.attrs[2], Attr::At(Position::Pair(_, _))));
    }

    #[test]
    fn behind_parses_as_contextual_extension_attribute() {
        let p = pic("A: box\nbox behind A");
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        let Some(Attr::Behind(Place::Name {
            name, subscript, ..
        })) = object.attrs.last()
        else {
            panic!("expected behind attribute");
        };
        assert_eq!(name, "A");
        assert!(subscript.is_none());

        let p = pic("behind = 2\nbox wid behind");
        assert_eq!(p.stmts.len(), 2);
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
    }

    #[test]
    fn fit_parses_as_contextual_extension_attribute() {
        let p = pic("box \"long label\" fit");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Fit)));

        let p = pic("fit = 2\nbox wid fit");
        assert_eq!(p.stmts.len(), 2);
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));

        let p = pic("fit = 2\nline fit");
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dist(_, _)));
    }

    #[test]
    fn hatch_parses_as_contextual_extension_attribute() {
        let p = pic("box hatch hatchangle 30 hatchsep .05 hatchwid 1.2 hatchcolor red");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::Hatch(HatchKind::Single)))
        );
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::HatchAngle(_)))
        );
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::HatchSep(_))));
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::HatchWidth(_)))
        );
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::HatchColor(..)))
        );

        let p = pic("hatch = 2\nbox wid hatch");
        assert_eq!(p.stmts.len(), 2);
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Wid, _)));
    }

    #[test]
    fn opacity_parses_as_contextual_extension_attribute() {
        let p = pic("box opacity 0.5");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Opacity(_))));

        let p = pic("opacity = 2\nbox wid opacity");
        assert_eq!(p.stmts.len(), 2);
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Wid, _)));
    }

    #[test]
    fn close_parses_as_contextual_line_extension_attribute() {
        let p = pic("line right then up close");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Close)));

        let p = pic("close = 2\nbox wid close");
        assert_eq!(p.stmts.len(), 2);
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Wid, _)));
    }

    #[test]
    fn gradient_parses_as_contextual_extension_attribute() {
        let p = pic("box gradient \"steelblue\" white gradientangle 45");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Gradient(..))));
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::GradientAngle(_)))
        );

        // contextual fallback: `gradient` stays usable as a variable
        let p = pic("gradient = 2\nbox wid gradient");
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Wid, _)));
    }

    #[test]
    fn class_parses_inline_and_statement_forms() {
        let p = pic("box class \"critical\"");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Class(_))));

        let p = pic("A: box\nclass A \"hot\"\nclass last box \"cold\"");
        assert!(matches!(p.stmts[1], Stmt::Class { .. }));
        assert!(matches!(p.stmts[2], Stmt::Class { .. }));

        // contextual fallbacks: assignment and expression use survive
        let p = pic("class = 2\nbox wid class");
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(object.attrs[0], Attr::Dim(DimKind::Wid, _)));
    }

    #[test]
    fn brace_parses_as_contextual_extension_object() {
        let p = pic(
            "A: box\nB: box\nbrace from A.e to B.w down \"group\" wid .2 bracepos .4 labeloffset .1",
        );
        let Stmt::Object { object, .. } = &p.stmts[2] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Brace);
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::From(_))));
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::To(_))));
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::Direction(Dir::Down, None)))
        );
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::Text(_))));
        assert!(object.attrs.iter().any(|a| matches!(a, Attr::BracePos(_))));
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::BraceLabelOffset(_)))
        );

        let p = pic("brace = 2\nline right brace");
        assert!(matches!(p.stmts[0], Stmt::Assign(_)));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Line));

        let p = pic("brace from 0,0 to 1,0\nline from last brace.start to last brace.end");
        assert_eq!(p.stmts.len(), 2);
    }

    #[test]
    fn labeled_and_corners() {
        let p = pic("B1: box\narc -> from top of B1 to last box.ne");
        assert!(matches!(p.stmts[0], Stmt::Object { label: Some(_), .. }));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Arc));
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::Arrowhead(Arrow::Right, _)))
        );
        // from top of B1
        assert!(object.attrs.iter().any(|a| matches!(
            a,
            Attr::From(Position::Place(Location::Place(Place::CornerOf(
                Corner::N,
                _
            ))))
        )));
    }

    #[test]
    fn with_at_and_shift() {
        let p = pic("ellipse \"2\" with .nw at last ellipse.se + (0.1,0)");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        let with = object
            .attrs
            .iter()
            .find(|a| matches!(a, Attr::With { .. }))
            .unwrap();
        let Attr::With { anchor, at } = with else {
            panic!()
        };
        assert_eq!(*anchor, WithAnchor::Corner(Corner::Nw));
        // `last ellipse.se + (0.1,0)` is a position sum
        assert!(matches!(at, Position::Sum(Sign::Plus, _, _)));
    }

    #[test]
    fn with_member_anchor_parses() {
        let p = pic("[ A: box ] with .A.c at Here");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        let with = object
            .attrs
            .iter()
            .find(|a| matches!(a, Attr::With { .. }))
            .unwrap();
        let Attr::With { anchor, .. } = with else {
            panic!()
        };
        assert!(matches!(
            anchor,
            WithAnchor::Place(Place::Corner(inner, Corner::Center))
                if matches!(inner.as_ref(), Place::Name { name, .. } if name == "A")
        ));
    }

    #[test]
    fn expression_precedence() {
        // 2 + 3 * 4 ^ 2  ==  2 + (3 * (4^2)) = 50
        let p = pic("x = 2 + 3 * 4 ^ 2");
        let Stmt::Assign(list) = &p.stmts[0] else {
            panic!()
        };
        // structure: Add(2, Mul(3, Pow(4,2)))
        let Expr::Bin(BinOp::Add, _, rhs) = &list[0].value else {
            panic!("expected top-level add")
        };
        assert!(matches!(**rhs, Expr::Bin(BinOp::Mul, _, _)));
    }

    #[test]
    fn between_position() {
        let p = pic("arrow from 1/3 of the way between A.ne and A.se");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(
            a,
            Attr::From(Position::Between {
                of_the_way: true,
                ..
            })
        )));
    }

    #[test]
    fn assignment_list_and_envvar() {
        let p = pic("boxht = 0.3; boxwid = 2 * boxht");
        assert_eq!(p.stmts.len(), 2);
        let Stmt::Assign(a0) = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(a0[0].target, AssignTarget::Env(EnvVar::Boxht));
    }

    #[test]
    fn dpic_svg_font_stub_parses_as_string() {
        let p = pic("print svg_font(\"Times\", 12)");
        let Stmt::Print(PrintItem::Str(StringExpr::SvgFont(args))) = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn subscripted_variable_refs_parse() {
        let p = pic("P[1] = 2\nx = P[1]");
        let Stmt::Assign(a0) = &p.stmts[0] else {
            panic!()
        };
        assert!(matches!(&a0[0].target, AssignTarget::Var(name, Some(_)) if name == "P"));

        let Stmt::Assign(a1) = &p.stmts[1] else {
            panic!()
        };
        assert!(matches!(&a1[0].value, Expr::Var(name, Some(_)) if name == "P"));
    }

    #[test]
    fn block_object() {
        let p = pic("[ box; circle ] with .nw at Here");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        let ObjectKind::Block(inner) = &object.kind else {
            panic!()
        };
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn diamond_line_with_then() {
        let p = pic("line up right then down right then down left then up left");
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        let thens = object
            .attrs
            .iter()
            .filter(|a| matches!(a, Attr::Then))
            .count();
        assert_eq!(thens, 3);
    }

    #[test]
    fn place_scalar_in_coord_pair() {
        // issue #3: (A.x, expr) must parse as an (expr,expr) pair, not a place
        let p = pic("A: box\n\"t\" at (A.x, A.y - 0.5)");
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert!(
            object
                .attrs
                .iter()
                .any(|a| matches!(a, Attr::At(Position::Pair(_, _))))
        );
        // a plain point place still parses as a place position
        let q = pic("A: box\nbox at A.ne");
        let Stmt::Object { object, .. } = &q.stmts[1] else {
            panic!()
        };
        assert!(object.attrs.iter().any(|a| matches!(
            a,
            Attr::At(Position::Place(Location::Place(Place::Corner(_, _))))
        )));
    }

    #[test]
    fn ignores_non_svg_backend_preambles() {
        let p = pic(r#".PS
verbatimtex
\global\def\foo#1{#1}
etex
\global\def\bar#1{#1}
\psset{arrowsize=4pt}
box
.PE
"#);
        assert_eq!(p.stmts.len(), 1);
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Box));
    }

    #[test]
    fn backend_filter_keeps_global_lines_inside_strings() {
        let p = pic(
            "sh \"echo -n \\\"print \\\\\"\\\" > x\"\nif dpicopt==optPGF then { command \"cycle; \\\n\\global\\let\\dpicdraw=x\" } else { box }",
        );
        assert_eq!(p.stmts.len(), 2);
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Box));
    }

    #[test]
    fn static_if_copy_defines_macros_before_following_statements() {
        let dir = std::env::temp_dir().join(format!("rpic_static_if_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("macros.pic"), "define makebox { box wid $1 }\n").unwrap();
        std::fs::write(
            dir.join("inc.pic"),
            "define makecircle { circle rad 0.1 }\n",
        )
        .unwrap();
        let p = parse_in_dir(
            "if \"plotlib\" != \"1\" then { copy \"macros.pic\" }\ndefine choose { if \"$1\"==\"\" then { box } else { copy \"$1/inc.pic\" } }\nchoose(.)\nmakecircle()\nmakebox(0.4)",
            Some(dir.as_path()),
        )
        .unwrap_or_else(|e| panic!("parse error: {e}"));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(p.stmts.len(), 2);
        let Stmt::Object { object, .. } = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Circle));
        let Stmt::Object { object, .. } = &p.stmts[1] else {
            panic!()
        };
        assert_eq!(object.kind, ObjectKind::Primitive(Prim::Box));
    }

    #[test]
    fn unsupported_control_is_clear() {
        // `copy "file"` with no filesystem context reports a clear file error
        let e = parse("copy \"x\"").unwrap_err();
        assert!(e.msg.contains("copy") && e.msg.contains("file"));
    }

    #[test]
    fn deeply_nested_input_errors_instead_of_overflowing() {
        // #283: pathological nesting must return a clean error, not abort the
        // process by overflowing the stack. Run on a roomy thread stack:
        // reaching MAX_PARSE_DEPTH costs far more stack in an unoptimized test
        // build than the harness's default thread provides (release/wasm frames
        // are much smaller — that's what the limit is tuned for).
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let deep_parens = format!("box wid {}1{}", "(".repeat(5000), ")".repeat(5000));
                let e = parse(&deep_parens).unwrap_err();
                assert!(e.msg.contains("nested too deeply"), "{}", e.msg);
                let deep_blocks = format!("{}box{}", "[".repeat(5000), "]".repeat(5000));
                assert!(
                    parse(&deep_blocks)
                        .unwrap_err()
                        .msg
                        .contains("nested too deeply")
                );
                // a realistic depth (the corpus max is 4) parses fine
                assert!(parse("[[[[ box ]]]]").is_ok());
                assert!(parse(&format!("box wid {}1{}", "(".repeat(64), ")".repeat(64))).is_ok());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn flat_binary_expression_chain_errors_instead_of_overflowing() {
        // #306: flat chains used to bypass `descend` and later overflow the
        // evaluator stack. They now fail at parse time with a normal error.
        let expr = (0..2000).map(|_| "1").collect::<Vec<_>>().join("+");
        let e = parse(&format!("x = {expr}")).unwrap_err();
        assert!(e.msg.contains("too many chained operators"), "{}", e.msg);
    }

    #[test]
    fn recursion_and_ast_depth_vectors_error_instead_of_overflowing() {
        // #318: four constructs that #306/#314 left uncovered used to abort by
        // overflowing the stack (parser recursion) or dropping/evaluating an
        // unbounded left-deep AST. Each must now return a clean parse error.
        // Roomy stack for the unoptimized test build (see the #283 test).
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                // brace-group nesting (parser recursion)
                let braces = format!("{}box{}", "{".repeat(5000), "}".repeat(5000));
                assert!(
                    parse(&braces)
                        .unwrap_err()
                        .msg
                        .contains("nested too deeply"),
                    "brace group"
                );
                // leading corner chain (parser recursion)
                let corners = format!("line to {}Here", ".n".repeat(5000));
                assert!(
                    parse(&corners)
                        .unwrap_err()
                        .msg
                        .contains("nested too deeply"),
                    "corner chain"
                );
                // string concatenation (left-deep StringExpr::Concat)
                let concat = vec!["\"a\""; 2000].join("+");
                assert!(
                    parse(&format!("print {concat}"))
                        .unwrap_err()
                        .msg
                        .contains("too many chained operators"),
                    "string concat"
                );
                // trailing member/corner chain in place context (left-deep AST)
                let members = format!("line to A{}.sw", ".B".repeat(2000));
                assert!(
                    parse(&format!("box\n{members}"))
                        .unwrap_err()
                        .msg
                        .contains("too many chained operators"),
                    "member chain"
                );
                // realistic depths still parse
                assert!(parse("{{{{ box }}}}").is_ok());
                assert!(parse("box\nline to A.n.sw").is_ok());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn control_constructs_parse() {
        assert!(parse("for i = 1 to 3 do { box }").is_ok());
        assert!(parse("if 1 > 0 then { box } else { circle }").is_ok());
        assert!(parse("reset boxht, boxwid").is_ok());
        // define is consumed by the preprocessor and expanded
        let p = parse("define e { box }\ne\ne").unwrap();
        assert_eq!(p.stmts.len(), 2);
    }
}
