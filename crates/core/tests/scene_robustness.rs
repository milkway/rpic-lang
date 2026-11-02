//! C1/C2 — robustness of the scene path: hostile inputs produce a structured
//! error or a valid scene, never a panic/hang; plus an in-tree mutational
//! fuzzer (the repo has no cargo-fuzz harness; this seeds from pic-shaped
//! snippets and random mutations).
//!
//! The fuzzer is `#[ignore]`d for CI speed. Run the C1 budget locally with:
//!
//! ```sh
//! RPIC_FUZZ_SECS=600 cargo test -p rpic-core --test scene_robustness --release -- --ignored fuzz
//! ```

use rpic_core::{
    CompileOptions, IncludePolicy, SceneOptions, compile_with_diagnostics, scene_json,
};

fn scene_of(src: &str, opts: &CompileOptions, sopts: &SceneOptions) -> Result<String, String> {
    match compile_with_diagnostics(src, opts) {
        Ok(d) => Ok(scene_json(&d, sopts).json),
        Err(e) => Err(e.message),
    }
}

#[test]
fn denied_copy_is_a_structured_error_not_a_crash() {
    let opts = CompileOptions {
        includes: IncludePolicy::Deny,
        ..Default::default()
    };
    let err = compile_with_diagnostics("copy \"evil.pic\"\nbox", &opts).unwrap_err();
    assert_eq!(err.info.kind, "include_denied");
}

#[test]
fn giant_labels_produce_a_valid_scene() {
    let label = "x".repeat(1_000_000);
    let src = format!("box \"{label}\"");
    let json = scene_of(&src, &Default::default(), &Default::default()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        v["items"][0]["text"][0]["text"].as_str().unwrap().len(),
        1_000_000
    );
}

#[test]
fn hundred_thousand_objects_error_or_succeed_structurally() {
    // a host shape budget below the count: structured error
    let src = "for i = 1 to 100000 do { box at (i/100, 0) }";
    let capped = CompileOptions {
        max_shapes: Some(50_000),
        ..Default::default()
    };
    let err = compile_with_diagnostics(src, &capped).unwrap_err();
    assert!(err.message.contains("shapes"), "{}", err.message);

    // raised budget: a valid scene with 1e5 items
    let opts = CompileOptions {
        max_shapes: Some(150_000),
        max_loop_iterations: Some(150_000),
        ..Default::default()
    };
    let d = compile_with_diagnostics(src, &opts).unwrap();
    let out = scene_json(&d, &SceneOptions::default());
    let v: serde_json::Value = serde_json::from_str(&out.json).expect("valid JSON");
    assert_eq!(v["items"].as_array().unwrap().len(), 100_000);
}

#[test]
fn runaway_macro_recursion_is_a_structured_error() {
    let src = "define boom { boom() }\nboom()";
    let err = compile_with_diagnostics(src, &Default::default()).unwrap_err();
    assert!(!err.message.is_empty());
}

#[test]
fn hostile_flatten_tolerances_stay_bounded() {
    let d = compile_with_diagnostics(
        "spline from (0,0) to (1e6,1e6) then to (2e6,0) then to (3e6,1e6)",
        &Default::default(),
    )
    .unwrap();
    for tol in [1e-300, 1e-12, 0.0_f64.next_up(), 1e9] {
        let out = scene_json(
            &d,
            &SceneOptions {
                flatten: Some(tol),
                ..Default::default()
            },
        );
        // depth cap: ≤ 2^12 points per cubic; the corpus never gets close
        assert!(
            out.json.len() < 3_000_000,
            "tol {tol}: {} bytes",
            out.json.len()
        );
        let _: serde_json::Value = serde_json::from_str(&out.json).expect("valid JSON");
    }
}

#[test]
fn flatten_point_budget_bounds_hostile_inputs_and_warns() {
    // a huge spline × tiny tolerance would amplify the output unboundedly;
    // the scene-wide point budget degrades resolution instead, with a warning
    let mut src = String::from("spline from (0,0)");
    for i in 1..4000 {
        src.push_str(&format!(
            " then to ({}, {})",
            i as f64 / 100.0,
            (i % 2) as f64
        ));
    }
    let d = compile_with_diagnostics(&src, &Default::default()).unwrap();
    let out = scene_json(
        &d,
        &SceneOptions {
            flatten: Some(1e-300),
            ..Default::default()
        },
    );
    assert!(
        out.json.len() < 64_000_000,
        "output must stay bounded: {} bytes",
        out.json.len()
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.kind == "scene_unsupported" && w.message.contains("flatten point budget")),
        "budget exhaustion must be reported: {:?}",
        out.warnings
    );
    let _: serde_json::Value = serde_json::from_str(&out.json).expect("valid JSON");
}

#[test]
fn non_finite_geometry_still_yields_valid_json() {
    // division toward zero-length elements and huge coordinates
    let src = "x = 1e308\nline from (0,0) to (x, x)\nbox wid 0 ht 0\narc from (0,0) to (0,0)";
    if let Ok(d) = compile_with_diagnostics(src, &Default::default()) {
        let out = scene_json(&d, &SceneOptions::default());
        let _: serde_json::Value = serde_json::from_str(&out.json).expect("valid JSON");
    }
}

// ---- mutational fuzz (C1) ------------------------------------------------------

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const SEEDS: &[&str] = &[
    "box \"a\" fit\narrow right\nC: circle rad 0.2 dashed\nspline from C.s down 0.3 then right ->",
    "K: [ B: box behind last; dot at B.c ]\nline from K.e to (3,0) close fill 0.3",
    "arc from (0,0) to (1,1) cw <->\nbrace from (2,0) to (2,1) \"b\"",
    "for i = 1 to 9 do { box at (i/4, 0) color rgb(i*20, 100, 200) }",
    "geo = -34.88\n\"$x^2$\" at (0,1) rotated 45 bold\ncopy \"circuits\"",
    "define m(a) { box wid $1 }\nm(0.4)\nm(0.2)\nanimate last with \"draw\" from 20% to 80%",
];
const TOKENS: &[&str] = &[
    "box", "circle", "ellipse", "line", "arrow", "spline", "arc", "move", "dot", "brace", "from",
    "to", "then", "at", "with", ".e", ".nw", "last", "2nd", "behind", "close", "fit", "dashed",
    "dotted", "invis", "fill", "0.5", "-1e9", "(", ")", "[", "]", "{", "}", "\"t\"", "$", "\\",
    ";", "\n", "define", "copy", "for", "do", "rotated", "aligned", "hatch", "gradient", "opacity",
    "link", "class", "wid", "ht", "rad", "same", "<->", "->",
];

#[test]
#[ignore = "fuzz budget; run with RPIC_FUZZ_SECS=600 -- --ignored"]
fn fuzz_scene_never_panics() {
    let secs: u64 = std::env::var("RPIC_FUZZ_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut rng = XorShift(0x9e3779b97f4a7c15);
    let opts = CompileOptions {
        includes: IncludePolicy::Deny,
        max_loop_iterations: Some(10_000),
        max_shapes: Some(10_000),
        ..Default::default()
    };
    let mut iters: u64 = 0;
    while std::time::Instant::now() < deadline {
        iters += 1;
        let mut src = SEEDS[rng.below(SEEDS.len())].to_string();
        for _ in 0..rng.below(12) {
            match rng.below(4) {
                0 => {
                    // splice a random token at a random byte boundary
                    let mut at = rng.below(src.len() + 1);
                    while !src.is_char_boundary(at) {
                        at -= 1;
                    }
                    src.insert_str(at, TOKENS[rng.below(TOKENS.len())]);
                }
                1 => {
                    // truncate
                    let mut at = rng.below(src.len() + 1);
                    while !src.is_char_boundary(at) {
                        at -= 1;
                    }
                    src.truncate(at);
                }
                2 => {
                    // duplicate a slice
                    let mut a = rng.below(src.len() + 1);
                    while !src.is_char_boundary(a) {
                        a -= 1;
                    }
                    let mut b = a + rng.below(src.len() - a + 1);
                    while !src.is_char_boundary(b) {
                        b -= 1;
                    }
                    let slice = src[a..b].to_string();
                    src.push_str(&slice);
                }
                _ => {
                    // random byte flip to an ASCII char
                    if !src.is_empty() {
                        let mut at = rng.below(src.len());
                        while !src.is_char_boundary(at) {
                            at -= 1;
                        }
                        let ch = (32 + rng.below(95)) as u8 as char;
                        src.replace_range(
                            at..at + src[at..].chars().next().unwrap().len_utf8(),
                            &ch.to_string(),
                        );
                    }
                }
            }
        }
        let flatten = match rng.below(3) {
            0 => None,
            1 => Some(1e-3),
            _ => Some(f64::from_bits(rng.next()).abs()),
        };
        let sopts = SceneOptions {
            flatten: flatten.filter(|f| f.is_finite() && *f > 0.0),
            outline: rng.below(2) == 1,
            source: None,
        };
        let result = std::panic::catch_unwind(|| {
            if let Ok(d) = compile_with_diagnostics(&src, &opts) {
                let out = scene_json(&d, &sopts);
                assert!(
                    serde_json::from_str::<serde_json::Value>(&out.json).is_ok(),
                    "scene JSON must stay well-formed"
                );
            }
        });
        assert!(result.is_ok(), "panic on input:\n{src}");
    }
    eprintln!("fuzzed {iters} inputs in {secs}s without a panic");
}
