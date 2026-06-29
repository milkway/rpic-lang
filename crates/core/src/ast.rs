//! Abstract syntax tree for the pic language.
//!
//! Mirrors the structure of dpic's `grammar.txt`. The current parser populates
//! the drawing core (pictures, primitives + attributes, positions, expressions,
//! blocks, assignments). Control constructs (`if`/`for`/`define`/`print`/…) have
//! AST slots reserved but are wired up in a later milestone.

use crate::lexer::Spanned;
use crate::token::{
    Arrow, Color, Corner, Dir, EnvVar, Func1, Func2, LineType, Param, Prim, TextPos,
};

/// Macro definitions (name → body tokens), carried from parse to eval so that
/// `if`/`for` bodies can be expanded lazily along the executed path.
pub type Macros = std::collections::HashMap<String, Vec<Spanned>>;

/// A deferred block of statements, kept as raw tokens until the branch/iteration
/// that contains it actually runs (so dead branches and recursive macro calls
/// are never parsed). Parsed on demand by the evaluator.
pub type Body = Vec<Spanned>;

/// A complete picture: optional `.PS <w> <h>` dimensions plus a list of elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: Option<Expr>,
    pub height: Option<Expr>,
    pub stmts: Vec<Stmt>,
    pub macros: Macros,
    /// Directory used to resolve `copy "file"` includes (set by `parse_in_dir`);
    /// `None` when parsing has no filesystem context (WASM/bindings).
    pub base_dir: Option<std::path::PathBuf>,
}

/// A label with an optional `[subscript]` suffix.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub name: String,
    pub subscript: Option<Expr>,
}

/// A top-level element.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// A drawn object, optionally labelled (`Start: box …`).
    Object {
        label: Option<Label>,
        object: Object,
    },
    /// `Label: position` — names a point without drawing.
    Place { label: Label, pos: Position },
    /// One or more comma-separated assignments.
    Assign(Vec<Assignment>),
    /// A bare direction change (`up` / `down` / `left` / `right`).
    Direction(Dir),
    /// `{ … }` grouping block (local scope, no bounding object).
    Group(Vec<Stmt>),
    /// rpic animation directive (extension; see [`Animate`]).
    Animate(Animate),
    /// `if <cond> then { … } [else { … }]`. Bodies are deferred raw tokens.
    If {
        cond: Expr,
        then_body: Body,
        else_body: Option<Body>,
    },
    /// `for v = from to to [by [*] step] do { … }`. Body is deferred raw tokens.
    For {
        var: String,
        from: Expr,
        to: Expr,
        by: Expr,
        /// `by *` multiplies instead of adds.
        mult: bool,
        body: Body,
    },
    /// `print …` (evaluated for diagnostics; no drawing effect).
    Print(PrintItem),
    /// `exec <string>` — evaluate generated pic source in the current state.
    Exec {
        command: StringExpr,
        arg_frame: Option<Vec<Vec<Spanned>>>,
    },
    /// `reset` (all) or `reset a, b, …` — restore environment variables.
    Reset(Vec<EnvVar>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrintItem {
    Str(StringExpr),
    Expr(Expr),
}

/// `animate <target> with "<effect>" [for <dur>] [at <t> | after <ref>] [delay <d>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Animate {
    pub target: Place,
    pub effect: StringExpr,
    pub duration: Option<Expr>,
    pub timing: Timing,
    pub delay: Option<Expr>,
}

/// When an animation starts.
#[derive(Debug, Clone, PartialEq)]
pub enum Timing {
    /// After the previously declared animation ends (default).
    Sequential,
    /// At an absolute time (seconds).
    At(Expr),
    /// After the named object's animation ends.
    After(Place),
}

/// A single assignment `target op value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub target: AssignTarget,
    pub op: AssignOp,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Var(String, Option<Expr>),
    Env(EnvVar),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set, // =  or  :=
    Add, // +=
    Sub, // -=
    Mul, // *=
    Div, // /=
    Rem, // %=
}

/// A drawable object: a base plus a chain of attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub kind: ObjectKind,
    pub attrs: Vec<Attr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKind {
    Primitive(Prim),
    Block(Vec<Stmt>),
    /// `[]` empty-block reference.
    Empty,
    /// A bare quoted string places a text-only object.
    Text,
    /// `continue` — extend the previous line with another segment.
    Continue,
}

/// An object attribute (applied left to right).
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Dim(DimKind, Expr),
    Direction(Dir, Option<Expr>),
    /// A bare distance with no direction word (e.g. `move 1`): advance by this
    /// much along the prevailing direction.
    Dist(Expr),
    LineStyle(LineType, Option<Expr>),
    Chop(Option<Expr>),
    Fill(Option<Expr>),
    Arrowhead(Arrow, Option<Expr>),
    Then,
    Cw,
    Ccw,
    Same,
    Continue,
    Text(StringExpr),
    TextPos(TextPos),
    From(Position),
    To(Position),
    At(Position),
    By(Position),
    With {
        anchor: WithAnchor,
        at: Position,
    },
    Color(Color, StringExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimKind {
    Ht,
    Wid,
    Rad,
    Diam,
    Thick,
    Scaled,
}

/// The reference point on an object used by `with … at …`.
#[derive(Debug, Clone, PartialEq)]
pub enum WithAnchor {
    Corner(Corner),
    Pair(Expr, Expr),
    Place(Place),
    Plain,
}

/// A position (point) expression. Positions support full vector arithmetic
/// (dpic): `p + q`, `p - q`, `p * s`, `p / s` with the usual precedence.
#[derive(Debug, Clone, PartialEq)]
pub enum Position {
    /// Explicit `x , y`.
    Pair(Expr, Expr),
    /// A bare location.
    Place(Location),
    /// `frac [of the way] between A and B`.
    Between {
        frac: Box<Expr>,
        a: Box<Position>,
        b: Box<Position>,
        of_the_way: bool,
    },
    /// `p + q` / `p - q`.
    Sum(Sign, Box<Position>, Box<Position>),
    /// `p * s` (or `p / s` when `div`), scaling a position by a scalar.
    Scale(Box<Position>, Expr, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    Plus,
    Minus,
}

/// A point-valued location.
#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    Place(Place),
    /// `( position )`.
    Paren(Box<Position>),
    /// `( position , position )` — x of the first, y of the second.
    ParenPair(Box<Position>, Box<Position>),
}

/// A named place in the drawing.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// A label, optionally subscripted.
    Name {
        name: String,
        subscript: Option<Box<Expr>>,
    },
    /// `last box`, `2nd last circle`, etc.
    Nth {
        count: Nth,
        obj: PrimObj,
    },
    /// `place . corner` (e.g. `A.ne`).
    Corner(Box<Place>, Corner),
    /// `corner of place` / `corner place` (e.g. `top of A`).
    CornerOf(Corner, Box<Place>),
    /// `place . place` (block sub-label, e.g. `B.A`).
    Member(Box<Place>, Box<Place>),
    Here,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Nth {
    Last,
    /// `count`-th object; `from_last` true for `… last`.
    Count(Box<Expr>, bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimObj {
    Prim(Prim),
    Block,
    Str(String),
    EmptyBrack,
}

/// A string-valued expression.
#[derive(Debug, Clone, PartialEq)]
pub enum StringExpr {
    Lit(String),
    Concat(Box<StringExpr>, Box<StringExpr>),
    Sprintf(Box<StringExpr>, Vec<Expr>),
    /// dpic SVG-backend helper. rpic does not emit backend preamble text, so
    /// this evaluates to a harmless empty string.
    SvgFont(Vec<Expr>),
    Arg(u32),
}

/// A scalar (numeric / boolean) expression. pic treats booleans as numbers.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    /// A string operand, valid only as an operand of `==`/`!=` (pic compares
    /// strings for equality, e.g. the `"$1"==""` default-argument idiom).
    Str(StringExpr),
    Var(String, Option<Box<Expr>>),
    Env(EnvVar),
    Unary(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Func1(Func1, Box<Expr>),
    Func2(Func2, Box<Expr>, Box<Expr>),
    Rand(Option<Box<Expr>>),
    /// `( name = expr )` — assign and yield the assigned value.
    Assign(String, Option<Box<Expr>>, Box<Expr>),
    DotX(Location),
    DotY(Location),
    PlaceAttr(Place, Param),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
