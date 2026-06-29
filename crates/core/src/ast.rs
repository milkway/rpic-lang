//! Abstract syntax tree for the pic language.
//!
//! Mirrors the structure of dpic's `grammar.txt`. The current parser populates
//! the drawing core (pictures, primitives + attributes, positions, expressions,
//! blocks, assignments). Control constructs (`if`/`for`/`define`/`print`/…) have
//! AST slots reserved but are wired up in a later milestone.

use crate::token::{
    Arrow, Color, Corner, Dir, EnvVar, Func1, Func2, LineType, Param, Prim, TextPos,
};

/// A complete picture: optional `.PS <w> <h>` dimensions plus a list of elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: Option<Expr>,
    pub height: Option<Expr>,
    pub stmts: Vec<Stmt>,
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
    /// `if <cond> then { … } [else { … }]`.
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    /// `for v = from to to [by [*] step] do { … }`.
    For {
        var: String,
        from: Expr,
        to: Expr,
        by: Expr,
        /// `by *` multiplies instead of adds.
        mult: bool,
        body: Vec<Stmt>,
    },
    /// `print …` (evaluated for diagnostics; no drawing effect).
    Print(PrintItem),
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
    With { anchor: WithAnchor, at: Position },
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
    Plain,
}

/// A position (point) expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Position {
    /// Explicit `x , y`.
    Pair(Expr, Expr),
    /// A location plus zero or more `± location` shifts.
    Place(Location, Vec<Shift>),
    /// `frac [of the way] between A and B`.
    Between {
        frac: Box<Expr>,
        a: Box<Position>,
        b: Box<Position>,
        of_the_way: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shift {
    pub sign: Sign,
    pub loc: Location,
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
    Arg(u32),
}

/// A scalar (numeric / boolean) expression. pic treats booleans as numbers.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(String),
    Env(EnvVar),
    Unary(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Func1(Func1, Box<Expr>),
    Func2(Func2, Box<Expr>, Box<Expr>),
    Rand(Option<Box<Expr>>),
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
