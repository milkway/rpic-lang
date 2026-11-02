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
///
/// Adding a variant? `name()` is an exhaustive match (the compiler reminds
/// you), and the variant must ALSO join [`EnvVar::ALL`] — the
/// `all_is_exhaustive` test cross-checks the two.
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
    Maxanimrepeat,
    Maxanimseconds,
}

impl EnvVar {
    /// Every built-in, for whole-environment exports (the scene's `vars`).
    pub const ALL: [EnvVar; 32] = [
        EnvVar::Arcrad,
        EnvVar::Arrowht,
        EnvVar::Arrowwid,
        EnvVar::Boxht,
        EnvVar::Boxrad,
        EnvVar::Boxwid,
        EnvVar::Circlerad,
        EnvVar::Dashwid,
        EnvVar::Ellipseht,
        EnvVar::Ellipsewid,
        EnvVar::Lineht,
        EnvVar::Linewid,
        EnvVar::Moveht,
        EnvVar::Movewid,
        EnvVar::Textht,
        EnvVar::Textoffset,
        EnvVar::Textwid,
        EnvVar::Arrowhead,
        EnvVar::Fillval,
        EnvVar::Linethick,
        EnvVar::Maxpsht,
        EnvVar::Maxpswid,
        EnvVar::Scale,
        EnvVar::Margin,
        EnvVar::Topmargin,
        EnvVar::Rightmargin,
        EnvVar::Bottommargin,
        EnvVar::Leftmargin,
        EnvVar::Texlabels,
        EnvVar::Dotrad,
        EnvVar::Maxanimrepeat,
        EnvVar::Maxanimseconds,
    ];

    /// The pic-source spelling of this built-in (the lexer's keyword).
    pub fn name(self) -> &'static str {
        match self {
            EnvVar::Arcrad => "arcrad",
            EnvVar::Arrowht => "arrowht",
            EnvVar::Arrowwid => "arrowwid",
            EnvVar::Boxht => "boxht",
            EnvVar::Boxrad => "boxrad",
            EnvVar::Boxwid => "boxwid",
            EnvVar::Circlerad => "circlerad",
            EnvVar::Dashwid => "dashwid",
            EnvVar::Ellipseht => "ellipseht",
            EnvVar::Ellipsewid => "ellipsewid",
            EnvVar::Lineht => "lineht",
            EnvVar::Linewid => "linewid",
            EnvVar::Moveht => "moveht",
            EnvVar::Movewid => "movewid",
            EnvVar::Textht => "textht",
            EnvVar::Textoffset => "textoffset",
            EnvVar::Textwid => "textwid",
            EnvVar::Arrowhead => "arrowhead",
            EnvVar::Fillval => "fillval",
            EnvVar::Linethick => "linethick",
            EnvVar::Maxpsht => "maxpsht",
            EnvVar::Maxpswid => "maxpswid",
            EnvVar::Scale => "scale",
            EnvVar::Margin => "margin",
            EnvVar::Topmargin => "topmargin",
            EnvVar::Rightmargin => "rightmargin",
            EnvVar::Bottommargin => "bottommargin",
            EnvVar::Leftmargin => "leftmargin",
            EnvVar::Texlabels => "texlabels",
            EnvVar::Dotrad => "dotrad",
            EnvVar::Maxanimrepeat => "maxanimrepeat",
            EnvVar::Maxanimseconds => "maxanimseconds",
        }
    }
}

#[cfg(test)]
mod envvar_tests {
    use super::EnvVar;

    #[test]
    fn all_is_exhaustive() {
        // `name()` is an exhaustive match; ALL is a hand-kept list. Walking
        // discriminants until one is missing from ALL (or names collide)
        // catches a new variant that forgot to join ALL.
        let mut names = std::collections::BTreeSet::new();
        for v in EnvVar::ALL {
            assert!(names.insert(v.name()), "duplicate in ALL: {}", v.name());
        }
        // every u8 discriminant below the count maps into ALL
        assert_eq!(EnvVar::ALL.len(), names.len(), "ALL must be duplicate-free");
        let max_disc = EnvVar::ALL.iter().map(|&v| v as u8).max().unwrap();
        assert_eq!(
            usize::from(max_disc) + 1,
            EnvVar::ALL.len(),
            "a new EnvVar variant is missing from EnvVar::ALL"
        );
    }
}
