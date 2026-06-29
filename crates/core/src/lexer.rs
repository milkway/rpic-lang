//! Hand-written lexer for the pic language.
//!
//! Recognises the full dpic token vocabulary (see `token.rs`). It is a simple
//! character scanner — pic sources are small, so we keep the whole input in a
//! `Vec<char>` and index into it. Line/column are tracked for diagnostics.

use crate::token::*;

/// A token together with its source position (1-based line/column).
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub tok: Token,
    pub line: u32,
    pub col: u32,
}

/// A lexing error with location.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

/// Tokenize `src`. The returned vector always ends with [`Token::Eof`].
pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    Lexer::new(src).run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    out: Vec<Spanned>,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            out: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    /// Consume and return the next char, advancing line/column.
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn err<T>(&self, msg: impl Into<String>) -> Result<T, LexError> {
        Err(LexError {
            msg: msg.into(),
            line: self.line,
            col: self.col,
        })
    }

    fn run(mut self) -> Result<Vec<Spanned>, LexError> {
        loop {
            // Skip spaces, tabs, CR, and `#` comments. Backslash-newline is a
            // line continuation (both consumed).
            loop {
                match self.peek() {
                    Some(' ') | Some('\t') | Some('\r') => {
                        self.bump();
                    }
                    Some('#') => {
                        while let Some(c) = self.peek() {
                            if c == '\n' {
                                break;
                            }
                            self.bump();
                        }
                    }
                    Some('\\') => {
                        // continuation: `\` [spaces] newline
                        let save = (self.pos, self.line, self.col);
                        self.bump();
                        while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) {
                            self.bump();
                        }
                        if self.peek() == Some('\n') {
                            self.bump();
                        } else {
                            // not a continuation; restore and let the main
                            // matcher report the stray backslash.
                            self.pos = save.0;
                            self.line = save.1;
                            self.col = save.2;
                            break;
                        }
                    }
                    _ => break,
                }
            }

            let (line, col) = (self.line, self.col);
            let c = match self.peek() {
                None => {
                    self.push(Token::Eof, line, col);
                    return Ok(self.out);
                }
                Some(c) => c,
            };

            let tok = if c == '\n' || c == ';' {
                self.bump();
                Token::Newline
            } else if c == '"' {
                self.lex_string()?
            } else if c == '$' {
                self.lex_arg()?
            } else if c.is_ascii_digit()
                || (c == '.' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()))
            {
                self.lex_number()?
            } else if c == '.' {
                self.lex_dot()?
            } else if c.is_alphabetic() || c == '_' {
                self.lex_word()
            } else {
                match self.lex_operator()? {
                    Some(t) => t,
                    None => continue, // (should not happen)
                }
            };
            self.push(tok, line, col);
        }
    }

    fn push(&mut self, tok: Token, line: u32, col: u32) {
        self.out.push(Spanned { tok, line, col });
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return self.err("unterminated string literal"),
                Some('"') => break,
                Some('\\') => {
                    // Preserve escape sequences (e.g. troff escapes) verbatim;
                    // a backslash-quote does not terminate the string.
                    s.push('\\');
                    match self.bump() {
                        Some(c) => s.push(c),
                        None => return self.err("unterminated string literal"),
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Token::Str(s))
    }

    fn lex_arg(&mut self) -> Result<Token, LexError> {
        self.bump(); // '$'
        let mut n = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                n.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if n.is_empty() {
            return self.err("expected digit after `$`");
        }
        Ok(Token::Arg(n.parse().unwrap()))
    }

    fn lex_number(&mut self) -> Result<Token, LexError> {
        let mut s = String::new();
        if self.peek() == Some('.') {
            s.push('0'); // ".5" -> "0.5"
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        // fractional part: only consume `.` when a digit follows, so a trailing
        // `.` (e.g. in `box.ne`) stays a separate token.
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()) {
            s.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        // exponent
        if matches!(self.peek(), Some('e') | Some('E'))
            && (self.peek_at(1).is_some_and(|d| d.is_ascii_digit())
                || (matches!(self.peek_at(1), Some('+') | Some('-'))
                    && self.peek_at(2).is_some_and(|d| d.is_ascii_digit())))
        {
            s.push('e');
            self.bump();
            if matches!(self.peek(), Some('+') | Some('-')) {
                s.push(self.bump().unwrap());
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        match s.parse::<f64>() {
            Ok(v) => Ok(Token::Float(v)),
            Err(_) => self.err(format!("invalid number `{s}`")),
        }
    }

    /// Lex a `.`-prefixed token: `.PS/.PE`, compass corners, `.x/.y`, dotted
    /// attribute accessors, or a bare `.` (the placename separator).
    fn lex_dot(&mut self) -> Result<Token, LexError> {
        let save = (self.pos, self.line, self.col);
        self.bump(); // '.'
        // read the following word
        let mut w = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                w.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if let Some(tok) = dot_keyword(&w) {
            Ok(tok)
        } else {
            // Not a dotted keyword: emit a bare `.` and rewind so the word is
            // lexed normally on the next pass.
            self.pos = save.0 + 1;
            self.line = save.1;
            self.col = save.2 + 1;
            Ok(Token::Dot)
        }
    }

    fn lex_word(&mut self) -> Token {
        let mut w = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                w.push(c);
                self.bump();
            } else {
                break;
            }
        }
        word_keyword(&w)
    }

    fn lex_operator(&mut self) -> Result<Option<Token>, LexError> {
        let c = self.bump().unwrap();
        let next = self.peek();
        let tok = match c {
            '(' => Token::Lparen,
            ')' => Token::Rparen,
            ',' => Token::Comma,
            '^' => Token::Caret,
            '{' => Token::LeftBrace,
            '}' => Token::RightBrace,
            ']' => Token::RightBrack,
            '`' => Token::LeftQuote,
            '\'' => Token::RightQuote,
            '[' => {
                if next == Some(']') {
                    self.bump();
                    Token::Block
                } else {
                    Token::LeftBrack
                }
            }
            ':' => self.two('=', Token::ColonEq, Token::Colon),
            '=' => self.two('=', Token::EqEq, Token::Eq),
            '!' => self.two('=', Token::Neq, Token::Not),
            '+' => self.two('=', Token::PlusEq, Token::Plus),
            '*' => self.two('=', Token::MultEq, Token::Mult),
            '/' => self.two('=', Token::DivEq, Token::Div),
            '%' => self.two('=', Token::RemEq, Token::Percent),
            '>' => self.two('=', Token::Ge, Token::Gt),
            '&' => self.two('&', Token::AndAnd, Token::Ampersand),
            '|' => {
                if next == Some('|') {
                    self.bump();
                    Token::OrOr
                } else {
                    return self.err("unexpected `|`");
                }
            }
            '-' => match next {
                Some('>') => {
                    self.bump();
                    Token::Arrow(Arrow::Right)
                }
                Some('=') => {
                    self.bump();
                    Token::MinusEq
                }
                _ => Token::Minus,
            },
            '<' => match next {
                Some('=') => {
                    self.bump();
                    Token::Le
                }
                Some('-') => {
                    self.bump();
                    if self.peek() == Some('>') {
                        self.bump();
                        Token::Arrow(Arrow::Double)
                    } else {
                        Token::Arrow(Arrow::Left)
                    }
                }
                _ => Token::Lt,
            },
            other => return self.err(format!("unexpected character `{other}`")),
        };
        Ok(Some(tok))
    }

    /// If the next char is `c`, consume it and return `yes`; otherwise `no`.
    fn two(&mut self, c: char, yes: Token, no: Token) -> Token {
        if self.peek() == Some(c) {
            self.bump();
            yes
        } else {
            no
        }
    }
}

/// Classify a bare word (after the keyword vocabulary). Unknown words become a
/// [`Token::Label`] (upper-initial) or [`Token::Name`] (otherwise).
fn word_keyword(w: &str) -> Token {
    use Token::*;
    match w {
        // primitives
        "box" => Prim(self::Prim::Box),
        "circle" => Prim(self::Prim::Circle),
        "ellipse" => Prim(self::Prim::Ellipse),
        "arc" => Prim(self::Prim::Arc),
        "line" => Prim(self::Prim::Line),
        "arrow" => Prim(self::Prim::Arrow),
        "move" => Prim(self::Prim::Move),
        "spline" => Prim(self::Prim::Spline),
        // directions
        "up" => Dir(self::Dir::Up),
        "down" => Dir(self::Dir::Down),
        "right" => Dir(self::Dir::Right),
        "left" => Dir(self::Dir::Left),
        // attributes & joiners
        "height" | "ht" => Kw(self::Kw::Ht),
        "width" | "wid" => Kw(self::Kw::Wid),
        "radius" | "rad" => Kw(self::Kw::Rad),
        "diameter" | "diam" => Kw(self::Kw::Diam),
        "thickness" | "thick" => Kw(self::Kw::Thick),
        "scaled" => Kw(self::Kw::Scaled),
        "from" => Kw(self::Kw::From),
        "to" => Kw(self::Kw::To),
        "at" => Kw(self::Kw::At),
        "with" => Kw(self::Kw::With),
        "by" => Kw(self::Kw::By),
        "then" => Kw(self::Kw::Then),
        "cw" => Kw(self::Kw::Cw),
        "ccw" => Kw(self::Kw::Ccw),
        "continue" => Kw(self::Kw::Continue),
        "chop" => Kw(self::Kw::Chop),
        "same" => Kw(self::Kw::Same),
        "of" => Kw(self::Kw::Of),
        "the" => Kw(self::Kw::The),
        "way" => Kw(self::Kw::Way),
        "between" => Kw(self::Kw::Between),
        "and" => Kw(self::Kw::And),
        "last" => Kw(self::Kw::Last),
        "fill" | "filled" => Kw(self::Kw::Fill),
        "st" | "nd" | "rd" | "th" => Kw(self::Kw::Nth),
        "Here" => Kw(self::Kw::Here),
        // bare corner words
        "top" => Corner(self::Corner::N),
        "bottom" => Corner(self::Corner::S),
        "start" => Corner(self::Corner::Start),
        "end" => Corner(self::Corner::End),
        // commands / control
        "print" => Kw(self::Kw::Print),
        "copy" => Kw(self::Kw::Copy),
        "reset" => Kw(self::Kw::Reset),
        "exec" => Kw(self::Kw::Exec),
        "sh" => Kw(self::Kw::Sh),
        "command" => Kw(self::Kw::Command),
        "define" => Kw(self::Kw::Define),
        "undefine" | "undef" => Kw(self::Kw::Undef),
        "rand" => Kw(self::Kw::Rand),
        "if" => Kw(self::Kw::If),
        "else" => Kw(self::Kw::Else),
        "for" => Kw(self::Kw::For),
        "do" => Kw(self::Kw::Do),
        "sprintf" => Kw(self::Kw::Sprintf),
        // rpic animation extension
        "animate" => Kw(self::Kw::Animate),
        "after" => Kw(self::Kw::After),
        "delay" => Kw(self::Kw::Delay),
        // line types
        "solid" => LineType(self::LineType::Solid),
        "dotted" => LineType(self::LineType::Dotted),
        "dashed" => LineType(self::LineType::Dashed),
        "invis" | "invisible" => LineType(self::LineType::Invis),
        // color / outline / shade
        "color" | "colour" | "colored" | "coloured" => Color(self::Color::Colored),
        "outline" | "outlined" => Color(self::Color::Outlined),
        "shade" | "shaded" => Color(self::Color::Shaded),
        // text position
        "center" | "centre" => TextPos(self::TextPos::Center),
        "ljust" => TextPos(self::TextPos::Ljust),
        "rjust" => TextPos(self::TextPos::Rjust),
        "above" => TextPos(self::TextPos::Above),
        "below" => TextPos(self::TextPos::Below),
        // one-arg functions
        "abs" => Func1(self::Func1::Abs),
        "acos" => Func1(self::Func1::Acos),
        "asin" => Func1(self::Func1::Asin),
        "cos" => Func1(self::Func1::Cos),
        "exp" => Func1(self::Func1::Exp),
        "expe" => Func1(self::Func1::Expe),
        "int" => Func1(self::Func1::Int),
        "log" => Func1(self::Func1::Log),
        "loge" => Func1(self::Func1::Loge),
        "sign" => Func1(self::Func1::Sign),
        "sin" => Func1(self::Func1::Sin),
        "sqrt" => Func1(self::Func1::Sqrt),
        "tan" => Func1(self::Func1::Tan),
        "floor" => Func1(self::Func1::Floor),
        // two-arg functions
        "atan2" => Func2(self::Func2::Atan2),
        "max" => Func2(self::Func2::Max),
        "min" => Func2(self::Func2::Min),
        "pmod" => Func2(self::Func2::Pmod),
        // environment variables
        "arcrad" => EnvVar(self::EnvVar::Arcrad),
        "arrowht" => EnvVar(self::EnvVar::Arrowht),
        "arrowwid" => EnvVar(self::EnvVar::Arrowwid),
        "boxht" => EnvVar(self::EnvVar::Boxht),
        "boxrad" => EnvVar(self::EnvVar::Boxrad),
        "boxwid" => EnvVar(self::EnvVar::Boxwid),
        "circlerad" => EnvVar(self::EnvVar::Circlerad),
        "dashwid" => EnvVar(self::EnvVar::Dashwid),
        "ellipseht" => EnvVar(self::EnvVar::Ellipseht),
        "ellipsewid" => EnvVar(self::EnvVar::Ellipsewid),
        "lineht" => EnvVar(self::EnvVar::Lineht),
        "linewid" => EnvVar(self::EnvVar::Linewid),
        "moveht" => EnvVar(self::EnvVar::Moveht),
        "movewid" => EnvVar(self::EnvVar::Movewid),
        "textht" => EnvVar(self::EnvVar::Textht),
        "textoffset" => EnvVar(self::EnvVar::Textoffset),
        "textwid" => EnvVar(self::EnvVar::Textwid),
        "arrowhead" => EnvVar(self::EnvVar::Arrowhead),
        "fillval" => EnvVar(self::EnvVar::Fillval),
        "linethick" => EnvVar(self::EnvVar::Linethick),
        "maxpsht" => EnvVar(self::EnvVar::Maxpsht),
        "maxpswid" => EnvVar(self::EnvVar::Maxpswid),
        "scale" => EnvVar(self::EnvVar::Scale),
        // identifier / label
        _ => {
            if w.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                Label(w.to_string())
            } else {
                Name(w.to_string())
            }
        }
    }
}

/// Classify the word following a `.`. Returns `None` if it is not a dotted
/// keyword (so the caller emits a bare `.`).
fn dot_keyword(w: &str) -> Option<Token> {
    use Token::*;
    let t = match w {
        "PS" => DotPS,
        "PE" => DotPE,
        "x" => DotX,
        "y" => DotY,
        // compass corners
        "ne" => Corner(self::Corner::Ne),
        "se" => Corner(self::Corner::Se),
        "nw" => Corner(self::Corner::Nw),
        "sw" => Corner(self::Corner::Sw),
        "n" | "t" | "top" | "north" => Corner(self::Corner::N),
        "s" | "b" | "bot" | "bottom" | "south" => Corner(self::Corner::S),
        "e" | "r" | "east" | "right" => Corner(self::Corner::E),
        "w" | "l" | "west" | "left" => Corner(self::Corner::W),
        "start" => Corner(self::Corner::Start),
        "end" => Corner(self::Corner::End),
        "c" | "center" | "centre" => Corner(self::Corner::Center),
        // dotted attribute accessors
        "ht" | "height" => Param(self::Param::Height),
        "wid" | "width" => Param(self::Param::Width),
        "rad" | "radius" => Param(self::Param::Radius),
        "diam" | "diameter" => Param(self::Param::Diameter),
        "thick" | "thickness" => Param(self::Param::Thickness),
        "len" | "length" => Param(self::Param::Length),
        _ => return None,
    };
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Token> {
        lex(src).unwrap().into_iter().map(|s| s.tok).collect()
    }

    #[test]
    fn basic_box() {
        assert_eq!(
            toks("box \"hi\""),
            vec![Token::Prim(Prim::Box), Token::Str("hi".into()), Token::Eof]
        );
    }

    #[test]
    fn label_vs_name() {
        assert_eq!(
            toks("Start: boxwid"),
            vec![
                Token::Label("Start".into()),
                Token::Colon,
                Token::EnvVar(EnvVar::Boxwid),
                Token::Eof
            ]
        );
        // a lower-initial non-keyword is a Name
        assert_eq!(toks("myvar"), vec![Token::Name("myvar".into()), Token::Eof]);
    }

    #[test]
    fn numbers() {
        assert_eq!(toks("0.5"), vec![Token::Float(0.5), Token::Eof]);
        assert_eq!(toks(".25"), vec![Token::Float(0.25), Token::Eof]);
        assert_eq!(toks("1e3"), vec![Token::Float(1000.0), Token::Eof]);
        assert_eq!(toks("2.5e-1"), vec![Token::Float(0.25), Token::Eof]);
    }

    #[test]
    fn operators_and_arrows() {
        assert_eq!(
            toks("a := b <= c -> d <- e <-> f"),
            vec![
                Token::Name("a".into()),
                Token::ColonEq,
                Token::Name("b".into()),
                Token::Le,
                Token::Name("c".into()),
                Token::Arrow(Arrow::Right),
                Token::Name("d".into()),
                Token::Arrow(Arrow::Left),
                Token::Name("e".into()),
                Token::Arrow(Arrow::Double),
                Token::Name("f".into()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn minus_is_not_arrow_without_gt() {
        assert_eq!(
            toks("2-3"),
            vec![
                Token::Float(2.0),
                Token::Minus,
                Token::Float(3.0),
                Token::Eof
            ]
        );
    }

    #[test]
    fn compass_and_params() {
        assert_eq!(
            toks("last box.ne A.ht .center"),
            vec![
                Token::Kw(Kw::Last),
                Token::Prim(Prim::Box),
                Token::Corner(Corner::Ne),
                Token::Label("A".into()),
                Token::Param(Param::Height),
                Token::Corner(Corner::Center),
                Token::Eof
            ]
        );
    }

    #[test]
    fn dot_then_label() {
        // `.` not followed by a dotted keyword is a bare Dot, then the word.
        assert_eq!(
            toks("B.A"),
            vec![
                Token::Label("B".into()),
                Token::Dot,
                Token::Label("A".into()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn statement_separators_and_comments() {
        assert_eq!(
            toks("box; circle # a comment\narc"),
            vec![
                Token::Prim(Prim::Box),
                Token::Newline,
                Token::Prim(Prim::Circle),
                Token::Newline,
                Token::Prim(Prim::Arc),
                Token::Eof
            ]
        );
    }

    #[test]
    fn line_continuation() {
        assert_eq!(
            toks("box \\\n  wid 2"),
            vec![
                Token::Prim(Prim::Box),
                Token::Kw(Kw::Wid),
                Token::Float(2.0),
                Token::Eof
            ]
        );
    }

    #[test]
    fn ps_pe_and_block() {
        assert_eq!(
            toks(".PS\nbox\n.PE"),
            vec![
                Token::DotPS,
                Token::Newline,
                Token::Prim(Prim::Box),
                Token::Newline,
                Token::DotPE,
                Token::Eof
            ]
        );
        assert_eq!(toks("[]"), vec![Token::Block, Token::Eof]);
        assert_eq!(
            toks("[ box ]"),
            vec![
                Token::LeftBrack,
                Token::Prim(Prim::Box),
                Token::RightBrack,
                Token::Eof
            ]
        );
    }

    #[test]
    fn macro_arg() {
        assert_eq!(
            toks("box $1"),
            vec![Token::Prim(Prim::Box), Token::Arg(1), Token::Eof]
        );
    }

    #[test]
    fn func_and_envvar() {
        assert_eq!(
            toks("sqrt(2) atan2 scale"),
            vec![
                Token::Func1(Func1::Sqrt),
                Token::Lparen,
                Token::Float(2.0),
                Token::Rparen,
                Token::Func2(Func2::Atan2),
                Token::EnvVar(EnvVar::Scale),
                Token::Eof
            ]
        );
    }
}
