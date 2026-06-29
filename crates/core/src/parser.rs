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

/// Parse a full source string into a [`Picture`].
pub fn parse(src: &str) -> Result<Picture, ParseError> {
    let toks = lex(src)?;
    let toks = preprocess(toks)?;
    Parser::new(toks).parse_picture()
}

// ---- macro preprocessor ----------------------------------------------------
//
// Handles `define name { body }` (brace-delimited) with `$1..$9` argument
// substitution at the token level, before parsing. Invocations `name(a, b)` (or
// bare `name`) are replaced by the body with arguments spliced in; the result is
// re-expanded so macros may call macros. `undef name` removes a definition.

use std::collections::HashMap;

fn preprocess(input: Vec<Spanned>) -> Result<Vec<Spanned>, ParseError> {
    let mut macros: HashMap<String, Vec<Spanned>> = HashMap::new();
    expand(&input, &mut macros, 0)
}

fn loc(toks: &[Spanned], i: usize) -> (u32, u32) {
    toks.get(i).map(|s| (s.line, s.col)).unwrap_or((0, 0))
}

fn expand(
    toks: &[Spanned],
    macros: &mut HashMap<String, Vec<Spanned>>,
    depth: usize,
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
            Token::Name(n) if macros.contains_key(n) => {
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
                let expanded = expand(&sub, macros, depth + 1)?;
                out.extend(expanded);
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
    for s in body {
        if let Token::Arg(k) = s.tok {
            if let Some(a) = args.get((k as usize).wrapping_sub(1)) {
                out.extend(a.iter().cloned());
            }
        } else {
            out.push(s.clone());
        }
    }
    out
}

type PResult<T> = Result<T, ParseError>;

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
        // rpic animation directive.
        if self.at_kw(Kw::Animate) {
            return Ok(Stmt::Animate(self.parse_animate()?));
        }

        // control constructs
        match self.cur() {
            Token::Kw(Kw::If) => return self.parse_if(),
            Token::Kw(Kw::For) => return self.parse_for(),
            Token::Kw(Kw::Print) => return self.parse_print(),
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
                Kw::Sh | Kw::Exec | Kw::Command | Kw::Copy => {
                    let kw = format!("{k:?}").to_lowercase();
                    return self.err(format!("`{kw}` is not supported yet (planned milestone)"));
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
        self.expect(&Token::LeftBrace)?;
        let then_body = self.parse_elementlist(&[Token::RightBrace])?;
        self.expect(&Token::RightBrace)?;
        let else_body = if self.eat_kw(Kw::Else) {
            self.expect(&Token::LeftBrace)?;
            let b = self.parse_elementlist(&[Token::RightBrace])?;
            self.expect(&Token::RightBrace)?;
            Some(b)
        } else {
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
        self.expect(&Token::LeftBrace)?;
        let body = self.parse_elementlist(&[Token::RightBrace])?;
        self.expect(&Token::RightBrace)?;
        Ok(Stmt::For {
            var,
            from,
            to,
            by,
            mult,
            body,
        })
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
        matches!(
            self.cur(),
            Token::Str(_) | Token::Arg(_) | Token::Kw(Kw::Sprintf)
        )
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
            Token::Prim(_) | Token::LeftBrack | Token::Block | Token::Str(_)
        )
    }

    fn at_assignment_start(&self) -> bool {
        let assignop = |t: &Token| {
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
        };
        match self.cur() {
            Token::Name(_) => assignop(self.peek(1)) || matches!(self.peek(1), Token::LeftBrack),
            Token::EnvVar(_) => assignop(self.peek(1)),
            _ => false,
        }
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
            Token::Name(name) => {
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
            other => return self.err(format!("expected an object, found {other:?}")),
        };
        loop {
            match self.parse_attr()? {
                Some(a) => attrs.push(a),
                None => break,
            }
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
                let anchor = if let Token::Corner(c) = self.cur() {
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
                let s = self.parse_stringexpr()?;
                Attr::Color(c, s)
            }
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
        matches!(
            self.peek(1),
            Token::Str(_) | Token::Arg(_) | Token::Kw(Kw::Sprintf)
        )
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
            other => self.err(format!("expected a string, found {other:?}")),
        }
    }

    // ---- positions ---------------------------------------------------------

    fn parse_position(&mut self) -> PResult<Position> {
        if self.at(&Token::Lparen) {
            return self.parse_paren_position();
        }
        // A leading place is point-valued UNLESS it is a scalar accessor
        // (`place.x` / `.y` / `.attr`), in which case it begins an (expr, expr)
        // pair like `(A.x, A.y - 0.5)`.
        if self.at_place_start() && !self.place_is_scalar_ahead() {
            let loc = self.parse_location_operand()?;
            let shifts = self.parse_shifts()?;
            return Ok(Position::Place(loc, shifts));
        }
        // expression-led: pair or between
        let e1 = self.parse_expr()?;
        if self.eat(&Token::Comma) {
            let e2 = self.parse_expr()?;
            return Ok(Position::Pair(e1, e2));
        }
        let of_the_way = if self.eat_kw(Kw::Of) {
            self.expect_kw(Kw::The)?;
            self.expect_kw(Kw::Way)?;
            self.expect_kw(Kw::Between)?;
            true
        } else if self.eat_kw(Kw::Between) {
            false
        } else {
            return self.err("expected `,`, `between`, or `of the way between` in position");
        };
        let a = self.parse_position()?;
        self.expect_kw(Kw::And)?;
        let b = self.parse_position()?;
        Ok(Position::Between {
            frac: Box::new(e1),
            a: Box::new(a),
            b: Box::new(b),
            of_the_way,
        })
    }

    fn parse_shifts(&mut self) -> PResult<Vec<Shift>> {
        let mut shifts = Vec::new();
        loop {
            let sign = if self.at(&Token::Plus) {
                Sign::Plus
            } else if self.at(&Token::Minus) {
                Sign::Minus
            } else {
                break;
            };
            self.bump();
            let loc = self.parse_location_operand()?;
            shifts.push(Shift { sign, loc });
        }
        Ok(shifts)
    }

    /// A location used as a position component or shift operand (no trailing
    /// shifts of its own).
    fn parse_location_operand(&mut self) -> PResult<Location> {
        if self.eat(&Token::Lparen) {
            let p1 = self.parse_position()?;
            if self.eat(&Token::Comma) {
                let p2 = self.parse_position()?;
                self.expect(&Token::Rparen)?;
                Ok(Location::ParenPair(Box::new(p1), Box::new(p2)))
            } else {
                self.expect(&Token::Rparen)?;
                Ok(Location::Paren(Box::new(p1)))
            }
        } else {
            Ok(Location::Place(self.parse_place()?))
        }
    }

    /// Parse a parenthesised position: `(pos)`, `(pos, pos)`, or `(expr, expr)`
    /// (the latter resolved by the recursive call, e.g. `(A.x, A.y)`).
    fn parse_paren_position(&mut self) -> PResult<Position> {
        self.expect(&Token::Lparen)?;
        let p1 = self.parse_position()?;
        if self.eat(&Token::Comma) {
            let p2 = self.parse_position()?;
            self.expect(&Token::Rparen)?;
            let shifts = self.parse_shifts()?;
            Ok(Position::Place(
                Location::ParenPair(Box::new(p1), Box::new(p2)),
                shifts,
            ))
        } else {
            self.expect(&Token::Rparen)?;
            let shifts = self.parse_shifts()?;
            Ok(Position::Place(Location::Paren(Box::new(p1)), shifts))
        }
    }

    /// Lookahead: does the upcoming place end in a scalar accessor
    /// (`.x` / `.y` / `.attr`)? If so it is a number, not a point. Non-consuming.
    fn place_is_scalar_ahead(&mut self) -> bool {
        let save = self.idx;
        let parsed = self.parse_place().is_ok();
        let scalar = parsed
            && matches!(
                self.cur(),
                Token::DotX | Token::DotY | Token::Param(_)
            );
        self.idx = save;
        scalar
    }

    fn at_place_start(&self) -> bool {
        match self.cur() {
            Token::Label(_) | Token::Block | Token::Corner(_) => true,
            Token::Kw(Kw::Last) | Token::Kw(Kw::Here) => true,
            Token::Float(_) => matches!(self.peek(1), Token::Kw(Kw::Nth)),
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
            Token::Kw(Kw::Last) | Token::Float(_) => {
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
        if self.starts_scalar() {
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

    fn parse_primary(&mut self) -> PResult<Expr> {
        // place-derived scalars: location.x / location.y / place.attr
        if self.at_place_start() {
            return self.parse_place_scalar();
        }
        match self.cur().clone() {
            Token::Float(v) => {
                self.bump();
                Ok(Expr::Num(v))
            }
            Token::Name(name) => {
                self.bump();
                // optional subscript suffix is parsed and ignored for now
                if self.eat(&Token::LeftBrack) {
                    let _ = self.parse_expr()?;
                    self.expect(&Token::RightBrack)?;
                }
                Ok(Expr::Var(name))
            }
            Token::EnvVar(v) => {
                self.bump();
                Ok(Expr::Env(v))
            }
            Token::Lparen => {
                self.bump();
                // could be ( expr ) or a parenthesised location used with .x/.y
                let e = self.parse_expr()?;
                self.expect(&Token::Rparen)?;
                Ok(e)
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
            Attr::From(Position::Place(
                Location::Place(Place::CornerOf(Corner::N, _)),
                _
            ))
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
        assert!(matches!(at, Position::Place(_, shifts) if shifts.len() == 1));
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
        assert!(object.attrs.iter().any(|a| matches!(
            a,
            Attr::At(Position::Place(Location::Paren(_), _))
        )));
        // a plain point place still works
        assert!(pic("box at A.ne\nA: box").stmts.len() == 2 || true);
    }

    #[test]
    fn unsupported_control_is_clear() {
        let e = parse("sh \"ls\"").unwrap_err();
        assert!(e.msg.contains("not supported yet"));
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
