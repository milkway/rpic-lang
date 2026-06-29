//! Recursive-descent parser for the pic drawing core.
//!
//! Follows dpic's `grammar.txt`. Implemented: pictures (`.PS … .PE`), primitives
//! with the full attribute set, positions (pairs, places, corners, ordinals,
//! `between`, `± shifts`), expressions with proper precedence, `[ … ]` blocks,
//! `{ … }` groups, labels and assignments. Control constructs
//! (`if`/`for`/`define`/`print`/`sh`/…) are reported as unsupported for now.

use crate::ast::*;
use crate::lexer::{LexError, Spanned, lex};
use crate::token::*;

/// A parse error with source location.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            msg: e.msg,
            line: e.line,
            col: e.col,
        }
    }
}

/// Parse a full source string into a [`Picture`] with no filesystem context.
/// `copy "file"` includes are unavailable (they require a base directory).
pub fn parse(src: &str) -> Result<Picture, ParseError> {
    parse_in_dir(src, None)
}

/// Parse pic source, resolving `copy "file"` includes relative to `base`.
pub fn parse_in_dir(src: &str, base: Option<&Path>) -> Result<Picture, ParseError> {
    let src = strip_backend_preamble(src);
    let toks = lex(&src)?;
    let (toks, macros) = preprocess(toks, base)?;
    let mut pic = Parser::new(toks).parse_picture()?;
    pic.macros = macros;
    pic.base_dir = base.map(|p| p.to_path_buf());
    Ok(pic)
}

/// Parse a deferred body (the raw tokens of an `if`/`for` block) with the macro
/// table in scope, expanding macro calls (and `copy` includes) along this
/// executed path. Used by the evaluator so dead branches and recursive macros
/// are never parsed.
pub fn parse_body_tokens(
    toks: &[Spanned],
    macros: &Macros,
    base: Option<&Path>,
) -> Result<Vec<Stmt>, ParseError> {
    let mut m = macros.clone();
    let mut input = toks.to_vec();
    input.push(Spanned::new(Token::Eof, 0, 0));
    let expanded = expand(&input, &mut m, 0, base)?;
    let mut p = Parser::new(expanded);
    p.parse_elementlist(&[])
}

/// Parse pic source produced by `exec`, applying the caller's macro argument
/// frame before normal macro expansion.
pub(crate) fn parse_exec_source(
    src: &str,
    macros: &Macros,
    base: Option<&Path>,
    arg_frame: Option<&[Vec<Spanned>]>,
) -> Result<Vec<Stmt>, ParseError> {
    let mut toks = lex(src)?;
    if let Some(args) = arg_frame {
        toks = substitute(&toks, args);
    }
    let mut m = macros.clone();
    let expanded = expand(&toks, &mut m, 0, base)?;
    let mut p = Parser::new(expanded);
    p.parse_elementlist(&[])
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

    for line in src.lines() {
        let trimmed = line.trim_start();
        if in_verbatimtex {
            out.push('\n');
            if starts_word(trimmed, "etex") {
                in_verbatimtex = false;
            }
            continue;
        }

        if starts_word(trimmed, "verbatimtex") {
            in_verbatimtex = true;
            out.push('\n');
        } else if is_ignored_backend_line(trimmed) {
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
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
    base: Option<&Path>,
) -> Result<(Vec<Spanned>, Macros), ParseError> {
    let mut macros: Macros = builtin_unit_macros();
    let out = expand(&input, &mut macros, 0, base)?;
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
            let body_toks: Vec<Spanned> = toks.into_iter().filter(|s| s.tok != Token::Eof).collect();
            m.insert(name.to_string(), body_toks);
        }
    }
    m
}

fn loc(toks: &[Spanned], i: usize) -> (u32, u32) {
    toks.get(i).map(|s| (s.line, s.col)).unwrap_or((0, 0))
}

fn expand(
    toks: &[Spanned],
    macros: &mut HashMap<String, Vec<Spanned>>,
    depth: usize,
    base: Option<&Path>,
) -> Result<Vec<Spanned>, ParseError> {
    if depth > 64 {
        return Err(ParseError {
            msg: "macro expansion too deep (recursive define?)".into(),
            line: 0,
            col: 0,
        });
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i].tok {
            Token::Kw(Kw::Define) => {
                let (l, c) = loc(toks, i);
                i += 1;
                let name = match toks.get(i).map(|s| &s.tok) {
                    Some(Token::Name(n)) | Some(Token::Label(n)) => n.clone(),
                    _ => {
                        return Err(ParseError {
                            msg: "define: expected a macro name".into(),
                            line: l,
                            col: c,
                        });
                    }
                };
                i += 1;
                // the `{` body may begin on a following line
                while toks.get(i).map(|s| &s.tok) == Some(&Token::Newline) {
                    i += 1;
                }
                if toks.get(i).map(|s| &s.tok) != Some(&Token::LeftBrace) {
                    let (l, c) = loc(toks, i);
                    return Err(ParseError {
                        msg: "define: expected `{` (only `define name { body }` is supported)"
                            .into(),
                        line: l,
                        col: c,
                    });
                }
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
                    return Err(ParseError {
                        msg: "define: unterminated `{` body".into(),
                        line: l,
                        col: c,
                    });
                }
                let body = toks[start..i].to_vec();
                i += 1; // past `}`
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
                out.push(toks[i].clone());
                i += 1;
                let Some(te) = find_kw_depth0(toks, i, Kw::Then) else {
                    continue; // malformed; let the parser report it
                };
                out.extend(expand(&toks[i..te], macros, depth + 1, base)?);
                out.push(toks[te].clone()); // `then`
                i = copy_braced(toks, te + 1, &mut out)?;
                // optional `else { … }` (possibly across newlines)
                let mut j = i;
                while matches!(toks.get(j).map(|s| &s.tok), Some(Token::Newline)) {
                    j += 1;
                }
                if matches!(toks.get(j).map(|s| &s.tok), Some(Token::Kw(Kw::Else))) {
                    out.push(toks[j].clone());
                    i = copy_braced(toks, j + 1, &mut out)?;
                }
            }
            Token::Kw(Kw::For) => {
                out.push(toks[i].clone());
                i += 1;
                let Some(de) = find_kw_depth0(toks, i, Kw::Do) else {
                    continue;
                };
                out.extend(expand(&toks[i..de], macros, depth + 1, base)?);
                out.push(toks[de].clone()); // `do`
                i = copy_braced(toks, de + 1, &mut out)?;
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
                let expanded = expand(&sub, macros, depth + 1, base)?;
                out.extend(expanded);
            }
            // `copy "file"` splices another pic file's (expanded) tokens inline.
            Token::Kw(Kw::Copy) => {
                let (l, c) = loc(toks, i);
                i += 1;
                let Some(Token::Str(fname)) = toks.get(i).map(|s| &s.tok) else {
                    return Err(ParseError {
                        msg:
                            "copy: expected a quoted file name (only `copy \"file\"` is supported)"
                                .into(),
                        line: l,
                        col: c,
                    });
                };
                let fname = fname.clone();
                i += 1;
                let inc = include_file(base, &fname, macros, depth, l, c)?;
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
            return Err(ParseError {
                msg: "unterminated macro arguments".into(),
                line: 0,
                col: 0,
            });
        };
        match &s.tok {
            Token::Lparen | Token::LeftBrack | Token::LeftBrace => {
                depth += 1;
                cur.push(s.clone());
                i += 1;
            }
            Token::Rparen if depth == 0 => {
                i += 1;
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
                args.push(std::mem::take(&mut cur));
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

/// Replace `$k` argument tokens in a macro body with the k-th argument's tokens.
fn substitute(body: &[Spanned], args: &[Vec<Spanned>]) -> Vec<Spanned> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
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
/// inside string literals (numbers, names and strings; other tokens elide).
fn tokens_to_text(toks: &[Spanned]) -> String {
    let mut s = String::new();
    for t in toks {
        match &t.tok {
            Token::Float(v) => s.push_str(&format!("{v}")),
            Token::Str(t) => s.push_str(t),
            Token::Name(n) | Token::Label(n) => s.push_str(n),
            _ => {}
        }
    }
    s
}

/// Read and tokenize a `copy "file"` include, returning its expanded tokens
/// (with the trailing `Eof` removed so it splices cleanly mid-stream). The
/// included file resolves nested `copy`s relative to its own directory.
fn include_file(
    base: Option<&Path>,
    fname: &str,
    macros: &mut HashMap<String, Vec<Spanned>>,
    depth: usize,
    l: u32,
    c: u32,
) -> Result<Vec<Spanned>, ParseError> {
    let mkerr = |msg: String| ParseError {
        msg,
        line: l,
        col: c,
    };
    let p = Path::new(fname);
    let path = if p.is_absolute() {
        p.to_path_buf()
    } else {
        match base {
            Some(b) => b.join(p),
            None => {
                return Err(mkerr(format!(
                    "copy \"{fname}\": file includes require a file path (unavailable here)"
                )));
            }
        }
    };
    let content =
        std::fs::read_to_string(&path).map_err(|e| mkerr(format!("copy \"{fname}\": {e}")))?;
    let toks = lex(&content)?;
    let inc_base = path.parent().map(|d| d.to_path_buf());
    let mut expanded = expand(&toks, macros, depth + 1, inc_base.as_deref())?;
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
) -> Result<usize, ParseError> {
    while matches!(toks.get(i).map(|s| &s.tok), Some(Token::Newline)) {
        out.push(toks[i].clone());
        i += 1;
    }
    if !matches!(toks.get(i).map(|s| &s.tok), Some(Token::LeftBrace)) {
        return Ok(i); // no body; the parser will report the problem
    }
    let mut depth = 0i32;
    while let Some(s) = toks.get(i) {
        match &s.tok {
            Token::LeftBrace => depth += 1,
            Token::RightBrace => {
                out.push(s.clone());
                i += 1;
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
                continue;
            }
            _ => {}
        }
        out.push(s.clone());
        i += 1;
    }
    Err(ParseError {
        msg: "unterminated `{` body".into(),
        line: 0,
        col: 0,
    })
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
}

impl Parser {
    fn new(toks: Vec<Spanned>) -> Self {
        Parser { toks, idx: 0 }
    }

    // ---- cursor helpers ----------------------------------------------------

    fn cur(&self) -> &Token {
        &self.toks[self.idx].tok
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
            self.err(format!("expected {t:?}, found {:?}", self.cur()))
        }
    }
    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        let s = &self.toks[self.idx];
        Err(ParseError {
            msg: msg.into(),
            line: s.line,
            col: s.col,
        })
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
            base_dir: None,
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
            return Ok(Stmt::Animate(self.parse_animate()?));
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
                // `command`/`sh` emit raw backend text or run a shell; neither
                // affects SVG geometry, so consume the rest of the line.
                Kw::Command | Kw::Sh => {
                    self.bump();
                    while !self.at(&Token::Newline) && !self.at(&Token::Eof) {
                        self.bump();
                    }
                    return Ok(Stmt::Print(PrintItem::Str(StringExpr::Lit(String::new()))));
                }
                Kw::Copy => {
                    return self.err("`copy` is not supported yet (planned milestone)");
                }
                _ => {}
            }
        }

        // `{ … }` grouping
        if self.eat(&Token::LeftBrace) {
            let stmts = self.parse_elementlist(&[Token::RightBrace])?;
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
                let pos = self.parse_position()?;
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
            Token::Name(s) => s,
            other => return self.err(format!("expected loop variable, found {other:?}")),
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
        let effect = self.parse_stringexpr()?;
        let mut duration = None;
        let mut timing = Timing::Sequential;
        let mut delay = None;
        loop {
            if self.eat_kw(Kw::For) {
                duration = Some(self.parse_expr()?);
            } else if self.eat_kw(Kw::At) {
                timing = Timing::At(self.parse_expr()?);
            } else if self.eat_kw(Kw::After) {
                timing = Timing::After(self.parse_place()?);
            } else if self.eat_kw(Kw::Delay) {
                delay = Some(self.parse_expr()?);
            } else {
                break;
            }
        }
        Ok(Animate {
            target,
            effect,
            duration,
            timing,
            delay,
        })
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
        )
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
            let e = self.parse_expr()?;
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
                    let e = self.parse_expr()?;
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
            Token::Eq | Token::ColonEq => AssignOp::Set,
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

    // ---- objects & attributes ---------------------------------------------

    fn parse_object(&mut self) -> PResult<Object> {
        let mut attrs = Vec::new();
        // a bare string expression (literal, `$arg`, sprintf, concatenation)
        // places a text-only object.
        if self.at_string_start() {
            attrs.push(Attr::Text(self.parse_stringexpr()?));
            while let Some(a) = self.parse_attr()? {
                attrs.push(a);
            }
            return Ok(Object {
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
            other => return self.err(format!("expected an object, found {other:?}")),
        };
        while let Some(a) = self.parse_attr()? {
            attrs.push(a);
        }
        Ok(Object { kind, attrs })
    }

    fn parse_attr(&mut self) -> PResult<Option<Attr>> {
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
            Token::Kw(Kw::Scaled) => {
                self.bump();
                Attr::Dim(DimKind::Scaled, self.parse_expr()?)
            }
            Token::Dir(d) => {
                self.bump();
                Attr::Direction(d, self.opt_expr()?)
            }
            Token::LineType(lt) => {
                self.bump();
                Attr::LineStyle(lt, self.opt_expr()?)
            }
            Token::Kw(Kw::Chop) => {
                self.bump();
                Attr::Chop(self.opt_expr()?)
            }
            Token::Kw(Kw::Fill) => {
                self.bump();
                Attr::Fill(self.opt_expr()?)
            }
            Token::Arrow(a) => {
                self.bump();
                Attr::Arrowhead(a, self.opt_expr()?)
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
                // a colour may be a quoted string or a bareword name (e.g.
                // `shaded Custom`, `outlined red`).
                let s = match self.cur().clone() {
                    Token::Name(n) | Token::Label(n) => {
                        self.bump();
                        StringExpr::Lit(n)
                    }
                    _ => self.parse_stringexpr()?,
                };
                Attr::Color(c, s)
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
            | Token::Kw(Kw::Rand) => Attr::Dist(self.parse_expr()?),
            _ if self.place_is_scalar_ahead() => Attr::Dist(self.parse_expr()?),
            _ => return Ok(None),
        };
        Ok(Some(attr))
    }

    fn expect_kw(&mut self, k: Kw) -> PResult<()> {
        if self.eat_kw(k) {
            Ok(())
        } else {
            self.err(format!("expected `{k:?}`, found {:?}", self.cur()))
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
        while self.at(&Token::Plus) && self.string_after_plus() {
            self.bump();
            let rhs = self.parse_string_atom()?;
            e = StringExpr::Concat(Box::new(e), Box::new(rhs));
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

    /// Positions support vector arithmetic; `+`/`-` are the lowest precedence.
    fn parse_position(&mut self) -> PResult<Position> {
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
        // `corner [of] placename`
        if let Token::Corner(c) = self.cur() {
            let c = *c;
            self.bump();
            self.eat_kw(Kw::Of);
            let inner = self.parse_place()?;
            return Ok(Place::CornerOf(c, Box::new(inner)));
        }

        let mut place = self.parse_place_base()?;

        // trailing `.corner`, `.label`, `.nth primobj`
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
                self.bump();
                let subscript = if self.eat(&Token::LeftBrack) {
                    let e = self.parse_expr()?;
                    self.expect(&Token::RightBrack)?;
                    Some(Box::new(e))
                } else {
                    None
                };
                Ok(Place::Name { name, subscript })
            }
            Token::Kw(Kw::Last) | Token::Float(_) | Token::LeftBrace | Token::LeftQuote => {
                let count = self.parse_nth()?;
                let obj = self.parse_primobj()?;
                Ok(Place::Nth { count, obj })
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

    fn parse_primobj(&mut self) -> PResult<PrimObj> {
        match self.cur().clone() {
            Token::Prim(p) => {
                self.bump();
                Ok(PrimObj::Prim(p))
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
        self.parse_or()
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut e = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let r = self.parse_and()?;
            e = Expr::Bin(BinOp::Or, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut e = self.parse_cmp()?;
        while self.eat(&Token::AndAnd) {
            let r = self.parse_cmp()?;
            e = Expr::Bin(BinOp::And, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_cmp(&mut self) -> PResult<Expr> {
        let mut e = self.parse_add()?;
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
            let r = self.parse_add()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_add(&mut self) -> PResult<Expr> {
        let mut e = self.parse_mul()?;
        loop {
            let op = match self.cur() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let r = self.parse_mul()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_mul(&mut self) -> PResult<Expr> {
        let mut e = self.parse_unary()?;
        loop {
            let op = match self.cur() {
                Token::Mult => BinOp::Mul,
                Token::Div => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            let r = self.parse_unary()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let op = match self.cur() {
            Token::Minus => Some(UnOp::Neg),
            Token::Plus => Some(UnOp::Pos),
            Token::Not => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let e = self.parse_unary()?;
            Ok(Expr::Unary(op, Box::new(e)))
        } else {
            self.parse_pow()
        }
    }

    fn parse_pow(&mut self) -> PResult<Expr> {
        let base = self.parse_primary()?;
        if self.eat(&Token::Caret) {
            let exp = self.parse_unary()?; // right-associative
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
                    let e = self.parse_expr()?;
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
                        let e = self.parse_expr()?;
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
    fn unsupported_control_is_clear() {
        // `copy "file"` with no filesystem context reports a clear file error
        let e = parse("copy \"x\"").unwrap_err();
        assert!(e.msg.contains("copy") && e.msg.contains("file"));
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
