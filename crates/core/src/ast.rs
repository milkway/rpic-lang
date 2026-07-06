//! Abstract syntax tree for the pic language.
//!
//! Mirrors the structure of dpic's `grammar.txt`. The parser populates the
//! drawing core (pictures, primitives + attributes, positions, expressions,
//! blocks, assignments) plus the control constructs (`if`/`for`/`define`/
//! `print`/`exec`) used by the evaluator and macro preprocessor.

use crate::diagnostic::Span;
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
    /// Filesystem context for `copy "file"` includes (directory + policy);
    /// carried so eval-time deferred parsing resolves includes the same way.
    pub includes: IncludeCtx,
}

/// Policy for `copy "file"` filesystem includes. The default matches the CLI:
/// full access, absolute paths allowed. Embedders compiling untrusted source
/// should pick a restrictive policy. The embedded `copy "circuits"` library
/// is always available — it never touches the filesystem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IncludePolicy {
    /// Resolve like the CLI: absolute paths allowed, no fence. (Default.)
    #[default]
    Unrestricted,
    /// Only files inside the base directory (canonicalized prefix check, so
    /// `..` and symlink escapes are rejected); absolute paths are errors.
    SandboxedToBase,
    /// No filesystem includes at all (the wasm behavior everywhere).
    Deny,
}

/// Where and how `copy "file"` includes resolve: the current directory (the
/// including file's own dir, which varies as includes nest) plus the fixed
/// [`IncludePolicy`] and its canonicalized fence root.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IncludeCtx {
    /// Directory relative includes resolve against; `None` when parsing has
    /// no filesystem context (WASM/bindings without `base`).
    pub dir: Option<std::path::PathBuf>,
    /// Policy applied to every filesystem include in this compilation.
    pub policy: IncludePolicy,
    /// Canonicalized sandbox root. Set when the policy is `SandboxedToBase`
    /// and the base directory canonicalizes; `None` under that policy means
    /// every filesystem include is denied (a fence that failed to resolve
    /// must fail closed).
    pub(crate) fence: Option<std::path::PathBuf>,
}

impl IncludeCtx {
    /// The CLI behavior: resolve against `dir`, no restrictions.
    pub fn unrestricted(dir: Option<std::path::PathBuf>) -> Self {
        Self {
            dir,
            policy: IncludePolicy::Unrestricted,
            fence: None,
        }
    }

    /// Build a context for `dir` under `policy`, canonicalizing the fence
    /// root for `SandboxedToBase` (fails closed if it cannot resolve).
    pub fn with_policy(dir: Option<std::path::PathBuf>, policy: IncludePolicy) -> Self {
        let fence = match policy {
            IncludePolicy::SandboxedToBase => {
                dir.as_deref().and_then(|d| std::fs::canonicalize(d).ok())
            }
            _ => None,
        };
        Self { dir, policy, fence }
    }

    /// A nested include's context: its own directory, same policy and fence.
    pub(crate) fn child(&self, dir: Option<std::path::PathBuf>) -> Self {
        Self {
            dir,
            policy: self.policy,
            fence: self.fence.clone(),
        }
    }
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
    // Boxed: `Animate` carries several `Place`s and is far larger than the
    // other variants (clippy::large_enum_variant).
    Animate(Box<Animate>),
    /// rpic extension: `class <place> "name"` — append a CSS class to an
    /// already-drawn object's shape group (labels and ordinals both work).
    Class { target: Place, class: StringExpr },
    /// rpic extension: `canvas from <pos> to <pos>` — fix the output page to
    /// the rectangle spanned by the two corners, independent of content, so
    /// the viewBox stays stable while objects move (visual editors).
    Canvas { from: Position, to: Position },
    /// `if <cond> then { … } [else { … }]`. Bodies are deferred raw tokens.
    If {
        cond: Expr,
        then_body: Body,
        else_body: Option<Body>,
    },
    /// `for v = from to to [by [*] step] do { … }`. Body is deferred raw tokens.
    For {
        var: String,
        subscript: Option<Expr>,
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

/// `animate <target> with "<effect>" [along <path>] [for <dur>]
///   [at <t> | after <ref>] [delay <d>] [repeat <n>] [yoyo] [ease "<name>"]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Animate {
    pub target: Place,
    pub effect: StringExpr,
    pub effect_span: Option<Span>,
    pub duration: Option<Expr>,
    pub timing: Timing,
    pub delay: Option<Expr>,
    /// Object whose geometry the `move` effect follows (GSAP MotionPath).
    pub along: Option<Place>,
    /// Number of repeats after the first play (GSAP `repeat`): `-1` loops
    /// forever, `0`/absent plays once.
    pub repeat: Option<Expr>,
    /// Alternate direction on each repeat (GSAP `yoyo`).
    pub yoyo: bool,
    /// GSAP easing name overriding the per-effect default (e.g. `"elastic.out"`).
    pub ease: Option<StringExpr>,
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
    Set,      // =
    ColonSet, // :=
    Add,      // +=
    Sub,      // -=
    Mul,      // *=
    Div,      // /=
    Rem,      // %=
}

/// A drawable object: a base plus a chain of attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub kind: ObjectKind,
    pub attrs: Vec<Attr>,
    /// Span of the statement's leading token (for per-object geometry export
    /// and future diagnostics).
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKind {
    Primitive(Prim),
    Block(Vec<Stmt>),
    /// `[]` empty-block reference.
    Empty,
    /// A bare quoted string places a text-only object.
    Text,
    /// rpic extension: a curly brace annotation between two points.
    Brace,
    /// rpic extension: a junction dot — a tiny solid circle (`dotrad`).
    Dot,
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
    Dist(Expr, Option<Span>),
    /// `spline <expr> …`: the expression right after `spline` is a spline
    /// tension parameter (dpic semantics), NOT a distance.
    SplineTension(Expr),
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
    /// rpic extension: size a closed object to text declared before `fit`.
    Fit,
    /// rpic extension: hatch-filled closed regions.
    Hatch(HatchKind),
    /// rpic extension: hatch line angle in degrees.
    HatchAngle(Expr),
    /// rpic extension: hatch line spacing in pic units.
    HatchSep(Expr),
    /// rpic extension: hatch line width in points.
    HatchWidth(Expr),
    /// rpic extension: hatch line color.
    HatchColor(StringExpr),
    /// rpic extension: fill opacity, 0 = transparent and 1 = opaque.
    Opacity(Expr),
    /// rpic extension: two-stop linear gradient fill (`from`, `to` colors).
    Gradient(StringExpr, StringExpr),
    /// rpic extension: gradient angle in degrees, pic coordinates.
    GradientAngle(Expr),
    /// rpic extension: close a `line` path into a polygon.
    Close,
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
    /// rpic extension: draw this object below another already-placed object.
    Behind(Place),
    /// rpic extension: CSS class hook attached to this object's shape group.
    Class(StringExpr),
    /// rpic extension: bold face for the preceding text string.
    Bold,
    /// rpic extension: italic face for the preceding text string.
    Italic,
    /// rpic extension: monospace family for the preceding text string.
    Mono,
    /// rpic extension: font family for the preceding text string.
    Font(StringExpr),
    /// rpic extension: font size in points for the preceding text string.
    FontSize(Expr),
    /// rpic extension: rotation in degrees (CCW) for the preceding string.
    Rotated(Expr),
    /// rpic extension: align the preceding string to the host segment's angle
    /// (pikchr `aligned`). Only linear objects have a segment; elsewhere inert.
    Aligned,
    /// rpic extension: pikchr `big`/`small` text size (sugar over `fontsize`);
    /// `true` = big (1.5×), `false` = small (0.7×) of the classic 11 pt.
    Sized(bool),
    /// rpic extension: relative cusp position for a `brace` object.
    BracePos(Expr),
    /// rpic extension: extra outward spacing between a `brace` cusp and label.
    BraceLabelOffset(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HatchKind {
    Single,
    Cross,
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
        /// Span of the reference (for eval-phase diagnostics).
        span: Option<Span>,
    },
    /// `last box`, `2nd last circle`, etc.
    Nth {
        count: Nth,
        obj: PrimObj,
        /// Span of the reference (for eval-phase diagnostics).
        span: Option<Span>,
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
    Brace,
    Block,
    Str(String),
    EmptyBrack,
    /// Untyped `last` / `Nth last` — the most recent object of any kind
    /// (e.g. `last.c`, `2nd last.n`). Used when no type keyword follows.
    Any,
}

/// A string-valued expression.
#[derive(Debug, Clone, PartialEq)]
pub enum StringExpr {
    Lit(String),
    Concat(Box<StringExpr>, Box<StringExpr>),
    Sprintf(Box<StringExpr>, Vec<Expr>),
    /// rpic extension: `rgb(r,g,b)` colour literal (components 0–255);
    /// evaluates to `#rrggbb`.
    Rgb(Box<[Expr; 3]>),
    /// rpic extension: a numeric colour in colour position (`shaded
    /// 0x1b5e20`, pikchr-style); evaluates to `#rrggbb`.
    ColorNum(Box<Expr>),
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
    /// Comma-separated array subscript, valid only inside `name[...]`.
    Index(Vec<Expr>),
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
