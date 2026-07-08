//! Token set for the pic language, mirroring dpic's `dpic.toks`.
//!
//! Open-ended lexical classes (numbers, strings, identifiers, labels, macro
//! args) carry their payload; the large but finite keyword vocabulary is folded
//! into grouped enums ([`Kw`], [`Corner`], [`Param`], [`Func1`], [`Func2`],
//! [`LineType`], [`TextPos`], [`Arrow`], [`Dir`], [`Prim`], [`Color`],
//! [`EnvVar`]) so the parser can branch on them directly.

/// A single lexical token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ---- literals & open classes -------------------------------------------
    /// Numeric literal (pic has only floating-point numbers).
    Float(f64),
    /// `"…"` string literal (quotes stripped, escapes resolved).
    Str(String),
    /// Lower-initial identifier / variable name.
    Name(String),
    /// Upper-initial place label (e.g. `Start`).
    Label(String),
    /// `$n` macro argument reference.
    Arg(u32),
    /// `$+` — the number of arguments passed to the current macro.
    ArgCount,
    /// A literal `$` not introducing a macro argument (e.g. `$f$` LaTeX text
    /// passed unquoted as a macro argument). Carried as text by the macro layer.
    Dollar,
    /// A literal `\` that is not a line continuation (e.g. `\beta` LaTeX text
    /// passed unquoted as a macro argument). Carried as text by the macro layer.
    Backslash,

    // ---- structural --------------------------------------------------------
    /// End of statement: newline or `;`.
    Newline,
    /// `.PS` picture start (optionally followed by width/height terms).
    DotPS,
    /// `.PE` picture end.
    DotPE,
    /// End of input.
    Eof,

    // ---- punctuation & operators ------------------------------------------
    Lt,         // <
    Lparen,     // (
    Rparen,     // )
    Mult,       // *
    Plus,       // +
    Minus,      // -
    Div,        // /
    Percent,    // %
    Caret,      // ^
    Not,        // !
    AndAnd,     // &&
    OrOr,       // ||
    Ampersand,  // &
    Comma,      // ,
    Colon,      // :
    LeftBrack,  // [
    RightBrack, // ]
    LeftBrace,  // {
    RightBrace, // }
    Dot,        // .
    Block,      // []  (empty-block reference)
    LeftQuote,  // `
    RightQuote, // '
    Eq,         // =
    ColonEq,    // :=
    PlusEq,     // +=
    MinusEq,    // -=
    MultEq,     // *=
    DivEq,      // /=
    RemEq,      // %=
    EqEq,       // ==
    Neq,        // !=
    Ge,         // >=
    Le,         // <=
    Gt,         // >
    DotX,       // .x
    DotY,       // .y

    // ---- grouped keyword classes ------------------------------------------
    Kw(Kw),
    Corner(Corner),
    Param(Param),
    Func1(Func1),
    Func2(Func2),
    LineType(LineType),
    TextPos(TextPos),
    Arrow(Arrow),
    Dir(Dir),
    Prim(Prim),
    Color(Color),
    EnvVar(EnvVar),
}

/// General keywords: attributes, ordinals, control words, commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    Ht,
    Wid,
    Rad,
    Diam,
    Thick,
    Thin,
    Scaled,
    From,
    To,
    At,
    With,
    By,
    Then,
    Continue,
    Chop,
    Same,
    Cw,
    Ccw,
    Of,
    The,
    Way,
    Between,
    And,
    Here,
    Last,
    Fill,
    Nth, // ordinal marker: st / nd / rd / th
    Print,
    Copy,
    Reset,
    Exec,
    Sh,
    Command,
    Define,
    Undef,
    Rand,
    If,
    Else,
    For,
    Do,
    Sprintf,
    // rpic animation extension (not in classic pic)
    Animate,
    After,
    Delay,
    Repeat,
    Yoyo,
    Ease,
    Along,
    Stagger,
    Out,
    Scroll,
    Into,
}

/// Compass / named corners of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    N,
    S,
    E,
    W,
    Ne,
    Se,
    Nw,
    Sw,
    Start,
    End,
    Center,
}

/// Dotted attribute accessors: `.ht .wid .rad .diam .thick .len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Height,
    Width,
    Radius,
    Diameter,
    Thickness,
    Length,
}

/// Single-argument math functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func1 {
    Abs,
    Acos,
    Asin,
    Cos,
    Exp,
    Expe,
    Int,
    Log,
    Loge,
    Sign,
    Sin,
    Sqrt,
    Tan,
    Floor,
}

/// Two-argument math functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func2 {
    Atan2,
    Max,
    Min,
    Pmod,
}

/// Line style attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    Solid,
    Dotted,
    Dashed,
    Invis,
}

/// Text justification attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPos {
    Center,
    Ljust,
    Rjust,
    Above,
    Below,
}

/// Arrowhead specifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    Left,   // <-
    Right,  // ->
    Double, // <->
}

/// Direction-of-motion words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Right,
    Left,
}

/// Drawable primitive objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Box,
    Circle,
    Ellipse,
    Arc,
    Line,
    Arrow,
    Move,
    Spline,
}

/// Color / outline / shade attribute keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Colored,
    Outlined,
    Shaded,
}

/// Built-in environment variables (default dimensions & globals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvVar {
    Arcrad,
    Arrowht,
    Arrowwid,
    Boxht,
    Boxrad,
    Boxwid,
    Circlerad,
    Dashwid,
    Ellipseht,
    Ellipsewid,
    Lineht,
    Linewid,
    Moveht,
    Movewid,
    Textht,
    Textoffset,
    Textwid,
    Arrowhead,
    Fillval,
    Linethick,
    Maxpsht,
    Maxpswid,
    Scale,
    Margin,
    Topmargin,
    Rightmargin,
    Bottommargin,
    Leftmargin,
    Texlabels,
    Dotrad,
}
