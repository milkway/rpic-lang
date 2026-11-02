//! A2 — geometric round-trip: a minimal reference renderer turns each corpus
//! figure's **scene** back into a plain SVG, both the official and the
//! reference SVG are rasterized with the same resvg pipeline (same bundled
//! fonts), and the pixel difference must stay ≤ 0.5% per figure. This proves
//! the scene alone carries enough to redraw the picture without rpic.
//!
//! The renderer is deliberately naive: it reads only documented scene fields
//! (docs/scene.md) and knows nothing about pic.

#![cfg(feature = "raster")]

use resvg::{tiny_skia, usvg};
use serde_json::Value;
use std::path::{Path, PathBuf};

// ---- reference renderer (scene JSON → SVG) -----------------------------------

struct Ref {
    out: String,
    defs: usize,
    // the documented scene→pixel transform
    ox: f64,
    oy: f64,
    h: f64,
}

fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

impl Ref {
    fn px(&self, p: &Value) -> (f64, f64) {
        let (x, y) = (n(&p[0]), n(&p[1]));
        ((x + self.ox) * 96.0, (self.h - (y + self.oy)) * 96.0)
    }

    fn pxs(&self, pts: &Value) -> String {
        pts.as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let (x, y) = self.px(p);
                format!("{x},{y}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn path_d(&self, path: &Value) -> String {
        let mut d = String::new();
        if let Some(cubics) = path.get("cubics") {
            let (sx, sy) = self.px(&path["start"]);
            d.push_str(&format!("M {sx} {sy}"));
            for c in cubics.as_array().unwrap() {
                let (x1, y1) = self.px(&c[0]);
                let (x2, y2) = self.px(&c[1]);
                let (x, y) = self.px(&c[2]);
                d.push_str(&format!(" C {x1} {y1} {x2} {y2} {x} {y}"));
            }
        } else {
            for (i, p) in path["points"].as_array().unwrap().iter().enumerate() {
                let (x, y) = self.px(p);
                d.push_str(&format!("{} {x} {y}", if i == 0 { "M" } else { " L" }));
            }
        }
        if path["closed"].as_bool().unwrap_or(false) {
            d.push_str(" Z");
        }
        d
    }

    fn stroke_attrs(&self, item: &Value) -> Option<String> {
        let s = item.get("stroke")?;
        let width = n(&s["width"]) * 96.0 / 72.0;
        let mut a = format!(
            "stroke=\"{}\" stroke-width=\"{width}\"",
            s["color"].as_str().unwrap()
        );
        if let Some(dash) = s.get("dash") {
            match dash["kind"].as_str().unwrap() {
                "dashed" => {
                    let l = n(&dash["len"]) * 96.0;
                    a.push_str(&format!(
                        " stroke-dasharray=\"{},{}\"",
                        l * 7.0 / 6.0,
                        l * 5.0 / 6.0
                    ));
                }
                _ => {
                    let gap = dash.get("gap").map(|g| n(g) * 96.0).unwrap_or(width * 5.0);
                    a.push_str(&format!(
                        " stroke-dasharray=\"0.5,{gap}\" stroke-linecap=\"round\""
                    ));
                }
            }
        }
        Some(a)
    }

    /// The SVG `fill` paint for an item: solid color, gradient or hatch
    /// pattern (gradient-under-hatch via a second underlay paint).
    fn fill_paints(&mut self, item: &Value) -> Option<(String, Option<String>)> {
        let f = item.get("fill")?;
        let hatch = f.get("hatch");
        let grad = f.get("gradient").map(|g| self.def_gradient(g));
        let color = f.get("color").map(|c| c.as_str().unwrap().to_string());
        match hatch {
            Some(hc) => {
                let tile_bg = color; // solid background lives inside the tile
                let pat = self.def_hatch(hc, tile_bg);
                Some((format!("url(#{pat})"), grad.map(|g| format!("url(#{g})"))))
            }
            None => match grad {
                Some(g) => Some((format!("url(#{g})"), None)),
                None => color.map(|c| (c, None)),
            },
        }
    }

    fn def_gradient(&mut self, g: &Value) -> String {
        let id = format!("rg{}", self.defs);
        self.defs += 1;
        let a = n(&g["angle"]).to_radians();
        let (dx, dy) = (a.cos(), -a.sin());
        let (x1, y1) = (0.5 - dx / 2.0, 0.5 - dy / 2.0);
        let (x2, y2) = (0.5 + dx / 2.0, 0.5 + dy / 2.0);
        let stops: String = g["stops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                format!(
                    "<stop offset=\"{}\" stop-color=\"{}\"/>",
                    n(&s["offset"]),
                    s["color"].as_str().unwrap()
                )
            })
            .collect();
        self.out.push_str(&format!(
            "<defs><linearGradient id=\"{id}\" gradientUnits=\"objectBoundingBox\" x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\">{stops}</linearGradient></defs>\n"
        ));
        id
    }

    fn def_hatch(&mut self, hc: &Value, bg: Option<String>) -> String {
        let id = format!("rh{}", self.defs);
        self.defs += 1;
        let sep = (n(&hc["spacing"]) * 96.0).max(1.0);
        let width = n(&hc["width"]) * 96.0 / 72.0;
        let color = hc["color"].as_str().unwrap();
        self.out.push_str(&format!(
            "<defs><pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" width=\"{sep}\" height=\"{sep}\" patternTransform=\"rotate({})\">\n",
            -n(&hc["angle"])
        ));
        if let Some(bg) = bg {
            self.out.push_str(&format!(
                "<rect x=\"0\" y=\"0\" width=\"{sep}\" height=\"{sep}\" fill=\"{bg}\"/>\n"
            ));
        }
        self.out.push_str(&format!(
            "<line x1=\"{}\" y1=\"0\" x2=\"{}\" y2=\"0\" stroke=\"{color}\" stroke-width=\"{width}\"/>\n",
            -sep,
            2.0 * sep
        ));
        if hc["cross"].as_bool().unwrap_or(false) {
            self.out.push_str(&format!(
                "<line x1=\"0\" y1=\"{}\" x2=\"0\" y2=\"{}\" stroke=\"{color}\" stroke-width=\"{width}\"/>\n",
                -sep,
                2.0 * sep
            ));
        }
        self.out.push_str("</pattern></defs>\n");
        id
    }

    fn opacity_attr(item: &Value) -> String {
        item.get("opacity")
            .map(|o| format!(" fill-opacity=\"{}\"", n(o)))
            .unwrap_or_default()
    }

    fn item(&mut self, item: &Value) {
        let kind = item["kind"].as_str().unwrap();
        if kind == "block" || item.get("hidden").is_some() {
            return;
        }
        let stroke = self.stroke_attrs(item);
        let fills = self.fill_paints(item);
        let op = Self::opacity_attr(item);
        match kind {
            "box" => {
                let (cx, cy) = self.px(&item["at"]);
                let (w, h) = (n(&item["w"]) * 96.0, n(&item["h"]) * 96.0);
                let mut attrs = format!(
                    "x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\"",
                    cx - w / 2.0,
                    cy - h / 2.0
                );
                if let Some(r) = item.get("rad") {
                    attrs.push_str(&format!(" rx=\"{}\"", n(r) * 96.0));
                }
                self.closed_el("rect", &attrs, &stroke, &fills, &op);
            }
            "circle" | "dot" => {
                let (cx, cy) = self.px(&item["at"]);
                let attrs = format!("cx=\"{cx}\" cy=\"{cy}\" r=\"{}\"", n(&item["r"]) * 96.0);
                self.closed_el("circle", &attrs, &stroke, &fills, &op);
            }
            "ellipse" => {
                let (cx, cy) = self.px(&item["at"]);
                let attrs = format!(
                    "cx=\"{cx}\" cy=\"{cy}\" rx=\"{}\" ry=\"{}\"",
                    n(&item["w"]) / 2.0 * 96.0,
                    n(&item["h"]) / 2.0 * 96.0
                );
                self.closed_el("ellipse", &attrs, &stroke, &fills, &op);
            }
            "line" | "arrow" => {
                let closed = item["closed"].as_bool().unwrap_or(false);
                if let Some((paint, under)) = &fills {
                    let pts = item.get("fillPoints").unwrap_or(&item["points"]);
                    let el = if closed { "polygon" } else { "polyline" };
                    let p = self.pxs(pts);
                    if let Some(u) = under {
                        self.out.push_str(&format!(
                            "<{el} points=\"{p}\" fill=\"{u}\"{op} stroke-width=\"0\"/>\n"
                        ));
                    }
                    self.out.push_str(&format!(
                        "<{el} points=\"{p}\" fill=\"{paint}\"{op} stroke-width=\"0\"/>\n"
                    ));
                }
                if let Some(st) = &stroke {
                    let el = if closed { "polygon" } else { "polyline" };
                    self.out.push_str(&format!(
                        "<{el} points=\"{}\" fill=\"none\" {st}/>\n",
                        self.pxs(&item["points"])
                    ));
                }
            }
            "spline" | "arc" | "brace" => {
                if let Some(paths) = item.get("paths").and_then(|p| p.as_array()) {
                    if let Some((paint, under)) = &fills {
                        let fp = item.get("fillPaths").and_then(|p| p.as_array());
                        for path in fp.unwrap_or(paths) {
                            let d = self.path_d(path);
                            if let Some(u) = under {
                                self.out.push_str(&format!(
                                    "<path d=\"{d}\" fill=\"{u}\"{op} stroke-width=\"0\"/>\n"
                                ));
                            }
                            self.out.push_str(&format!(
                                "<path d=\"{d}\" fill=\"{paint}\"{op} stroke-width=\"0\"/>\n"
                            ));
                        }
                    }
                    if let Some(st) = &stroke {
                        for path in paths {
                            self.out.push_str(&format!(
                                "<path d=\"{}\" fill=\"none\" {st}/>\n",
                                self.path_d(path)
                            ));
                        }
                    }
                }
            }
            "text" => {}
            other => panic!("unknown scene kind `{other}`"),
        }
        // arrowheads: painted in the stroke color
        if let (Some(heads), Some(s)) = (item.get("heads"), item.get("stroke")) {
            let color = s["color"].as_str().unwrap().to_string();
            let stroke_attrs = self.stroke_attrs(item).unwrap();
            for head in heads.as_array().unwrap() {
                if let Some(path) = head.get("path") {
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"{color}\" stroke-width=\"0\"/>\n",
                        self.path_d(path)
                    ));
                } else if head["filled"].as_bool().unwrap() {
                    self.out.push_str(&format!(
                        "<polygon points=\"{}\" fill=\"{color}\" stroke-width=\"0\"/>\n",
                        self.pxs(&head["points"])
                    ));
                } else {
                    self.out.push_str(&format!(
                        "<polyline points=\"{}\" fill=\"none\" {stroke_attrs}/>\n",
                        self.pxs(&head["points"])
                    ));
                }
            }
        }
        // text lines: `at` is the baseline anchor point (docs/scene.md §5)
        if let Some(lines) = item.get("text").and_then(|t| t.as_array()) {
            for line in lines {
                let (x, y) = self.px(&line["at"]);
                let anchor = match line["anchor"].as_str().unwrap() {
                    a if a.ends_with('w') => "start",
                    a if a.ends_with('e') => "end",
                    _ => "middle",
                };
                let f = &line["font"];
                let mut attrs = format!("font-size=\"{}pt\"", n(&f["size"]));
                if let Some(fam) = f.get("family") {
                    attrs.push_str(&format!(" font-family=\"{}\"", fam.as_str().unwrap()));
                }
                if f.get("bold").is_some() {
                    attrs.push_str(" font-weight=\"bold\"");
                }
                if f.get("italic").is_some() {
                    attrs.push_str(" font-style=\"italic\"");
                }
                if let Some(deg) = line.get("rotation") {
                    attrs.push_str(&format!(" transform=\"rotate({} {x} {y})\"", -n(deg)));
                }
                let text = line["text"].as_str().unwrap();
                let esc = text
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                self.out.push_str(&format!(
                    "<text {attrs} stroke-width=\"0.2pt\" fill=\"black\" x=\"{x}\" y=\"{y}\" text-anchor=\"{anchor}\">{esc}</text>\n"
                ));
            }
        }
    }

    fn closed_el(
        &mut self,
        el: &str,
        attrs: &str,
        stroke: &Option<String>,
        fills: &Option<(String, Option<String>)>,
        op: &str,
    ) {
        if let Some((paint, under)) = fills {
            if let Some(u) = under {
                self.out
                    .push_str(&format!("<{el} {attrs} fill=\"{u}\"{op}/>\n"));
            }
            self.out
                .push_str(&format!("<{el} {attrs} fill=\"{paint}\"{op}/>\n"));
        }
        if let Some(st) = stroke {
            self.out
                .push_str(&format!("<{el} {attrs} fill=\"none\" {st}/>\n"));
        }
    }
}

/// Render a scene document to a plain SVG string.
fn render_scene(scene: &Value) -> String {
    let c = &scene["canvas"];
    let (w, h) = (n(&c["w"]), n(&c["h"]));
    let mut r = Ref {
        out: String::new(),
        defs: 0,
        ox: n(&c["origin"][0]),
        oy: n(&c["origin"][1]),
        h,
    };
    r.out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"none\">\n",
        w * 96.0,
        h * 96.0,
        w * 96.0,
        h * 96.0,
        11.0 * 96.0 / 72.0
    ));
    for item in scene["items"].as_array().unwrap() {
        r.item(item);
    }
    r.out.push_str("</svg>\n");
    r.out
}

// ---- rasterized comparison ----------------------------------------------------

fn fontdb() -> usvg::fontdb::Database {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    let mut db = usvg::fontdb::Database::new();
    for f in [
        "Go-Regular.ttf",
        "Go-Bold.ttf",
        "Go-Italic.ttf",
        "Go-Bold-Italic.ttf",
        "Go-Mono.ttf",
        "Go-Mono-Bold.ttf",
    ] {
        db.load_font_data(std::fs::read(dir.join(f)).unwrap());
    }
    db.set_serif_family("Go");
    db.set_sans_serif_family("Go");
    db.set_cursive_family("Go");
    db.set_fantasy_family("Go");
    db.set_monospace_family("Go Mono");
    db
}

fn rasterize(svg: &str, scale: f32, db: &usvg::fontdb::Database) -> tiny_skia::Pixmap {
    let mut opt = usvg::Options::default();
    *opt.fontdb_mut() = db.clone();
    let tree = usvg::Tree::from_str(svg, &opt).expect("svg parses");
    let size = tree.size();
    let w = ((size.width() * scale).ceil() as u32).max(1);
    let h = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}

fn diff_ratio(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap) -> f64 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let (pa, pb) = (a.data(), b.data());
    let mut diff = 0usize;
    let total = (a.width() * a.height()) as usize;
    for i in 0..total {
        let o = i * 4;
        let d = (0..4)
            .map(|k| (pa[o + k] as i16 - pb[o + k] as i16).unsigned_abs())
            .max()
            .unwrap();
        if d > 32 {
            diff += 1;
        }
    }
    diff as f64 / total as f64
}

fn collect_pics(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_pics(&path, out);
        } else if path.extension().is_some_and(|e| e == "pic") {
            out.push(path);
        }
    }
}

#[test]
fn scene_reference_render_matches_official_svg_within_half_percent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut pics = Vec::new();
    collect_pics(&root.join("examples"), &mut pics);
    collect_pics(&root.join("assets"), &mut pics);
    pics.sort();
    assert!(pics.len() > 100);

    let db = fontdb();
    let opts = rpic_core::CompileOptions {
        circuits: true,
        base: None,
        ..Default::default()
    };
    let mut worst: Vec<(f64, String)> = Vec::new();
    let mut failures = Vec::new();
    for pic in &pics {
        let src = std::fs::read_to_string(pic).unwrap();
        let mut o = opts.clone();
        o.base = pic.parent().map(|p| p.to_path_buf());
        let Ok(d) = rpic_core::compile_with_options(&src, &o) else {
            continue; // shims / fragments that don't compile standalone
        };
        let official = rpic_core::to_svg(&d);
        let scene: Value = serde_json::from_str(
            &rpic_core::scene_json(&d, &rpic_core::SceneOptions::default()).json,
        )
        .unwrap();
        let reference = render_scene(&scene);

        // low resolution: longest side ≈ 400 px
        let probe = rasterize(&official, 1.0, &db);
        let longest = probe.width().max(probe.height()) as f32;
        let scale = (400.0 / longest).min(1.0);
        let a = rasterize(&official, scale, &db);
        let b = rasterize(&reference, scale, &db);
        let ratio = diff_ratio(&a, &b);
        worst.push((ratio, pic.display().to_string()));
        if ratio > 0.005 {
            failures.push(format!("{}: {:.3}%", pic.display(), ratio * 100.0));
        }
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    eprintln!("A2 worst figures:");
    for (r, name) in worst.iter().take(10) {
        eprintln!("  {:.4}%  {name}", r * 100.0);
    }
    assert!(
        failures.is_empty(),
        "figures over the 0.5% pixel budget:\n{}",
        failures.join("\n")
    );
}
