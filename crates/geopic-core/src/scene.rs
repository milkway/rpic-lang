//! The scene document (rpic `docs/scene.md`, format 1), as this crate reads
//! it. Every field the converter does not need is simply absent here; serde
//! ignores unknown keys, so additive growth of the format is harmless.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn one() -> f64 {
    1.0
}

/// A parsed scene.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Scene {
    /// Format version (this crate reads 1).
    pub scene: u32,
    #[serde(default)]
    pub units: String,
    #[serde(default)]
    pub y_up: bool,
    pub canvas: Canvas,
    /// Whole-picture rescale factor (`.PS <w>`, `maxps*`); `vars` are
    /// pre-rescale, the geometry is post-rescale. Absent in scenes older
    /// than engine 0.14 → 1.
    #[serde(default = "one")]
    pub rescale: f64,
    #[serde(default)]
    pub source: Option<String>,
    /// Final value of every variable, by pic name.
    #[serde(default)]
    pub vars: BTreeMap<String, f64>,
    /// Named places (`Label: position`), evaluation order.
    #[serde(default)]
    pub places: Vec<Place>,
    /// The draw list, draw order.
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Canvas {
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub margin: Margin,
    /// Where the pic origin sits, from the canvas lower-left corner.
    pub origin: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Margin {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub right: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub left: f64,
}

/// A named place (`Label: position`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Place {
    pub label: String,
    pub at: [f64; 2],
    #[serde(default)]
    pub block: Option<Vec<String>>,
    #[serde(default)]
    pub span: Option<Span>,
}

/// A source position, relative to its own source unit (as in diagnostics).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// One entry of the draw list.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub z: Option<usize>,
    pub label: Option<String>,
    pub block: Option<Vec<String>>,
    pub class: Option<String>,
    pub link: Option<String>,
    pub hidden: Option<bool>,
    pub at: Option<[f64; 2]>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub r: Option<f64>,
    pub rad: Option<f64>,
    pub points: Option<Vec<[f64; 2]>>,
    pub closed: Option<bool>,
    pub paths: Option<Vec<Path>>,
    #[serde(rename = "fillPaths")]
    pub fill_paths: Option<Vec<Path>>,
    #[serde(rename = "fillPoints")]
    pub fill_points: Option<Vec<[f64; 2]>>,
    pub heads: Option<Vec<Head>>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub opacity: Option<f64>,
    pub text: Option<Vec<TextLine>>,
    /// The centre the text lines were laid out around (engine ≥ 0.15.1,
    /// scene DECISIONS S12).
    pub text_center: Option<[f64; 2]>,
    /// The label stack's height in rows, blank rows included (engine after
    /// 0.15.1, scene DECISIONS S13).
    pub text_rows: Option<usize>,
    pub items: Option<Vec<String>>,
    pub span: Option<Span>,
    pub call: Option<serde_json::Value>,
}

/// A curve: cubic Béziers from `start`, or an already-flattened polyline.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Path {
    pub closed: bool,
    pub start: Option<[f64; 2]>,
    /// `[[c1, c2, end], …]`
    pub cubics: Option<Vec<[[f64; 2]; 3]>>,
    pub points: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Head {
    pub at: String,
    pub filled: bool,
    pub points: Option<Vec<[f64; 2]>>,
    pub path: Option<Path>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Stroke {
    pub color: String,
    /// Points.
    pub width: f64,
    pub dash: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Fill {
    pub color: Option<String>,
    pub gradient: Option<Gradient>,
    pub hatch: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub angle: f64,
    pub stops: Vec<Stop>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Stop {
    pub offset: f64,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct TextLine {
    pub text: String,
    pub at: [f64; 2],
    pub anchor: String,
    pub font: Option<serde_json::Value>,
    pub rotation: Option<f64>,
    pub math: Option<serde_json::Value>,
    pub outline: Option<String>,
    /// 0-based row of the line in the author's stack, blank rows included
    /// (engine after 0.15.1, scene DECISIONS S13).
    pub row: Option<usize>,
}

impl Item {
    /// A representative canvas point: the anchor/centre when the item has
    /// one, else the centroid of its vertices, else the start of its first
    /// path.
    pub fn reference_point(&self) -> Option<[f64; 2]> {
        if let Some(at) = self.at {
            return Some(at);
        }
        if let Some(pts) = &self.points
            && !pts.is_empty()
        {
            let n = pts.len() as f64;
            let (sx, sy) = pts
                .iter()
                .fold((0.0, 0.0), |(sx, sy), p| (sx + p[0], sy + p[1]));
            return Some([sx / n, sy / n]);
        }
        self.paths.as_ref().and_then(|ps| ps.first()).and_then(|p| {
            p.start
                .or_else(|| p.points.as_ref().and_then(|v| v.first().copied()))
        })
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden == Some(true)
    }
}

impl Scene {
    /// Canvas point a label names, with pic's scoping: among the places and
    /// objects carrying the label, a top-level definition beats a block-local
    /// one, and among those the **latest in the source** (by span; by
    /// position in the scene when spans are missing) wins — so a definition
    /// the user wrote overrides one a host prepended.
    pub fn point_of_label(&self, label: &str) -> Option<[f64; 2]> {
        // (top-level?, line, col, order) → point
        type Key = (bool, u32, u32, usize);
        let mut best: Option<(Key, [f64; 2])> = None;
        let mut consider = |top: bool, span: Option<&Span>, order: usize, at: [f64; 2]| {
            let key = (
                top,
                span.map_or(0, |s| s.line),
                span.map_or(0, |s| s.col),
                order,
            );
            if best.is_none_or(|(k, _)| key > k) {
                best = Some((key, at));
            }
        };
        for (i, p) in self.places.iter().enumerate() {
            if p.label == label {
                consider(p.block.is_none(), p.span.as_ref(), i, p.at);
            }
        }
        for (i, it) in self.items.iter().enumerate() {
            if it.label.as_deref() == Some(label)
                && let Some(at) = it.reference_point()
            {
                consider(it.block.is_none(), it.span.as_ref(), i, at);
            }
        }
        best.map(|(_, at)| at)
    }
}
