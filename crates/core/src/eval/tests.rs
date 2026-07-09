use super::*;
use crate::parser::{parse, parse_in_dir};

fn draw(src: &str) -> Drawing {
    eval(&parse(src).unwrap()).unwrap()
}

fn scalar(src: &str) -> ER<f64> {
    let prog = parse(&format!("x = {src}")).unwrap();
    let mut st = State::new();
    st.eval_stmts(&prog.stmts)?;
    Ok(st.vars["x"])
}

fn assert_box_size(shape: &Shape, want_w: f64, want_h: f64) {
    let Shape::Box { w, h, .. } = shape else {
        panic!()
    };
    assert!((*w - want_w).abs() < 1e-9, "w = {w}, want {want_w}");
    assert!((*h - want_h).abs() < 1e-9, "h = {h}, want {want_h}");
}

const DEFAULT_STROKE_IN: f64 = 0.8 / 72.0;

#[test]
fn zero_iteration_for_body_is_never_parsed() {
    // #196: a dead loop body must not be parsed or macro-expanded —
    // the same deferred rule as dead `if` branches (dpic accepts both).
    let d = draw("for i = 1 to 0 do { bxo }\nbox");
    assert_eq!(d.shapes.len(), 1);

    // a recursive macro in a dead body must not hit the expansion guard
    let d = draw("define f { f() }\nfor i = 1 to 0 do { f() }\nbox");
    assert_eq!(d.shapes.len(), 1);

    // a backwards `by` range that yields no iterations counts too
    let d = draw("for i = 5 to 1 do { bxo }\nbox");
    assert_eq!(d.shapes.len(), 1);
}

#[test]
fn executed_for_body_still_reports_errors_with_structure() {
    let e = eval(&parse("for i = 1 to 2 do { bxo }").unwrap()).unwrap_err();
    assert!(e.msg.contains("expected an object"), "{e}");
    let info = e.info.expect("structured info");
    assert_eq!(info.kind, "expected_token");
    assert_eq!(info.hint.as_deref(), Some("did you mean `box`?"));
}

#[test]
fn pipeline_chains_left_to_right() {
    let d = draw(".PS\nellipse \"document\"\narrow\nbox \"PIC\"\narrow\nbox \"TROFF\"\n.PE");
    // 5 shapes
    assert_eq!(d.shapes.len(), 5);
    // first ellipse centered at (0.375, 0): ellipsewid/2
    if let Shape::Ellipse { c, .. } = &d.shapes[0] {
        assert!((c.x - 0.375).abs() < 1e-9, "ellipse x = {}", c.x);
        assert!(c.y.abs() < 1e-9);
    } else {
        panic!("expected ellipse");
    }
    // bbox grows to the right
    assert!(d.bbox.width() > 2.0);
    assert!((d.bbox.height() - (0.5 + DEFAULT_STROKE_IN)).abs() < 1e-9);
}

#[test]
fn box_at_absolute() {
    let d = draw("box ht 0.3 wid 0.5 at 1,2");
    let Shape::Box { c, w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(*c, Point::new(1.0, 2.0));
    assert!((*w - 0.5).abs() < 1e-9 && (*h - 0.3).abs() < 1e-9);
}

#[test]
fn diamond_is_closed() {
    let d = draw("line up right then down right then down left then up left");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(pts.len(), 5);
    // returns to start
    assert!(pts[0].dist(*pts.last().unwrap()) < 1e-9);
}

#[test]
fn close_line_marks_polygon_and_uses_bbox_center_anchor() {
    let d =
        draw("L: line right 1 then up 1 close\nbox wid .1 ht .1 at L.c\nbox wid .1 ht .1 at L.end");
    let Shape::Path { pts, closed, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(*closed);
    assert_eq!(pts.len(), 4);
    assert_eq!(pts[0], Point::ZERO);
    assert_eq!(pts[1], Point::new(1.0, 0.0));
    assert_eq!(pts[2], Point::new(1.0, 1.0));
    assert_eq!(pts[3], Point::ZERO);

    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert_eq!(*c, Point::new(0.5, 0.5));

    let Shape::Box { c, .. } = &d.shapes[2] else {
        panic!()
    };
    assert_eq!(*c, Point::ZERO);
}

#[test]
fn close_line_requires_three_vertices_and_ends_path() {
    let err = eval(&parse("line right close").unwrap()).unwrap_err();
    assert!(err.msg.contains("need at least 3 vertices"), "{err}");

    let err = eval(&parse("line right then up close then left").unwrap()).unwrap_err();
    assert!(err.msg.contains("polygon is closed"), "{err}");
}

#[test]
fn corners_and_labels() {
    let d = draw("A: box wid 1 ht 1 at 0,0\nbox wid 0.5 ht 0.5 with .sw at A.ne");
    // second box sw corner at A.ne (0.5,0.5) => its center at (0.75,0.75)
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(
        (c.x - 0.75).abs() < 1e-9 && (c.y - 0.75).abs() < 1e-9,
        "c = {c:?}"
    );
}

#[test]
fn circle_corner_anchors_are_on_the_circumference() {
    let d = draw("C: circle rad 1 at 0,0\nline from C.sw to C.ne");
    let Shape::Path { pts, .. } = &d.shapes[1] else {
        panic!()
    };
    let a = 1.0 / 2.0_f64.sqrt();
    assert_eq!(pts.len(), 2);
    assert!((pts[0].x + a).abs() < 1e-9 && (pts[0].y + a).abs() < 1e-9);
    assert!((pts[1].x - a).abs() < 1e-9 && (pts[1].y - a).abs() < 1e-9);
}

#[test]
fn type_specific_corner_anchors_match_dpic() {
    let d = draw("L: line from (0,0) to (1,0) then to (1,1)\ncircle rad .01 at L.nw");
    let Shape::Circle { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(c.dist(Point::new(0.0, 0.0)) < 1e-9, "line nw = {c:?}");

    let d = draw("A: arc cw rad 0.5 from (0,0) to (0.5,0.5)\ncircle rad .01 at A.s");
    let Shape::Circle { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(c.dist(Point::new(0.5, -0.5)) < 1e-9, "arc s = {c:?}");

    let d = draw("B: box wid 1 ht 1 rad 0.3 at (0,0)\ncircle rad .01 at B.ne");
    let Shape::Circle { c, .. } = &d.shapes[1] else {
        panic!()
    };
    let inset = 0.3 * (1.0 - FRAC_1_SQRT_2);
    let expected = Point::new(0.5 - inset, 0.5 - inset);
    assert!(c.dist(expected) < 1e-9, "rounded box ne = {c:?}");
}

#[test]
fn with_corner_uses_ellipse_geometry_for_placed_object() {
    let d = draw("ellipse; ellipse with .nw at last ellipse.se");
    let Shape::Ellipse { c: first, .. } = &d.shapes[0] else {
        panic!()
    };
    let Shape::Ellipse { c: second, .. } = &d.shapes[1] else {
        panic!()
    };
    let expected = *first + Point::new(0.75 * FRAC_1_SQRT_2, -0.5 * FRAC_1_SQRT_2);
    assert!(
        second.dist(expected) < 1e-9,
        "second center = {second:?}, expected = {expected:?}"
    );
}

#[test]
fn for_loop_repeats() {
    let d = draw("for i = 1 to 3 do { box }");
    assert_eq!(d.shapes.len(), 3);
}

#[test]
fn for_loop_budget_preflights_before_body_parse() {
    let pic = parse("for i = 1 to 3 do { bxo }").unwrap();
    let err = eval_with_limits(
        &pic,
        EvalLimits {
            max_loop_iterations: 2,
            ..Default::default()
        },
    )
    .unwrap_err();

    assert!(err.msg.contains("for loop exceeded 2 iterations"), "{err}");
    assert!(!err.msg.contains("expected an object"), "{err}");
}

#[test]
fn shape_budget_caps_loop_expansion() {
    let pic = parse("for i = 1 to 3 do { box }").unwrap();
    let err = eval_with_limits(
        &pic,
        EvalLimits {
            max_loop_iterations: 10,
            max_shapes: 2,
            ..Default::default()
        },
    )
    .unwrap_err();

    assert!(err.msg.contains("drawing exceeded 2 shapes"), "{err}");
}

#[test]
fn for_loop_can_assign_subscripted_counter() {
    let d = draw("i = 1\nfor A[i] = 1 to 3 do { i += 1 }\nbox wid A[1] + A[2] + A[3] ht 0.3");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 6.0).abs() < 1e-9, "w = {w}");
}

#[test]
fn if_else_branches() {
    let d1 = draw("x = 1\nif x > 0 then { box } else { circle }");
    assert!(matches!(d1.shapes[0], Shape::Box { .. }));
    let d2 = draw("x = 0\nif x > 0 then { box } else { circle }");
    assert!(matches!(d2.shapes[0], Shape::Circle { .. }));
}

#[test]
fn define_macro_with_args() {
    let d = draw("define elem { box wid $1 }\nelem(0.5)\nelem(1.25)");
    assert_eq!(d.shapes.len(), 2);
    let Shape::Box { w: w0, .. } = &d.shapes[0] else {
        panic!()
    };
    let Shape::Box { w: w1, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((*w0 - 0.5).abs() < 1e-9 && (*w1 - 1.25).abs() < 1e-9);
}

#[test]
fn define_accepts_arbitrary_delimiter() {
    let d = draw("define elem / box /\nelem");
    assert_eq!(d.shapes.len(), 1);
    assert!(matches!(d.shapes[0], Shape::Box { .. }));
}

#[test]
fn labelled_call_to_multiline_body_macro() {
    // A multi-line `{ … }` body picks up newlines after `{` / before `}`.
    // They must not leak into a labelled call (`Q: m()` -> `Q: ⏎ <obj>`),
    // which would be a parse error. The block's terminals must still resolve.
    let d = draw(
        "define elem {\n  [\n    box wid 0.4 ht 0.2\n    L: last box.w\n    R: last box.e\n  ]\n}\nQ: elem() with .L at (1,1)\n\"x\" at Q.R",
    );
    // the block drew its box, and Q.R (a block sub-label) resolved for the text
    assert!(d.shapes.iter().any(|s| matches!(s, Shape::Box { .. })));
    let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    // Q placed with .L (west) at (1,1); .R (east) is one box-width to the right
    assert!(
        (at.x - 1.4).abs() < 1e-6 && (at.y - 1.0).abs() < 1e-6,
        "at = {at:?}"
    );
}

#[test]
fn copied_forward_macro_expands_in_deferred_multiline_call() {
    let dir = std::env::temp_dir().join(format!("rpic_forward_macro_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lib.pic"),
        "define outer { if gate then { inner(0.2,\n  0.3) } }\ndefine inner { box wid $1 ht $2 }\n",
    )
    .unwrap();

    let pic = parse_in_dir(
        "if 1 then { copy \"lib.pic\" }\ngate = 1\nouter()",
        Some(dir.as_path()),
    )
    .unwrap();
    let d = eval(&pic).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 0.2).abs() < 1e-9 && (*h - 0.3).abs() < 1e-9);
}

#[test]
fn deferred_body_uses_macro_frame_from_own_expansion() {
    let d = draw(
        "define draw_one { [\n  scalev = 2\n  define project { $1/scalev }\n  if use_it then { box wid project(4) ht 0.1 }\n] }\ndefine draw_two { define project { $1/future_scale } }\nuse_it = 1\ndraw_one()\ndraw_two()",
    );
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 2.0).abs() < 1e-9, "w = {w}");
}

#[test]
fn subscripted_variables_store_by_index() {
    let d = draw("P[1] = 0.4\nP[2] = 0.9\nP[2] += 0.1\nbox wid P[2] ht P[1]");
    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9 && (*h - 0.4).abs() < 1e-9);
}

#[test]
fn multidimensional_variables_store_by_index_tuple() {
    let d = draw("M[1,2] = 0.7\nj = 2\nM[1,j] += 0.2\nbox wid M[1,2] ht 0.3");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 0.9).abs() < 1e-9, "w = {w}");
}

#[test]
fn subscripted_label_places_resolve_by_index() {
    let d = draw("for i = 1 to 2 do { A[i]: circle rad 0.01 at (i,0) }\nline from A[1] to A[2]");
    let Shape::Path { pts, .. } = &d.shapes[2] else {
        panic!()
    };
    assert!(
        pts[0].dist(Point::new(1.0, 0.0)) < 1e-9,
        "start {:?}",
        pts[0]
    );
    assert!(pts[1].dist(Point::new(2.0, 0.0)) < 1e-9, "end {:?}", pts[1]);
}

#[test]
fn for_loop_places_in_a_row() {
    // boxes step right by default; bbox should be ~3*boxwid wide
    let d = draw("for i = 1 to 3 do { box; move }");
    assert!(d.bbox.width() > 2.0);
}

#[test]
fn animate_timing() {
    let d = draw(
        "A: box\narrow\nbox\nanimate A with \"fade\" for 0.5\nanimate last arrow with \"draw\"\nanimate 2nd box with \"pop\" after A",
    );
    assert_eq!(d.anims.len(), 3);
    // A: fade, start 0, dur 0.5, targets shape 0
    assert_eq!(d.anims[0].effect, "fade");
    assert_eq!(d.anims[0].shape, 0);
    assert!((d.anims[0].start).abs() < 1e-9 && (d.anims[0].duration - 0.5).abs() < 1e-9);
    // arrow: draw, sequential after A -> start 0.5, default dur 0.6, shape 1
    assert_eq!(d.anims[1].shape, 1);
    assert!((d.anims[1].start - 0.5).abs() < 1e-9);
    // 2nd box: pop, after A (ends at 0.5) -> start 0.5, shape 2
    assert_eq!(d.anims[2].shape, 2);
    assert!((d.anims[2].start - 0.5).abs() < 1e-9);
}

#[test]
fn animate_repeat_yoyo_ease() {
    let d = draw("box\nanimate last box with \"pop\" repeat 3 yoyo ease \"elastic.out(1, 0.3)\"");
    assert_eq!(d.anims.len(), 1);
    let a = &d.anims[0];
    assert_eq!(a.repeat, 3);
    assert!(a.yoyo);
    assert_eq!(a.ease.as_deref(), Some("elastic.out(1, 0.3)"));
    // No warning: yoyo is paired with a repeat.
    assert!(!d.warnings.iter().any(|w| w.kind == "yoyo_without_repeat"));
}

#[test]
fn animate_infinite_repeat_does_not_stall_sequence() {
    // An infinite loop must not push the next animation's start to infinity:
    // sequential timing tracks only the first iteration's end.
    let d = draw(
        "box\nbox\nanimate 1st box with \"fade\" for 0.4 repeat -1\nanimate 2nd box with \"pop\"",
    );
    assert_eq!(d.anims[0].repeat, -1);
    assert!((d.anims[1].start - 0.4).abs() < 1e-9);
}

#[test]
fn animate_yoyo_without_repeat_warns() {
    let d = draw("box\nanimate last box with \"fade\" yoyo");
    assert_eq!(d.anims[0].repeat, 0);
    assert!(d.anims[0].yoyo); // flag is still recorded, just inert in GSAP
    assert!(d.warnings.iter().any(|w| w.kind == "yoyo_without_repeat"));
}

#[test]
fn animate_defaults_leave_repeat_fields_inert() {
    let d = draw("box\nanimate last box with \"fade\"");
    let a = &d.anims[0];
    assert_eq!(a.repeat, 0);
    assert!(!a.yoyo);
    assert_eq!(a.ease, None);
    assert_eq!(a.path, None);
}

#[test]
fn animate_rejects_invalid_timing_values() {
    for (src, want) in [
        (
            "box\nanimate last box with \"fade\" for -1",
            "animation duration must be non-negative",
        ),
        (
            "box\nanimate last box with \"fade\" at -0.1",
            "animation start time must be non-negative",
        ),
        (
            "box\nanimate last box with \"fade\" delay -0.1",
            "animation delay must be non-negative",
        ),
        (
            "B: [ box; box ]\nanimate B with \"fade\" stagger -0.1",
            "animation stagger must be non-negative",
        ),
        (
            "box\nanimate last box with \"fade\" for 1e100",
            "animation duration must be at most",
        ),
        (
            "box\nanimate last box with \"fade\" repeat 1e100",
            "animation repeat must be at most",
        ),
        (
            "box\nanimate last box with \"fade\" repeat -2",
            "animation repeat must be -1 or non-negative",
        ),
    ] {
        let err = eval(&parse(src).unwrap()).unwrap_err();
        assert!(err.msg.contains(want), "{src}: {err}");
    }
}

#[test]
fn animate_source_limits_can_only_lower_defaults() {
    let err = eval(&parse("maxrepeats = 2\nbox\nanimate last box with \"fade\" repeat 3").unwrap())
        .unwrap_err();
    assert!(err.msg.contains("animation repeat must be at most 2"));

    let err = eval(&parse("maxanimseconds = 0.5\nbox\nanimate last box with \"fade\"").unwrap())
        .unwrap_err();
    assert!(err.msg.contains("animation duration must be at most 0.5"));
}

#[test]
fn animate_source_limits_reset_to_host_defaults() {
    let d = draw(
        "maxanimrepeat = 2\nreset maxanimrepeat\nbox\nanimate last box with \"fade\" repeat 3",
    );
    assert_eq!(d.anims[0].repeat, 3);

    let d = draw("maxanimseconds = 0.5\nreset maxanimseconds\nbox\nanimate last box with \"fade\"");
    assert_eq!(d.anims[0].duration, DEFAULT_ANIM_DUR);
}

#[test]
fn animate_rejects_invalid_source_limits() {
    for (src, want) in [
        ("maxanimrepeat = -1", "maxanimrepeat must be non-negative"),
        ("maxanimseconds = -1", "maxanimseconds must be non-negative"),
    ] {
        let err = eval(&parse(src).unwrap()).unwrap_err();
        assert!(err.msg.contains(want), "{src}: {err}");
    }
}

#[test]
fn animate_move_records_the_path_shape() {
    // The dot (shape 1) travels along the line (shape 0).
    let d = draw("L: line right 3\nD: dot at L.start\nanimate D with \"move\" along L for 2");
    assert_eq!(d.anims.len(), 1);
    let a = &d.anims[0];
    assert_eq!(a.effect, "move");
    assert_eq!(a.shape, 1);
    assert_eq!(a.path, Some(0));
    assert!((a.duration - 2.0).abs() < 1e-9);
    // `move` is a known effect: no unknown-effect warning.
    assert!(
        !d.warnings
            .iter()
            .any(|w| w.kind == "unknown_animation_effect")
    );
}

#[test]
fn animate_move_without_path_errors() {
    let err = eval(&parse("box\nanimate last box with \"move\"").unwrap()).unwrap_err();
    assert!(err.msg.contains("`move` needs a path"));
}

#[test]
fn animate_along_without_move_warns_and_is_dropped() {
    let d = draw("L: line right 2\nbox\nanimate last box with \"fade\" along L");
    assert!(d.warnings.iter().any(|w| w.kind == "along_without_move"));
    // `along` is ignored for non-move effects — no path leaks into the manifest.
    assert_eq!(d.anims[0].path, None);
}

#[test]
fn animate_highlight_resolves_colour_forms() {
    // Named colour passes through; rgb()/0xRRGGBB resolve to hex.
    let d = draw(
        "box\nbox\nbox\nanimate 1st box with \"highlight\" to \"crimson\"\nanimate 2nd box with \"highlight\" to rgb(255,140,0)\nanimate 3rd box with \"highlight\" to 0x1b5e20",
    );
    assert_eq!(d.anims[0].color.as_deref(), Some("crimson"));
    assert_eq!(d.anims[1].color.as_deref(), Some("#ff8c00"));
    assert_eq!(d.anims[2].color.as_deref(), Some("#1b5e20"));
    assert!(
        !d.warnings
            .iter()
            .any(|w| w.kind == "unknown_animation_effect")
    );
}

#[test]
fn animate_highlight_without_colour_is_allowed() {
    let d = draw("box\nanimate last box with \"highlight\" repeat 1 yoyo");
    assert_eq!(d.anims[0].effect, "highlight");
    assert_eq!(d.anims[0].color, None);
}

#[test]
fn animate_to_without_highlight_warns_and_is_dropped() {
    let d = draw("box\nanimate last box with \"fade\" to \"red\"");
    assert!(d.warnings.iter().any(|w| w.kind == "to_without_highlight"));
    // `to` is ignored for non-highlight effects — no colour in the manifest.
    assert_eq!(d.anims[0].color, None);
}

#[test]
fn animate_stagger_fans_across_block_children() {
    let d = draw(
        "B: [ box \"a\"; box \"b\"; box \"c\" ]\nanimate B with \"fade\" for 0.3 stagger 0.15",
    );
    assert_eq!(d.anims.len(), 3);
    assert_eq!(d.anims[0].shape, 0);
    assert_eq!(d.anims[1].shape, 1);
    assert_eq!(d.anims[2].shape, 2);
    assert!((d.anims[0].start - 0.0).abs() < 1e-9);
    assert!((d.anims[1].start - 0.15).abs() < 1e-9);
    assert!((d.anims[2].start - 0.3).abs() < 1e-9);
    assert!(d.anims.iter().all(|a| a.effect == "fade"));
}

#[test]
fn animate_stagger_skips_invisible_spines() {
    // The explicit `move`s between boxes are invisible: only the 3 boxes
    // (s0, s2, s4) get stagger slots — s1/s3 are skipped.
    let d = draw("B: [ box; move; box; move; box ]\nanimate B with \"pop\" stagger 0.1");
    assert_eq!(d.anims.len(), 3);
    assert_eq!(
        d.anims.iter().map(|a| a.shape).collect::<Vec<_>>(),
        vec![0, 2, 4]
    );
}

#[test]
fn animate_stagger_advances_the_sequence_past_the_last_child() {
    let d = draw(
        "B: [ box; box ]\ncircle\nanimate B with \"pop\" for 0.2 stagger 0.1\nanimate last circle with \"fade\"",
    );
    // children at 0.0 and 0.1 (dur 0.2) → last ends at 0.1+0.2 = 0.3.
    assert!((d.anims[2].start - 0.3).abs() < 1e-9);
    assert_eq!(d.anims[2].effect, "fade");
}

#[test]
fn animate_after_a_staggered_block_with_invisible_spine() {
    // `after <block>` resolves to the block's own shape index; the stagger
    // path must record the whole-stagger end there, even when the block
    // leads with an invisible spine (a `move`) so the first *visible* child
    // isn't the block's shape (audit).
    let d = draw(
        "B: [ move right 0.1; box; box ]\ncircle\n\
             animate B with \"pop\" for 0.2 stagger 0.1\n\
             animate last circle with \"fade\" after B",
    );
    let c = d.anims.iter().find(|a| a.effect == "fade").unwrap();
    // last staggered child ends at 0.1 + 0.2 = 0.3, not the first child's 0.2
    assert!((c.start - 0.3).abs() < 1e-9, "{}", c.start);
}

#[test]
fn animate_stagger_without_block_warns_and_animates_single() {
    let d = draw("box\nanimate last box with \"fade\" stagger 0.1");
    assert!(d.warnings.iter().any(|w| w.kind == "stagger_without_block"));
    assert_eq!(d.anims.len(), 1);
    assert_eq!(d.anims[0].shape, 0);
}

#[test]
fn animate_morph_records_the_target_shape() {
    // Box A (shape 0) morphs into circle B (shape 1).
    let d = draw("A: box\nB: circle at A+(2,0)\nanimate A with \"morph\" into B for 1");
    assert_eq!(d.anims.len(), 1);
    assert_eq!(d.anims[0].effect, "morph");
    assert_eq!(d.anims[0].shape, 0);
    assert_eq!(d.anims[0].morph, Some(1));
    assert!(
        !d.warnings
            .iter()
            .any(|w| w.kind == "unknown_animation_effect")
    );
}

#[test]
fn animate_morph_without_target_errors() {
    let err = eval(&parse("box\nanimate last box with \"morph\"").unwrap()).unwrap_err();
    assert!(err.msg.contains("`morph` needs a target"));
}

#[test]
fn animate_into_without_morph_warns_and_is_dropped() {
    let d = draw("A: box\nB: circle at A+(2,0)\nanimate A with \"fade\" into B");
    assert!(d.warnings.iter().any(|w| w.kind == "into_without_morph"));
    assert_eq!(d.anims[0].morph, None);
}

#[test]
fn animate_scroll_sets_the_timeline_hint() {
    let d = draw("box\nanimate last box with \"fade\"\nanimate scroll");
    assert!(d.anim_scroll);
    // it is a directive, not an object animation
    assert_eq!(d.anims.len(), 1);
}

#[test]
fn animate_scroll_defaults_off() {
    let d = draw("box\nanimate last box with \"fade\"");
    assert!(!d.anim_scroll);
}

#[test]
fn animate_slide_records_direction() {
    let d = draw("box\nanimate last box with \"slide\" from left for 0.4");
    assert_eq!(d.anims[0].effect, "slide");
    assert_eq!(d.anims[0].from.as_deref(), Some("left"));
    assert!(!d.anims[0].out);
}

#[test]
fn animate_slide_without_direction_errors() {
    let err = eval(&parse("box\nanimate last box with \"slide\"").unwrap()).unwrap_err();
    assert!(err.msg.contains("`slide` needs a direction"));
}

#[test]
fn animate_from_without_slide_warns_and_is_dropped() {
    let d = draw("box\nanimate last box with \"fade\" from up");
    assert!(d.warnings.iter().any(|w| w.kind == "from_without_slide"));
    assert_eq!(d.anims[0].from, None);
}

#[test]
fn animate_out_is_a_modifier_on_any_effect() {
    let d = draw("box\nbox\nanimate 1st box with \"fade\" out\nanimate 2nd box with \"pop\"");
    assert!(d.anims[0].out);
    assert!(!d.anims[1].out); // default is an entrance
}

#[test]
fn animate_slide_and_out_compose() {
    let d = draw("box\nanimate last box with \"slide\" from down out");
    assert_eq!(d.anims[0].from.as_deref(), Some("down"));
    assert!(d.anims[0].out);
}

#[test]
fn behind_sets_render_layer_without_changing_shape_indices() {
    let d = draw("A: box\nB: box behind A\nanimate B with \"fade\"");
    assert_eq!(d.shapes.len(), 2);
    assert_eq!(d.shape_layers, vec![0, -1]);
    assert_eq!(d.anims.len(), 1);
    assert_eq!(d.anims[0].shape, 1);
}

#[test]
fn behind_keeps_last_and_ordinals_semantic() {
    let d = draw("A: box at (0,0)\nB: box behind A at (2,0)\nline from last box.c to 1st box.c");
    let Shape::Path { pts, .. } = &d.shapes[2] else {
        panic!()
    };
    assert_eq!(pts.len(), 2);
    assert!(
        pts[0].dist(Point::new(2.0, 0.0)) < 1e-9,
        "start = {:?}",
        pts[0]
    );
    assert!(
        pts[1].dist(Point::new(0.0, 0.0)) < 1e-9,
        "end = {:?}",
        pts[1]
    );
}

#[test]
fn continue_extends_previous_line() {
    // issue #7: `continue` appends a segment to the last line (no new shape)
    let d = draw("line right 1\ncontinue down 0.5");
    assert_eq!(d.shapes.len(), 1, "should extend, not add a shape");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(pts.len(), 3);
    assert!(pts[2].dist(Point::new(1.0, -0.5)) < 1e-9, "{:?}", pts[2]);
    // bare continue extends in the current direction by linewid
    let d2 = draw("line right 1\ncontinue");
    let Shape::Path { pts, .. } = &d2.shapes[0] else {
        panic!()
    };
    assert!((pts.last().unwrap().x - 1.5).abs() < 1e-9);
}

#[test]
fn continue_rejects_closed_path() {
    let err =
        eval(&parse("line right then up then left close\ncontinue right").unwrap()).unwrap_err();
    assert!(err.msg.contains("polygon is closed"), "{err}");
}

#[test]
fn arc_from_to_endpoints_and_radius() {
    // issue #6: `arc from A to B` passes through both endpoints
    let d = draw("A:(0,0)\nB:(1,1)\narc from A to B");
    let Shape::Arc { c, r, a0, a1, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*r - 2.0_f64.sqrt() / 2.0).abs() < 1e-9, "r = {r}");
    let s = *c + Point::new(a0.cos(), a0.sin()) * *r;
    let e = *c + Point::new(a1.cos(), a1.sin()) * *r;
    assert!(s.dist(Point::new(0.0, 0.0)) < 1e-9, "start {s:?}");
    assert!(e.dist(Point::new(1.0, 1.0)) < 1e-9, "end {e:?}");

    let d_short = draw("arc from (0,0) to (0.3,0)");
    let Shape::Arc { r, .. } = &d_short.shapes[0] else {
        panic!()
    };
    assert!((*r - 0.25).abs() < 1e-9, "r = {r}");

    let d_custom = draw("arcrad = 0.5\narc from (0,0) to (0.3,0)");
    let Shape::Arc { r, .. } = &d_custom.shapes[0] else {
        panic!()
    };
    assert!((*r - 0.5).abs() < 1e-9, "r = {r}");

    // explicit radius is honored
    let d2 = draw("A:(0,0)\nB:(1,0)\narc from A to B rad 2");
    let Shape::Arc { r, .. } = &d2.shapes[0] else {
        panic!()
    };
    assert!((*r - 2.0).abs() < 1e-9, "r = {r}");
}

#[test]
fn arc_direction_disambiguates_from_to_radius() {
    let d = draw(
        "arc left from (0.5,0) to (0,0.5) rad 0.5\n\
             arc right from (0.5,0) to (0,0.5) rad 0.5 dashed",
    );
    let Shape::Arc {
        c: left,
        a0: left_a0,
        a1: left_a1,
        ..
    } = &d.shapes[0]
    else {
        panic!()
    };
    let Shape::Arc {
        c: right,
        a0: right_a0,
        a1: right_a1,
        ..
    } = &d.shapes[1]
    else {
        panic!()
    };

    assert!(left.dist(Point::new(0.0, 0.0)) < 1e-9, "left = {left:?}");
    assert!(right.dist(Point::new(0.5, 0.5)) < 1e-9, "right = {right:?}");
    assert!((left_a1 - left_a0).abs() < PI);
    assert!((right_a1 - right_a0).abs() > PI);
}

#[test]
fn arc_with_center_at_disambiguates_large_clockwise_sweep() {
    let d = draw("arc cw rad 1 from (0,-1) to (1,0) with .c at (0,0)");
    let Shape::Arc { c, r, a0, a1, .. } = &d.shapes[0] else {
        panic!()
    };
    let start = *c + Point::new(a0.cos(), a0.sin()) * *r;
    let end = *c + Point::new(a1.cos(), a1.sin()) * *r;

    assert!(c.dist(Point::ZERO) < 1e-9, "center = {c:?}");
    assert!((*r - 1.0).abs() < 1e-9, "r = {r}");
    assert!(
        start.dist(Point::new(0.0, -1.0)) < 1e-9,
        "start = {start:?}"
    );
    assert!(end.dist(Point::new(1.0, 0.0)) < 1e-9, "end = {end:?}");
    assert!(*a1 - *a0 < -PI, "sweep = {}", *a1 - *a0);
}

#[test]
fn arc_width_height_attrs_size_arrowheads() {
    let d = draw("arc <-> wid .5 ht .75");
    let Shape::Arc { style, .. } = &d.shapes[0] else {
        panic!()
    };

    assert!(
        (style.arrow_wid - 0.5).abs() < 1e-9,
        "wid = {}",
        style.arrow_wid
    );
    assert!(
        (style.arrow_ht - 0.75).abs() < 1e-9,
        "ht = {}",
        style.arrow_ht
    );
}

#[test]
fn scale_converts_user_units_to_inches() {
    // `scale = 2` means two user units per inch: defaults stay the same
    // physical size, while explicit dimensions and coordinates are halved.
    let d = draw("scale = 2\nbox");
    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (*w - 0.75).abs() < 1e-9 && (*h - 0.5).abs() < 1e-9,
        "{w} x {h}"
    );

    let d = draw("scale = 2\nbox wid 2 ht 1");
    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9 && (*h - 0.5).abs() < 1e-9);

    let d = draw("scale = 2\nline from (0,0) to (2,0)");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((pts.last().unwrap().x - 1.0).abs() < 1e-9, "{pts:?}");

    let d = draw("scale = 2\nA: (2,0)\nbox wid A.x ht .2");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9, "w = {w}");

    let d = draw("scale = 2\nbox wid 2 ht 1\nscale = 1\nbox wid 1 ht .5");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 2.0).abs() < 1e-9, "w = {w}");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.x - 2.5).abs() < 1e-9, "center = {c:?}");
}

#[test]
fn same_reuses_previous_dims() {
    // issue #4: `box same` reuses the previous box's dimensions
    let d = draw("box wid 1 ht 0.4 at 0,0\nbox same at 2,0");
    let Shape::Box { w, h, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(
        (*w - 1.0).abs() < 1e-9 && (*h - 0.4).abs() < 1e-9,
        "{w} x {h}"
    );
}

#[test]
fn same_reuses_previous_open_vector() {
    let d = draw("line up 1\nright\nline same");
    let Shape::Path { pts, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(pts[0].dist(Point::new(0.0, 1.0)) < 1e-9, "{pts:?}");
    assert!(pts[1].dist(Point::new(0.0, 2.0)) < 1e-9, "{pts:?}");
}

#[test]
fn spline_expr_is_tension_not_distance() {
    // issue #63: `spline <expr>` is a dpic tension parameter, not a bare
    // distance. The control polygon (and thus start/end) must be unchanged,
    // and the tension recorded.
    let d = draw("spline 0.5 from 0,0 to 1,1 to 2,0");
    let Shape::Spline { pts, tension, .. } = &d.shapes[0] else {
        panic!("expected a spline")
    };
    assert_eq!(*tension, Some(0.5));
    assert_eq!(pts.len(), 3, "tension must not add a segment: {pts:?}");
    assert!(pts[0].dist(Point::new(0.0, 0.0)) < 1e-9, "{pts:?}");
    assert!(pts[2].dist(Point::new(2.0, 0.0)) < 1e-9, "{pts:?}");
}

#[test]
fn spline_variable_tension_does_not_drift() {
    // The doc/spline.pic idiom: `for x … { spline x from 0,0 … }`. Each
    // tensioned spline must keep the same start/end as the untensioned one
    // (only the curvature changes), instead of `x` shifting the geometry.
    let plain = draw("spline from 0,0 up 1.5 then right 2 then down 1.5");
    let Shape::Spline {
        pts: p0,
        tension: t0,
        ..
    } = &plain.shapes[0]
    else {
        panic!()
    };
    assert_eq!(*t0, None);

    let tensioned = draw("x = 0.6\nspline x from 0,0 up 1.5 then right 2 then down 1.5");
    let Shape::Spline {
        pts: p1,
        tension: t1,
        ..
    } = &tensioned.shapes[0]
    else {
        panic!()
    };
    assert_eq!(*t1, Some(0.6));
    assert_eq!(p0.len(), p1.len());
    assert!(p0.first().unwrap().dist(*p1.first().unwrap()) < 1e-9);
    assert!(p0.last().unwrap().dist(*p1.last().unwrap()) < 1e-9);
}

#[test]
fn chop_trims_line_endpoints() {
    // issue #4: `chop` trims circlerad (0.25) off each end
    let d = draw("circle at 0,0\ncircle at 2,0\nline from 1st circle to 2nd circle chop");
    let Shape::Path { pts, .. } = &d.shapes[2] else {
        panic!()
    };
    assert!((pts[0].x - 0.25).abs() < 1e-9, "start {:?}", pts[0]);
    assert!(
        (pts.last().unwrap().x - 1.75).abs() < 1e-9,
        "end {:?}",
        pts.last()
    );

    let d = draw("line from (0,0) to (2,0) chop 0 chop .5");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((pts[0].x - 0.0).abs() < 1e-9, "{pts:?}");
    assert!((pts[1].x - 1.5).abs() < 1e-9, "{pts:?}");

    let d = draw("line from (0,0) to (2,0) chop .5 chop 0");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((pts[0].x - 0.5).abs() < 1e-9, "{pts:?}");
    assert!((pts[1].x - 2.0).abs() < 1e-9, "{pts:?}");

    let d = draw("line from (0,0) to (2,0) chop -.25 chop -.5");
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((pts[0].x + 0.25).abs() < 1e-9, "{pts:?}");
    assert!((pts[1].x - 2.5).abs() < 1e-9, "{pts:?}");
}

#[test]
fn chop_on_zero_length_line_is_ignored_like_dpic() {
    let plain = draw("line from (0,0) to (0,0)");
    let chopped = draw("line from (0,0) to (0,0) chop -0.1");
    assert_eq!(crate::to_svg(&chopped), crate::to_svg(&plain));

    let Shape::Path { pts, .. } = &chopped.shapes[0] else {
        panic!()
    };
    assert_eq!(pts, &[Point::ZERO, Point::ZERO]);
    assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));

    let extended = draw("line from (0,0) to (1,0) chop -0.1");
    let Shape::Path { pts, .. } = &extended.shapes[0] else {
        panic!()
    };
    assert!((pts[0].x + 0.1).abs() < 1e-9, "{pts:?}");
    assert!((pts[1].x - 1.1).abs() < 1e-9, "{pts:?}");
}

#[test]
fn unknown_variables_are_errors() {
    assert!(eval(&parse("box wid typo ht 0.2").unwrap()).is_err());
    assert!(eval(&parse("typo += 1").unwrap()).is_err());
}

#[test]
fn rand_advances_and_seed_repeats() {
    let mut st = State::new();
    let a = st.eval_expr(&Expr::Rand(None)).unwrap();
    let b = st.eval_expr(&Expr::Rand(None)).unwrap();
    assert!((0.0..1.0).contains(&a));
    assert!((0.0..1.0).contains(&b));
    assert_ne!(a, b);

    let seeded_a = st
        .eval_expr(&Expr::Rand(Some(Box::new(Expr::Num(1.0)))))
        .unwrap();
    let seeded_b = st
        .eval_expr(&Expr::Rand(Some(Box::new(Expr::Num(1.0)))))
        .unwrap();
    assert_eq!(seeded_a, seeded_b);
    assert!((seeded_a - 0.840_187_717).abs() < 1e-9, "{seeded_a}");
    let next = st.eval_expr(&Expr::Rand(None)).unwrap();
    assert!((next - 0.394_382_927).abs() < 1e-9, "{next}");
}

#[test]
fn arithmetic_matches_dpic_edge_cases() {
    assert_eq!(scalar("5.5 % 2").unwrap(), 0.0);
    assert_eq!(scalar("2.5 % 2").unwrap(), 1.0);
    assert_eq!(scalar("-2.5 % 2").unwrap(), -1.0);
    assert!(scalar("5 % 0.4").is_err());

    let mut st = State::new();
    st.eval_stmts(&parse("x = 5.5; x %= 2").unwrap().stmts)
        .unwrap();
    assert_eq!(st.vars["x"], 0.0);

    assert_eq!(scalar("sign(0)").unwrap(), 1.0);
    assert_eq!(scalar("sign(-0.1)").unwrap(), -1.0);

    assert_eq!(scalar("(-2)^3").unwrap(), -8.0);
    assert_eq!(scalar("(-2)^2").unwrap(), 4.0);
    assert_eq!(scalar("0^0").unwrap(), 1.0);
    assert!(scalar("(-2)^0.5").is_err());
    assert!(scalar("0^-1").is_err());
}

#[test]
fn long_allowed_binary_chain_evaluates_left_to_right() {
    let expr = (0..120).map(|_| "1").collect::<Vec<_>>().join("+");
    assert_eq!(scalar(&expr).unwrap(), 120.0);

    let mut st = State::new();
    st.eval_stmts(
        &parse("x = 0\ny = (x = x + 1) + (x = x + 10) + (x = x + 100)")
            .unwrap()
            .stmts,
    )
    .unwrap();
    assert_eq!(st.vars["x"], 111.0);
    assert_eq!(st.vars["y"], 123.0);
}

#[test]
fn standalone_text_occupies_invisible_box() {
    let d = draw("textwid = 1; textht = .2\n\"x\"\nbox wid .2 ht .2");
    let Shape::Text { at, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((at.x - 0.5).abs() < 1e-9, "text at {at:?}");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.x - 1.1).abs() < 1e-9, "box center {c:?}");
}

#[test]
fn text_position_modifies_the_preceding_string_only() {
    let d = draw("\"LLLL\" ljust");
    let Shape::Text { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].halign, -1);

    let d = draw("\"RRRR\" rjust");
    let Shape::Text { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].halign, 1);

    let d = draw("box wid 1 ht .6 \"AAAA\" above \"BBBB\" below");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].valign, 1);
    assert_eq!(text[1].valign, -1);

    let d = draw("box \"AAAA\" above \"BBBB\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].valign, 1);
    assert_eq!(text[1].valign, 0);
}

#[test]
fn fit_sizes_closed_objects_to_preceding_text() {
    let d = draw(
        "box \"wide label\" fit\n\
             ellipse \"one\" \"two\" \"three\" fit\n\
             circle \"wide label\" fit",
    );

    let Shape::Box { w, h, text, .. } = &d.shapes[0] else {
        panic!()
    };
    let (want_w, want_h) = fitted_text_size(text).unwrap();
    assert!((*w - want_w).abs() < 1e-9, "box w = {w}, want {want_w}");
    assert!((*h - want_h).abs() < 1e-9, "box h = {h}, want {want_h}");

    let Shape::Ellipse { w, h, text, .. } = &d.shapes[1] else {
        panic!()
    };
    let (want_w, want_h) = fitted_text_size(text).unwrap();
    assert!((*w - want_w).abs() < 1e-9, "ellipse w = {w}, want {want_w}");
    assert!((*h - want_h).abs() < 1e-9, "ellipse h = {h}, want {want_h}");

    let Shape::Circle { r, text, .. } = &d.shapes[2] else {
        panic!()
    };
    let (want_w, want_h) = fitted_text_size(text).unwrap();
    let want_r = want_w.hypot(want_h) / 2.0;
    assert!((*r - want_r).abs() < 1e-9, "circle r = {r}, want {want_r}");
}

#[test]
fn fit_respects_explicit_dimensions_and_text_order() {
    let d = draw("box wid 1 ht .2 \"very long label\" fit");
    assert_box_size(&d.shapes[0], 1.0, 0.2);

    let before = draw("box \"short\" fit");
    let after = draw("box \"short\" fit \"this later text does not affect fit\"");
    let Shape::Box {
        w: before_w,
        h: before_h,
        ..
    } = &before.shapes[0]
    else {
        panic!()
    };
    let Shape::Box {
        w: after_w,
        h: after_h,
        ..
    } = &after.shapes[0]
    else {
        panic!()
    };
    assert!((*before_w - *after_w).abs() < 1e-9);
    assert!((*before_h - *after_h).abs() < 1e-9);
}

#[test]
fn fit_without_preceding_visible_text_errors() {
    let err = eval(&parse("box fit").unwrap()).unwrap_err();
    assert!(err.msg.contains("visible text"), "{}", err.msg);

    let err = eval(&parse("box \"\" fit").unwrap()).unwrap_err();
    assert!(err.msg.contains("visible text"), "{}", err.msg);
}

#[test]
fn brace_draws_curly_annotation_between_points() {
    let d = draw("brace from (0,0) to (2,0) down \"n\" wid .25 bracepos .25");
    let Shape::Brace {
        a,
        b,
        cubics,
        label_at,
        text,
        ..
    } = &d.shapes[0]
    else {
        panic!()
    };
    assert!(a.dist(Point::new(0.0, 0.0)) < 1e-9, "a = {a:?}");
    assert!(b.dist(Point::new(2.0, 0.0)) < 1e-9, "b = {b:?}");
    assert_eq!(text[0].s, "n");
    assert!(label_at.y < -0.25, "label_at = {label_at:?}");
    assert!(
        (cubics[2][3].x - 0.5).abs() < 1e-9,
        "cusp = {:?}",
        cubics[2][3]
    );
    assert!(cubics[2][3].y < -0.2, "cusp = {:?}", cubics[2][3]);
    assert!(d.bbox.min.y < -0.25, "bbox = {:?}", d.bbox);
}

#[test]
fn brace_side_words_choose_absolute_side() {
    let up = draw("brace from (0,0) to (2,0) up wid .2");
    let down = draw("brace from (0,0) to (2,0) down wid .2");
    let Shape::Brace {
        label_at: up_label, ..
    } = &up.shapes[0]
    else {
        panic!()
    };
    let Shape::Brace {
        label_at: down_label,
        ..
    } = &down.shapes[0]
    else {
        panic!()
    };
    assert!(up_label.y > 0.0, "up label = {up_label:?}");
    assert!(down_label.y < 0.0, "down label = {down_label:?}");
}

#[test]
fn brace_labeloffset_moves_label_outward_from_cusp() {
    let base = draw("brace from (0,0) to (2,0) up \"n\" wid .25");
    let far = draw("brace from (0,0) to (2,0) up \"n\" wid .25 labeloffset .2");
    let Shape::Brace {
        label_at: base_label,
        ..
    } = &base.shapes[0]
    else {
        panic!()
    };
    let Shape::Brace {
        label_at: far_label,
        ..
    } = &far.shapes[0]
    else {
        panic!()
    };
    assert!(
        (far_label.y - base_label.y - 0.2).abs() < 1e-9,
        "base = {base_label:?}, far = {far_label:?}"
    );
}

#[test]
fn brace_compass_anchors_use_curve_bbox() {
    let d = draw(
        "B: brace from (0,0) to (2,0) up wid .25 bracepos .25\n\
             circle rad .01 at B.nw\n\
             circle rad .01 at B.ne\n\
             circle rad .01 at B.n\n\
             circle rad .01 at B.c",
    );
    let circles: Vec<Point> = d
        .shapes
        .iter()
        .skip(1)
        .map(|shape| {
            let Shape::Circle { c, .. } = shape else {
                panic!()
            };
            *c
        })
        .collect();

    assert!(circles[0].x.abs() < 1e-9, "nw = {:?}", circles[0]);
    assert!((circles[1].x - 2.0).abs() < 1e-9, "ne = {:?}", circles[1]);
    assert!(
        (circles[0].y - circles[1].y).abs() < 1e-9,
        "nw = {:?}, ne = {:?}",
        circles[0],
        circles[1]
    );
    assert!(
        (circles[2].x - 1.0).abs() < 1e-9 && circles[2].y > 0.2,
        "n = {:?}",
        circles[2]
    );
    assert!(
        (circles[3].x - 0.5).abs() < 1e-9 && circles[3].y > 0.2,
        "c = {:?}",
        circles[3]
    );
}

#[test]
fn brace_has_open_object_anchors_and_length() {
    let d = draw(
        "B: brace from (0,0) to (2,0) down wid .25\n\
             box wid (B.len) ht .1 at B.c\n\
             line from B.start to B.end",
    );
    assert_box_size(&d.shapes[1], 2.0, 0.1);
    let Shape::Path { pts, .. } = &d.shapes[2] else {
        panic!()
    };
    assert!(pts[0].dist(Point::new(0.0, 0.0)) < 1e-9);
    assert!(pts[1].dist(Point::new(2.0, 0.0)) < 1e-9);
}

#[test]
fn bracepos_must_be_inside_segment() {
    let err = eval(&parse("brace from (0,0) to (1,0) bracepos 1").unwrap()).unwrap_err();
    assert!(err.msg.contains("bracepos"), "{}", err.msg);
}

#[test]
fn style_globals_and_dash_lengths_apply() {
    let d = draw("linethick = 3\nline dashed .2\nline dotted .05");
    let Shape::Path { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.thick, Some(3.0));
    assert_eq!(style.dash, Dash::Dashed(0.2));
    let Shape::Path { style, .. } = &d.shapes[1] else {
        panic!()
    };
    assert_eq!(style.dash, Dash::Dotted(Some(0.05)));
}

#[test]
fn hatch_style_records_pattern_attributes() {
    let d = draw("box crosshatch hatchangle 30 hatchsep .05 hatchwidth 1.5 hatchcolor red");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    let hatch = style.hatch.as_ref().expect("expected hatch style");
    assert!(hatch.cross);
    assert!((hatch.angle - 30.0).abs() < 1e-9);
    assert!((hatch.sep - 0.05).abs() < 1e-9);
    assert!((hatch.width - 1.5).abs() < 1e-9);
    assert_eq!(hatch.color, "red");
    assert!(style.fill_open);
}

#[test]
fn opacity_style_records_fill_opacity() {
    let d = draw("box fill .8 opacity .4");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill_opacity, Some(0.4));
}

#[test]
fn block_opacity_multiplies_child_fill_opacity() {
    let d = draw("[ box opacity .5; circle ] opacity .5");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill_opacity, Some(0.25));
    let Shape::Circle { style, .. } = &d.shapes[1] else {
        panic!()
    };
    assert_eq!(style.fill_opacity, Some(0.5));
}

#[test]
fn opacity_must_be_between_zero_and_one() {
    let err = eval(&parse("box opacity 1.1").unwrap()).unwrap_err();
    assert!(err.msg.contains("opacity"), "{}", err.msg);
    let err = eval(&parse("box opacity -0.1").unwrap()).unwrap_err();
    assert!(err.msg.contains("opacity"), "{}", err.msg);
}

#[test]
fn standalone_text_rejects_opacity() {
    let err = eval(&parse("\"note\" opacity .5").unwrap()).unwrap_err();
    assert!(err.msg.contains("filled regions"), "{}", err.msg);
}

#[test]
fn color_attribute_expands_runtime_macro_string() {
    let d = draw(
        "r = 0; g = 0; b = 0.6\n\
             if dpicopt == optSVG then {\n\
               define customcolor { sprintf(\"rgb(%g,%g,%g)\", int(r*255), int(g*255), int(b*255)) }\n\
             }\n\
             arc color customcolor",
    );
    let Shape::Arc { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.stroke.as_deref(), Some("rgb(0,0,153)"));
    assert_eq!(style.fill, Some(Fill::Color("rgb(0,0,153)".into())));
}

#[test]
fn color_attribute_accepts_dpictools_rgbstring_macro_call() {
    let d = draw(
        "if dpicopt == optSVG then {\n\
               define rgbstring { sprintf(\"rgb(%g,%g,%g)\", int(($1)*255+0.5), int(($2)*255+0.5), int(($3)*255+0.5)) }\n\
             }\n\
             circle shaded rgbstring(1,0.84,0) outlined \"black\"",
    );
    let Shape::Circle { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("rgb(255,214,0)".into())));
    assert_eq!(style.stroke.as_deref(), Some("black"));
}

#[test]
fn ps_width_scales_drawing() {
    // issue #4, dpic oracle: `.PS 6` scales the painted picture, so the
    // box geometry is slightly under 6in once default stroke is reserved.
    let d = draw(".PS 6\nbox\n.PE");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 5.912_408_759).abs() < 1e-9, "w = {w}");
    assert!(
        (d.bbox.width() - 5.923_519_870).abs() < 1e-9,
        "w = {}",
        d.bbox.width()
    );
}

#[test]
fn text_extent_in_bbox() {
    // issue #5: a bare label must yield a non-degenerate bbox (no clipping)
    let d = draw("\"a long label here\"");
    assert!(d.bbox.width() > 0.5, "w = {}", d.bbox.width());
    assert!(d.bbox.height() > 0.1, "h = {}", d.bbox.height());
    // text wider than its box widens the bbox beyond the box
    let d2 = draw("box wid 0.2 ht 0.2 \"a very wide label\"");
    assert!(d2.bbox.width() > 0.3, "w = {}", d2.bbox.width());
}

#[test]
fn text_object_width_bounds_rendered_bbox() {
    // dpic oracle: a standalone text object's `wid` controls its bbox;
    // the literal text width is not used when an explicit width is given.
    let d = draw("\"abcdefghij\" wid 0.1");
    assert!(
        (d.bbox.width() - 0.1).abs() < 1e-9,
        "w = {}",
        d.bbox.width()
    );

    let d = draw(".PS 1\n\"abcdefghij\" wid 0.1\n.PE");
    assert!(
        (d.bbox.width() - 1.0).abs() < 1e-9,
        "w = {}",
        d.bbox.width()
    );
}

#[test]
fn text_position_and_offset_expand_bbox_in_the_rendered_direction() {
    let d = draw("textoffset = 0.1\n\"abc\" ljust at (0,0)");
    assert!(d.bbox.min.x >= 0.1 - 1e-9, "{:?}", d.bbox);

    let d = draw("textoffset = 0.1\n\"abc\" rjust at (0,0)");
    assert!(d.bbox.max.x <= -0.1 + 1e-9, "{:?}", d.bbox);

    let d = draw("textoffset = 0.1\n\"abc\" above at (0,0)");
    assert!(d.bbox.min.y > 0.0, "{:?}", d.bbox);

    let d = draw("textoffset = 0.1\n\"abc\" below at (0,0)");
    assert!(d.bbox.max.y < 0.0, "{:?}", d.bbox);
}

#[test]
fn invisible_geometry_does_not_expand_drawing_bbox() {
    let d = draw("box invis wid 1000 ht 1000 at (0,0)\nbox wid 1 ht 1 at (0,0)");
    assert!(
        (d.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "w = {}",
        d.bbox.width()
    );
    assert!(
        (d.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "h = {}",
        d.bbox.height()
    );

    let d2 = draw("line invis from (0,0) to (1000,1000)\nbox wid 1 ht 1 at (0,0)");
    assert!(
        (d2.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "w = {}",
        d2.bbox.width()
    );
    assert!(
        (d2.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "h = {}",
        d2.bbox.height()
    );

    let d3 = draw("I: box invis wid 1000 ht 1000 at (0,0)\nbox wid 1 ht 1 with .sw at I.ne");
    let Shape::Box { c, .. } = &d3.shapes[1] else {
        panic!()
    };
    assert!(c.dist(Point::new(500.5, 500.5)) < 1e-9, "c = {c:?}");
    assert!(
        (d3.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "w = {}",
        d3.bbox.width()
    );
    assert!(
        (d3.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
        "h = {}",
        d3.bbox.height()
    );
}

#[test]
fn move_expands_drawing_bbox_like_dpic() {
    let d = draw("line from (0,0) to (1,0)\nmove left 0.4 from (0,0)");
    assert!(
        (d.bbox.width() - (1.4 + DEFAULT_STROKE_IN / 2.0)).abs() < 1e-9,
        "w = {}",
        d.bbox.width()
    );
}

#[test]
fn division_by_zero_errors() {
    // a zero divisor must error rather than silently produce NaN coordinates
    assert!(eval(&parse("box wid 1/0").unwrap()).is_err());
    assert!(eval(&parse("A:(0,0)\nB:(0,0)\nx = (B.x-A.x)/(B.y-A.y)").unwrap()).is_err());
}

#[test]
fn non_finite_numeric_values_error() {
    let literal = parse("box wid 1e999 ht 1").unwrap_err();
    assert!(literal.msg.contains("not finite"), "{literal}");

    for src in [
        "box wid exp(1000) ht 1",
        "box wid sqrt(-1) ht 1",
        "scale = exp(1000)\nbox",
        "x = 1e308\nx *= 1e308\nbox wid x",
    ] {
        let err = eval(&parse(src).unwrap()).unwrap_err();
        assert!(err.msg.contains("non-finite"), "{src}: {err}");
    }
}

#[test]
fn place_dot_in_coordinate_pair() {
    // (A.x, A.y - 1) — place scalar accessors inside a coordinate pair (issue #3)
    let d = draw("A: box wid 1 ht 1 at 2,3\nbox wid 0.2 ht 0.2 at (A.x, A.y - 1)");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(
        (c.x - 2.0).abs() < 1e-9 && (c.y - 2.0).abs() < 1e-9,
        "c = {c:?}"
    );
}

#[test]
fn bare_coordinate_pair_places_label() {
    let d = draw("P: 1,2\nbox wid 0.2 ht 0.2 at P");
    let Shape::Box { c, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(c.dist(Point::new(1.0, 2.0)) < 1e-9, "c = {c:?}");
}

#[test]
fn block_sub_labels_resolve() {
    // `B.A` and `B.A.corner` reach a labelled object inside a block
    let d = draw("B: [ A: box wid 1 ht 1 at 0,0 ]\nbox wid 0.2 ht 0.2 with .sw at B.A.ne");
    let Shape::Box { c, .. } = &d.shapes.last().unwrap() else {
        panic!()
    };
    // the block is placed with its center at (0.5,0); inner A.ne is then
    // (1.0,0.5), and the small box centers 0.1 beyond that corner.
    assert!(
        (c.x - 1.1).abs() < 1e-9 && (c.y - 0.6).abs() < 1e-9,
        "c = {c:?}"
    );
}

#[test]
fn block_can_anchor_on_own_member() {
    // The block bbox center is not A.c, so this catches the two-pass anchor
    // resolution rather than accidentally aligning the block center.
    let d = draw("P:(2,3)\n[ A: box wid 1 ht 1 at 0,0; circle rad 0.1 at 2,0 ] with .A.c at P");
    let Shape::Box { c, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(c.dist(Point::new(2.0, 3.0)) < 1e-9, "A center = {c:?}");
    let Shape::Circle { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(c.dist(Point::new(4.0, 3.0)) < 1e-9, "circle center = {c:?}");
}

#[test]
fn block_pair_anchor_uses_local_coordinates() {
    // dpic oracle: for `[ ... ] with (x,y) at P`, `(x,y)` is a point in
    // the block's local coordinate system, not an offset from its center.
    let d = draw("[ box wid 2 ht 1 at (1,0) ] with (0,0) at (10,20)");
    let Shape::Box { c, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(c.dist(Point::new(11.0, 20.0)) < 1e-9, "c = {c:?}");
}

#[test]
fn block_layout_matches_dpic_for_negative_box_width() {
    let d = draw("move 1\n[ box wid -0.5 ht 0.5 ]; box wid 0.75 ht 0.75");
    let boxes: Vec<Point> = d
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Box { c, .. } => Some(*c),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 2);
    assert!(
        boxes[0].dist(Point::new(0.75, 0.0)) < 1e-9,
        "negative box center = {:?}",
        boxes[0]
    );
    assert!(
        boxes[1].dist(Point::new(1.375, 0.0)) < 1e-9,
        "following box center = {:?}",
        boxes[1]
    );
}

#[test]
fn block_object_renders_attached_text() {
    let d = draw("[ box ] \"block label\"");
    assert!(d.shapes.iter().any(|s| {
        matches!(
            s,
            Shape::Text { text, .. } if text.iter().any(|line| line.s == "block label")
        )
    }));
}

#[test]
fn block_anchors_ignore_attached_text_extents() {
    // dpic oracle: text contributes to the painted bbox, but not to block
    // anchors such as `last [].s`; those come from the geometric objects.
    let d = draw(
        r#"B: [ right; box "{\bf veryveryverywide}"; move; box ]
box wid 0.1 ht 0.1 at B.s"#,
    );
    let Shape::Box { c, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!(c.dist(Point::new(1.0, -0.25)) < 1e-9, "c = {c:?}");
}

#[test]
fn nested_macro_block_can_reference_parent_label() {
    let d = draw(
        "define marker { [ P: circle rad 0.01 at $1.start ] with .P at $1.start }\n[ A: arrow from (0,0) to (1,0); marker(A) ]",
    );
    let Shape::Path { pts, .. } = &d.shapes[0] else {
        panic!()
    };
    let Shape::Circle { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!(
        c.dist(pts[0]) < 1e-9,
        "circle = {c:?}, arrow start = {:?}",
        pts[0]
    );
}

#[test]
fn position_vector_arithmetic() {
    // (w,h)/2 and p + q with correct precedence
    let d = draw("box wid 0.2 ht 0.2 at (2,4)/2 + (1,0)");
    let Shape::Box { c, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (c.x - 2.0).abs() < 1e-9 && (c.y - 2.0).abs() < 1e-9,
        "c = {c:?}"
    );
}

#[test]
fn interpolation_angle_brackets() {
    let d = draw("A:(0,0)\nB:(2,0)\nbox wid 0.1 ht 0.1 at 0.5 <A,B>");
    let Shape::Box { c, .. } = &d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((c.x - 1.0).abs() < 1e-9, "c = {c:?}");
}

#[test]
fn string_equality_in_if() {
    // the `"$1"==""` default-argument idiom (here without a macro)
    let d1 = draw("if \"a\" == \"\" then { box } else { circle }");
    assert!(matches!(d1.shapes[0], Shape::Circle { .. }));
    let d2 = draw("if \"\" == \"\" then { box } else { circle }");
    assert!(matches!(d2.shapes[0], Shape::Box { .. }));
}

#[test]
fn dpicopt_defaults_to_svg_backend() {
    let d = draw("if dpicopt == optSVG then { box } else { circle }");
    assert!(matches!(d.shapes[0], Shape::Box { .. }));
    // absolute values are ONE-based, oracle-checked against dpic
    // (`dpic -v` prints dpicopt=9, optMFpic=1 … optxfig=12): the dpic
    // suite's `case(dpicopt, …)` dispatch depends on them.
    assert_eq!(scalar("dpicopt").unwrap(), 9.0);
    assert_eq!(scalar("optMFpic").unwrap(), 1.0);
    assert_eq!(scalar("optPSTricks").unwrap(), 8.0);
    assert_eq!(scalar("optxfig").unwrap(), 12.0);
}

#[test]
fn exec_defines_persist_after_the_exec() {
    // a `define` inside exec'd text must register in the caller's macro
    // table (dpic behaviour) — it used to land in a discarded clone
    let d = draw("exec \"define Custom {\\\"#00ff00\\\"}\"\nbox shaded Custom");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#00ff00".into())));
}

#[test]
fn dpic_suite_define_rgb_color_machinery_works() {
    // the dpic test suite's DefineRGBColor: case() exec-dispatches on
    // dpicopt into a nested define of the colour macro — end to end,
    // Custom must resolve to the same rgb() string dpic -v emits
    let d = draw(concat!(
        "define case { exec sprintf(\"$%g\",floor($1+0.5)+1); }\n",
        "define DefineRGBColor { case(dpicopt,\n",
        "  A , B , C , D , E , F , G , H ,\n",
        "  define $1 {sprintf(\"rgb(%g,%g,%g)\",int($2*255),int($3*255),int($4*255))} ,\n",
        "  I , J , K ) }\n",
        "DefineRGBColor(Custom,0.8,0.7,0.6)\n",
        "box shaded Custom",
    ));
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("rgb(204,178,153)".into())));
    assert!(!d.warnings.iter().any(|w| w.kind == "invalid_color"));
}

#[test]
fn static_exec_arg_splice_relexes_cleanly() {
    // #280: a static `$N` inside an exec string glued the argument's
    // tokens (`box shaded "#00ff00"` → `boxshaded#00ff00`) and dropped
    // the quotes that keep `#…` from starting a comment
    let d = draw(
        "define pick { exec \"$2\" }\n\
             pick( box shaded \"#ff0000\" , box shaded \"#00ff00\" )",
    );
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#00ff00".into())));
    // keywords in the argument used to be silently dropped
    let d = draw("define pick { exec \"$1\" }\npick( box thick 3 )");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((style.thick.unwrap() - 3.0).abs() < 1e-9);
}

#[test]
fn string_splice_keeps_source_spacing() {
    // spans drive the spacing: tokens adjacent in the source stay glued …
    let d = draw("define lbl { box \"$1\" }\nlbl(2L)");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "2L");
    // … and tokens the source separates keep their gap (an unquoted TeX
    // label like `$\beta V$` used to collapse into `$\betaV$`)
    let d = draw("define lbl { box \"$1\" }\nlbl($\\beta V$)");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "$\\beta V$");
    // a lone quoted argument still splices as bare content (the classic
    // quote-at-use-site label idiom)
    let d = draw("define lbl { box \"$1\" }\nlbl(\"hello\")");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "hello");
    // a token that renders shorter than its source span (a normalized
    // float `1.50`→`1.5`) must not skew the gap test into spurious spaces
    // — spacing comes from the source spans, not rendered lengths
    let d = draw("define lbl { box \"$1\" }\nlbl((1.50,2.50))");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "(1.5,2.5)");
}

#[test]
fn svg_font_stub_and_string_sprintf_are_harmless() {
    let d = draw("box sprintf(\"x%s\", svg_font(\"Times\", 12))");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "x");
}

#[test]
fn sprintf_precision_is_clamped() {
    // #284: an enormous precision must not panic/OOM — it clamps to 512
    // digits and the result is finite
    let d = draw("box sprintf(\"%.999999999f\", 1)");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s.len(), "1.".len() + 512);
    // ordinary precision is untouched
    let d = draw("box sprintf(\"%.2f\", 3.14159)");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "3.14");
}

#[test]
fn inch_suffix_and_bare_distance() {
    // `.5i` is half an inch; `move 1` / `move -0.1` advance the pen
    let d = draw("box wid .5i ht .5i");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 0.5).abs() < 1e-9, "w = {w}");
    let d2 = draw("right\nmove 1\nbox at Here");
    let Shape::Box { c, .. } = &d2.shapes.last().unwrap() else {
        panic!()
    };
    assert!(c.x > 0.9, "moved to {c:?}");
}

#[test]
fn embedded_assignment_returns_value() {
    let d = draw("if (s = 3) > 1 then { box wid s ht 0.1 }");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 3.0).abs() < 1e-9, "w = {w}");
}

#[test]
fn copy_includes_a_file() {
    // `copy "file"` splices another pic file relative to the base directory
    let dir = std::env::temp_dir().join(format!("rpic_copy_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("inc.pic"), "box wid 0.5 ht 0.5\n").unwrap();
    let pic = parse_in_dir("copy \"inc.pic\"\ncircle", Some(dir.as_path())).unwrap();
    let d = eval(&pic).unwrap();
    assert!(d.shapes.iter().any(|s| matches!(s, Shape::Box { .. })));
    assert!(d.shapes.iter().any(|s| matches!(s, Shape::Circle { .. })));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paren_position_coordinate() {
    // `.x`/`.y` on a parenthesised position expression
    let d = draw("A:(0,0)\nB:(3,1)\nbox wid (B-A).x ht (B-A).y at 0,-2");
    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (*w - 3.0).abs() < 1e-9 && (*h - 1.0).abs() < 1e-9,
        "{w} x {h}"
    );
}

#[test]
fn arrowhead_size_follows_globals() {
    // arrowht/arrowwid control the rendered arrowhead, not a hardcoded size
    let d = draw("arrowht = 0.3; arrowwid = 0.2\narrow right 1");
    let Shape::Path { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (style.arrow_ht - 0.3).abs() < 1e-9,
        "ht = {}",
        style.arrow_ht
    );
    assert!(
        (style.arrow_wid - 0.2).abs() < 1e-9,
        "wid = {}",
        style.arrow_wid
    );
}

#[test]
fn scaling_existing_geometry_scales_arrowhead_metadata() {
    // manual/man35 draws at a temporary scale, then restores `scale`.
    // The points are scaled at restore time, and the arrowhead dimensions
    // attached to the already-emitted path must scale with them.
    let factor = 6.6 / 8.2;
    let d = draw("scale = 6.6/8.2\nline <-\nscale = 1");
    let Shape::Path { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (style.arrow_ht - 0.1 * factor).abs() < 1e-9,
        "ht = {}",
        style.arrow_ht
    );
    assert!(
        (style.arrow_wid - 0.05 * factor).abs() < 1e-9,
        "wid = {}",
        style.arrow_wid
    );
}

#[test]
fn dpic_default_env_values_are_readable() {
    assert!((scalar("textoffset").unwrap() - 2.0 / 72.0).abs() < 1e-9);
    assert!((scalar("textht").unwrap() - (11.0 / 72.0) * 0.66).abs() < 1e-9);
    assert!((scalar("arrowhead").unwrap() - 1.0).abs() < 1e-9);
    assert!((scalar("linethick").unwrap() - 0.8).abs() < 1e-9);
    assert_eq!(scalar("margin").unwrap(), 0.0);
    assert_eq!(scalar("topmargin").unwrap(), 0.0);
    assert_eq!(scalar("rightmargin").unwrap(), 0.0);
    assert_eq!(scalar("bottommargin").unwrap(), 0.0);
    assert_eq!(scalar("leftmargin").unwrap(), 0.0);
    assert_eq!(
        scalar("maxanimrepeat").unwrap(),
        DEFAULT_MAX_ANIMATION_REPEAT as f64
    );
    assert_eq!(
        scalar("maxanimseconds").unwrap(),
        DEFAULT_MAX_ANIMATION_SECONDS
    );
}

#[test]
fn font_attrs_bind_per_string_like_ljust() {
    // after the string: binds to the preceding one
    let d = draw("box \"a\" bold \"b\" italic fontsize 9");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(text[0].bold && !text[0].italic && text[0].size_pt.is_none());
    assert!(text[1].italic && !text[1].bold && text[1].size_pt == Some(9.0));
    // before any string: binds to the next one only
    let d = draw("box bold \"a\" \"b\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(text[0].bold && !text[1].bold);
    // mono and font "…" set the family
    let d = draw("box \"a\" mono \"b\" font \"Georgia\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].family.as_deref(), Some("monospace"));
    assert_eq!(text[1].family.as_deref(), Some("Georgia"));
}

#[test]
fn font_attrs_feed_fit_and_bbox() {
    let plain = draw("box \"word\" fit");
    let bold = draw("box \"word\" bold fit");
    let big = draw("box \"word\" fontsize 22 fit");
    assert!(bold.bbox.width() > plain.bbox.width());
    assert!(big.bbox.width() > bold.bbox.width());
    assert!(big.bbox.height() > plain.bbox.height());
    // standalone text: default height follows the fontsize ratio
    let plain = draw("\"word\"");
    let big = draw("\"word\" fontsize 22");
    assert!(big.bbox.height() > 1.5 * plain.bbox.height());
}

#[test]
fn font_attrs_reject_bad_sizes() {
    let e = eval(&parse("box \"x\" fontsize 0").unwrap()).unwrap_err();
    assert!(e.msg.contains("positive number of points"), "{}", e.msg);
    let e = eval(&parse("box \"x\" fontsize -3").unwrap()).unwrap_err();
    assert!(e.msg.contains("positive number of points"), "{}", e.msg);
}

#[test]
fn font_attr_words_stay_usable_as_variables() {
    // `bold`/`italic`/`mono` only act in attribute position; as plain
    // variables in expressions they keep their classic meaning
    let d = draw("bold = 2\nbox wid bold \"x\"");
    assert!((d.bbox.width() - 2.0).abs() < 0.05, "{}", d.bbox.width());
}

#[test]
fn with_start_end_edge_aligns_closed_objects() {
    // #240: `with .start at X` / `with .end at X` edge-align, not center.
    // right dir: B.start (its .w) on A.end=(1,0) → B centered at 1.2
    let d = draw("A: box wid 1 ht 0.5\nB: box wid 0.4 ht 0.4 with .start at A.end");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.x - 1.2).abs() < 1e-9, "B.c.x = {}", c.x);
    // .end anchor: B.end (its .e) on A.start=(1.5,0) → B centered at 1.3
    let d = draw("A: box wid 1 ht 0.5 at (2,0)\nB: box wid 0.4 ht 0.4 with .end at A.start");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.x - 1.3).abs() < 1e-9, "B.c.x = {}", c.x);
    // vertical up: .start maps to the south edge → B centered at y=0.7
    let d = draw("up\nA: box wid 0.5 ht 1 at (0,0)\nB: box wid 0.4 ht 0.4 with .start at A.end");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.y - 0.7).abs() < 1e-9, "B.c.y = {}", c.y);
    // .c anchor unchanged: B centered on A.e=(1,0)
    let d = draw("A: box wid 1 ht 0.5\nB: box wid 0.4 with .c at A.e");
    let Shape::Box { c, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((c.x - 1.0).abs() < 1e-9, "B.c.x = {}", c.x);
}

#[test]
fn previous_is_a_synonym_for_last() {
    // #240: pikchr `previous` == `last`
    let a = crate::to_svg(&draw("box\ncircle at previous.e rad 0.1"));
    let b = crate::to_svg(&draw("box\ncircle at last.e rad 0.1"));
    assert_eq!(a, b);
    // `previous box`, `2nd previous box` parse and resolve
    assert!(eval(&parse("box; box\ncircle at previous box.n").unwrap()).is_ok());
    assert!(eval(&parse("box; box; box\ncircle at 2nd previous box.n").unwrap()).is_ok());
}

#[test]
fn aligned_rotates_label_to_segment_angle() {
    // #240: aligned sets the label rotation to the segment angle, readable
    let d = draw("line from (0,0) to (2,2) \"up\" aligned");
    let Shape::Path { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (text[0].rotate.unwrap() - 45.0).abs() < 1e-6,
        "{:?}",
        text[0].rotate
    );
    // horizontal → no rotation (upright, byte-identical to a plain label)
    let d = draw("line right 2 \"flat\" aligned");
    let Shape::Path { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].rotate, None);
    // leftward → normalized to stay readable (not 180)
    let d = draw("line from (2,0) to (0,0) \"back\" aligned");
    let Shape::Path { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].rotate, None); // 180 → 0 (upright)
}

#[test]
fn big_small_size_labels() {
    // #240: pikchr big/small sugar over fontsize (1.5× / 0.7× of 11pt)
    let d = draw("box \"a\" big \"b\" small");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].size_pt, Some(16.5));
    assert!((text[1].size_pt.unwrap() - 7.7).abs() < 1e-9);
    // no ignored_attribute warning
    let d = draw("box \"a\" big");
    assert!(d.warnings.is_empty());
}

#[test]
fn rotated_binds_per_string_and_grows_fit() {
    let d = draw("box \"a\" rotated 45 \"b\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].rotate, Some(45.0));
    assert_eq!(text[1].rotate, None);
    // a rotated label needs a taller fit box
    let plain = draw("box \"long caption\" fit");
    let rot = draw("box \"long caption\" rotated 30 fit");
    assert!(rot.bbox.height() > plain.bbox.height());
    // standalone rotated text: canvas covers the rotated extent
    let plain = draw("\"long caption text\"");
    let rot = draw("\"long caption text\" rotated 90");
    assert!(rot.bbox.height() > 2.0 * plain.bbox.height());
}

#[test]
fn color_literals_evaluate_to_hex() {
    let d = draw("box shaded rgb(27,94,32)");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
    let d = draw("box shaded 0x1B5E20");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
    // expressions inside rgb()
    let d = draw("v = 200\nbox shaded rgb(v, v/2, 0)");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#c86400".into())));
}

#[test]
fn color_literals_reject_out_of_range() {
    let e = eval(&parse("box shaded rgb(300,0,0)").unwrap()).unwrap_err();
    assert!(e.msg.contains("0-255"), "{}", e.msg);
    let e = eval(&parse("box shaded 0x1FFFFFF").unwrap()).unwrap_err();
    assert!(e.msg.contains("0-0xFFFFFF"), "{}", e.msg);
}

#[test]
fn xcolor_names_resolve_to_their_rgb() {
    // a dvips name browsers can't render maps to its dvipsnam.def RGB …
    let d = draw("box shaded \"Dandelion\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#ffb529".into())));
    assert!(!d.warnings.iter().any(|w| w.kind == "invalid_color"));
    // … while an xcolor name that is ALSO a CSS keyword stays as written
    // (browsers already render it; the dvips value differs)
    let d = draw("box shaded \"Goldenrod\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("Goldenrod".into())));
}

#[test]
fn unknown_colour_name_warns_valid_stays_quiet() {
    // a typo / unknown colour name is flagged (with a suggestion) …
    let d = draw("box shaded \"crimsom\"");
    let w = d
        .warnings
        .iter()
        .find(|w| w.kind == "invalid_color")
        .expect("expected an invalid_color warning");
    assert!(w.hint.as_deref().unwrap_or("").contains("crimson"), "{w:?}");
    // … but the shape still renders with the passed-through colour (advisory)
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("crimsom".into())));
    // valid CSS, xcolor, hex, rgb() and resolved variables stay quiet
    for src in [
        "box shaded \"crimson\"",
        "box shaded \"Dandelion\"",
        "box shaded \"#1b5e20\"",
        "box outlined \"rgb(1,2,3)\"",
        "c = 0x2f855a\nbox shaded c",
    ] {
        let d = draw(src);
        assert!(
            !d.warnings.iter().any(|w| w.kind == "invalid_color"),
            "unexpected invalid_color for `{src}`: {:?}",
            d.warnings
        );
    }
}

#[test]
fn color_attributes_reject_svg_paint_urls_and_css_functions() {
    for (src, want) in [
        ("box shaded \"url(https://example.invalid/p.svg#x)\"", "url"),
        ("box outlined \"url (#stroke)\"", "url"),
        ("box hatch hatchcolor \"url(#hatch)\"", "url"),
        ("box gradient \"white\" \"url(#grad)\"", "url"),
        ("box shaded \"var(--pic-fill)\"", "CSS variables"),
        (
            "box shaded \"linear-gradient(red, blue)\"",
            "unsupported CSS colour function",
        ),
        (
            "box shaded \"-webkit-linear-gradient(red, blue)\"",
            "unsupported CSS colour function",
        ),
    ] {
        let err = eval(&parse(src).unwrap()).unwrap_err();
        assert!(err.msg.contains(want), "{src}: {}", err.msg);
    }
}

#[test]
fn color_literal_words_stay_classic_elsewhere() {
    // `rotated` as a variable; `rgb` as a macro name; quoted colors as before
    let d = draw("rotated = 2\nbox wid rotated");
    assert!((d.bbox.width() - 2.0).abs() < 0.05);
    let d = draw("define rgb { box }\nrgb");
    assert_eq!(d.shapes.len(), 1);
    let d = draw("box shaded \"#1b5e20\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
}

#[test]
fn hex_number_literals_lex_as_numbers() {
    // 0x literals are plain numbers everywhere, not just colors
    let d = draw("box wid 0x2 ht 0x1");
    assert!((d.bbox.width() - 2.0).abs() < 0.05, "{}", d.bbox.width());
}

#[test]
fn color_from_variable_and_expression() {
    // a colour held in a variable resolves (previously emitted as the raw
    // name, e.g. `stroke="c"`, which no renderer understands)
    let d = draw("c = 0x2f855a\nbox shaded c");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#2f855a".into())));
    // outlined + variable
    let d = draw("c = 0x123456\nbox outlined c");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.stroke.as_deref(), Some("#123456"));
    // a parenthesised expression in colour position, incl. arithmetic
    let d = draw("base = 0xff0000\nbox shaded (base + 0x10)");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#ff0010".into())));
    // a variable holding an out-of-range value errors like a literal
    let e = eval(&parse("c = 0x1000000\nbox outlined c").unwrap()).unwrap_err();
    assert!(e.msg.contains("0-0xFFFFFF"), "{}", e.msg);
    // a bareword that is NOT a variable stays a literal colour name
    let d = draw("box outlined red");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.stroke.as_deref(), Some("red"));
}

#[test]
fn color_string_hex_is_normalised() {
    // a quoted 0xRRGGBB string (easy mistake — the bare literal works) is
    // converted to #rrggbb instead of passing through to invalid SVG
    let d = draw("box shaded \"0x2f855a\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#2f855a".into())));
    // short 0xRGB form too
    let d = draw("box shaded \"0xABC\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("#abc".into())));
    // a genuine CSS colour name is untouched
    let d = draw("box shaded \"crimson\"");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(style.fill, Some(Fill::Color("crimson".into())));
}

#[test]
fn thin_sets_a_lighter_stroke() {
    // `thin` (pikchr-flavoured, no value) = ⅔ of the default linethick (0.8)
    let d = draw("box thin");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (style.thick.unwrap() - 0.8 * 2.0 / 3.0).abs() < 1e-9,
        "{:?}",
        style.thick
    );
    // it tracks the current linethick
    let d = draw("linethick = 3\nline thin");
    let Shape::Path { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (style.thick.unwrap() - 2.0).abs() < 1e-9,
        "{:?}",
        style.thick
    );
    // explicit `thick <n>` is unaffected
    let d = draw("box thick 2");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((style.thick.unwrap() - 2.0).abs() < 1e-9);
    // `thin` is still usable as a plain identifier is NOT — it is a keyword
    // now (like `thick`); assert it parses as the attribute, not a var
    assert!(parse("box thin").is_ok());
}

#[test]
fn canvas_stmt_fixes_the_page_rect() {
    let d = draw("canvas from (0,0) to (4,3)\nbox at (1,1)");
    let c = d.canvas.unwrap();
    assert_eq!((c.min.x, c.min.y, c.max.x, c.max.y), (0.0, 0.0, 4.0, 3.0));
    // corners in either order, and place references both work
    let d = draw("F: box wid 3 ht 2 at (1.5,1) invis\ncanvas from F.ne to F.sw");
    let c = d.canvas.unwrap();
    assert_eq!((c.min.x, c.min.y, c.max.x, c.max.y), (0.0, 0.0, 3.0, 2.0));
    // last statement wins
    let d = draw("canvas from (0,0) to (9,9)\ncanvas from (0,0) to (1,1)\nbox");
    assert_eq!(d.canvas.unwrap().max.x, 1.0);
}

#[test]
fn canvas_stmt_is_inert_as_a_variable() {
    // `canvas = 3` stays a plain assignment; only `canvas from …` triggers
    let d = draw("canvas = 3\nbox wid canvas");
    assert!(d.canvas.is_none());
    // painted bbox includes the stroke — compare loosely
    assert!((d.bbox.width() - 3.0).abs() < 0.05, "{}", d.bbox.width());
}

#[test]
fn canvas_stmt_rejects_degenerate_rects() {
    let e = eval(&parse("canvas from (0,0) to (0,3)").unwrap()).unwrap_err();
    assert!(e.msg.contains("positive width and height"), "{}", e.msg);
}

#[test]
fn canvas_stmt_scales_with_the_picture() {
    // `scale = 2`: canvas given in user units, halved internally
    let d = draw("scale = 2\ncanvas from (0,0) to (8,6)\nbox");
    let c = d.canvas.unwrap();
    assert!((c.max.x - 4.0).abs() < 1e-9 && (c.max.y - 3.0).abs() < 1e-9);
    // maxps clamps the *page* (the fixed canvas), not the content bbox
    let d = draw("maxpswid = 2\ncanvas from (0,0) to (4,3)\nbox wid 1");
    let c = d.canvas.unwrap();
    assert!((c.width() - 2.0).abs() < 1e-6, "{}", c.width());
    assert!((d.bbox.width() - 0.5).abs() < 0.05, "{}", d.bbox.width());
}

#[test]
fn canvas_stmt_propagates_out_of_blocks_translated() {
    // canvas is global like variables; a block's rect lands in parent space
    let d = draw("box wid 1 ht 1 at (0.5,0.5)\n[ canvas from (0,0) to (2,1) ] with .sw at (0,0)");
    let c = d.canvas.unwrap();
    assert!((c.min.x - 0.0).abs() < 1e-9, "{}", c.min.x);
    assert!((c.width() - 2.0).abs() < 1e-9);
}

#[test]
fn canvas_margin_vars_are_scaled_dimensions() {
    let d = draw("margin = 1; topmargin = 0.5; rightmargin = 0.25; line right");
    assert_eq!(
        d.canvas_margin,
        CanvasMargin {
            top: 1.5,
            right: 1.25,
            bottom: 1.0,
            left: 1.0,
        }
    );

    let d = draw("scale = 2; margin = 1; topmargin = 1; line right");
    assert_eq!(
        d.canvas_margin,
        CanvasMargin {
            top: 1.0,
            right: 0.5,
            bottom: 0.5,
            left: 0.5,
        }
    );

    let d = draw("margin = 1; scale = 2; line right");
    assert_eq!(
        d.canvas_margin,
        CanvasMargin {
            top: 1.0,
            right: 1.0,
            bottom: 1.0,
            left: 1.0,
        }
    );
}

#[test]
fn print_statements_collect_diagnostics() {
    let d = draw("print 5.5\nprint 5.5%2\nprint \"hello\"\nprint sprintf(\"x=%g\", 1.25)");
    assert_eq!(d.diagnostics, ["5.5", "0", "hello", "x=1.25"]);

    let d = draw("[ print \"inside\"; box ]\nprint 7");
    assert_eq!(d.diagnostics, ["inside", "7"]);
}

#[test]
fn command_and_sh_are_silent_noops() {
    // Policy (#129): `command` raw backend text is never injected and `sh`
    // is never executed. Both are tolerated so dpic sources keep
    // compiling, and they emit no diagnostic lines and no shapes.
    let d = draw("box wid 1 ht 1\nsh \"echo hi\"\ncommand \"</g>\"\nbox wid 1 ht 1");
    assert!(d.diagnostics.is_empty(), "{:?}", d.diagnostics);
    assert_eq!(d.shapes.len(), 2);

    // Geometry flows across the skipped directives unchanged: the second
    // box lands exactly where it would without them.
    let plain = draw("box wid 1 ht 1\nbox wid 1 ht 1");
    assert_eq!(d.bbox, plain.bbox);
}

#[test]
fn gradient_style_records_stops_and_angle() {
    let d = draw("box gradient \"steelblue\" \"white\" gradientangle 45");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    let g = style.gradient.as_ref().expect("expected gradient");
    assert_eq!(g.from, "steelblue");
    assert_eq!(g.to, "white");
    assert!((g.angle - 45.0).abs() < 1e-9);
    assert!(style.fill_open);

    // gradientangle alone creates the default black-to-white gradient,
    // mirroring how `hatchangle` alone creates a default hatch
    let d = draw("box gradientangle 90");
    let Shape::Box { style, .. } = &d.shapes[0] else {
        panic!()
    };
    let g = style.gradient.as_ref().unwrap();
    assert_eq!((g.from.as_str(), g.to.as_str()), ("black", "white"));
}

fn fake_math(tex: &str, font_pt: f64) -> Result<crate::math::MathSpan, String> {
    if tex.contains("boom") {
        return Err("fake parse error".into());
    }
    let em = font_pt / 72.0;
    Ok(crate::math::MathSpan {
        svg: format!("<svg width=\"9.6\" height=\"14.08\"><!--{tex}--></svg>"),
        width: 2.0 * em,
        height: 0.8 * em,
        depth: 0.2 * em,
    })
}

#[test]
fn texlabels_routes_dollar_labels_through_the_math_hook() {
    crate::math::set_math_renderer(fake_math);

    // off by default: no math span even with a renderer registered
    let d = draw("box \"$x$\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(text[0].math.is_none());

    // on: fully $-delimited labels are typeset; others stay literal
    let d = draw("texlabels = 1\nbox \"$x$\" \"plain\" \"$a$b$\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    let m = text[0].math.as_ref().expect("math span");
    assert!((m.width - 2.0 * 11.0 / 72.0).abs() < 1e-9);
    assert!(text[0].s.contains("$x$")); // literal kept for fallback
    assert!(text[1].math.is_none());
    assert!(text[2].math.is_none()); // inner `$` disqualifies

    // exact metrics drive the text bbox (2 em wide, not 3 chars * 0.6 em)
    let d = draw("texlabels = 1\n\"$x$\" at (0,0)");
    assert!(
        (d.bbox.width() - 2.0 * 11.0 / 72.0).abs() < 0.02,
        "{}",
        d.bbox.width()
    );

    // renderer failure: literal fallback plus a diagnostic, never an error
    let d = draw("texlabels = 1\nbox \"$boom$\"");
    let Shape::Box { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(text[0].math.is_none());
    assert!(
        d.diagnostics.iter().any(|l| l.contains("fake parse error")),
        "{:?}",
        d.diagnostics
    );
}

#[test]
fn dot_is_a_solid_circle_with_dotrad_default() {
    let d = draw("dot at (0.5, 0.5)");
    let Shape::Circle { r, style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((r - 0.035).abs() < 1e-9);
    assert_eq!(style.fill, Some(Fill::Gray(0.0)));

    // dotrad env var + attribute overrides
    let d = draw("dotrad = 0.06\ndot\ndot rad 0.1 shaded \"red\"");
    let Shape::Circle { r, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((r - 0.06).abs() < 1e-9);
    let Shape::Circle { r, style, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((r - 0.1).abs() < 1e-9);
    assert_eq!(style.fill, Some(Fill::Color("red".into())));

    // contextual: dot stays usable as a variable
    let d = draw("dot = 2\nbox wid dot ht 0.3");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((w - 2.0).abs() < 1e-9);

    // dots are circles for ordinals
    let d = draw("dot at (0,0)\nbox at last circle + (0.5, 0)");
    assert_eq!(d.shapes.len(), 2);
}

#[test]
fn class_attribute_and_statement_set_shape_classes() {
    // inline attribute, and append composition
    let d = draw("box class \"critical\" class \"hot\"");
    assert_eq!(d.shape_classes[0].as_deref(), Some("critical hot"));

    // statement form by label, by ordinal, and reaching macro-drawn shapes
    let d = draw(
        "define wire { line right 0.5 }\nA: box\nwire()\nclass A \"node\"\nclass last line \"bus\"",
    );
    assert_eq!(d.shape_classes[0].as_deref(), Some("node"));
    assert_eq!(d.shape_classes[1].as_deref(), Some("bus"));

    // `class` stays usable as a plain variable
    let d = draw("class = 2\nbox wid class ht 1");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((w - 2.0).abs() < 1e-9);
}

#[test]
fn class_validates_names_and_targets() {
    let e = eval(&parse("box class \"a<b\"").unwrap()).unwrap_err();
    assert!(e.msg.contains("invalid class name"), "{e}");

    let e = eval(&parse("box class \"2fast\"").unwrap()).unwrap_err();
    assert!(e.msg.contains("invalid class name"), "{e}");

    let e = eval(&parse("A: (0,0)\nclass A \"x\"").unwrap()).unwrap_err();
    assert!(e.msg.contains("no drawn shape"), "{e}");
}

#[test]
fn class_composes_with_animate_on_the_same_shape() {
    // The class hook and the animation layer share the `s<N>` contract:
    // both must resolve to the same shape index, and adding a class must
    // not disturb the animation target.
    let d = draw(
        "A: box\ncircle\nanimate A with \"pop\"\nclass A \"critical\"\nanimate last circle with \"fade\"\nclass last circle \"soft\"",
    );
    assert_eq!(d.anims.len(), 2);
    assert_eq!(d.anims[0].shape, 0);
    assert_eq!(d.anims[1].shape, 1);
    assert_eq!(d.shape_classes[0].as_deref(), Some("critical"));
    assert_eq!(d.shape_classes[1].as_deref(), Some("soft"));
}

#[test]
fn class_inside_block_survives_flattening() {
    let d = draw("[ box class \"in\"; circle ]");
    assert_eq!(d.shape_classes[0].as_deref(), Some("in"));
    assert_eq!(d.shape_classes[1], None);

    let e = eval(&parse("[ box ] class \"x\"").unwrap()).unwrap_err();
    assert!(e.msg.contains("block"), "{e}");
}

#[test]
fn open_object_width_height_attrs_are_arrowhead_dimensions() {
    let d = draw("arrowwid = 0.2; arrowht = 0.3\nA: line right 2\nbox wid (A.wid) ht (A.ht)");
    assert_box_size(&d.shapes[1], 0.2, 0.3);

    let d = draw("arrowwid = 0.12; arrowht = 0.34\nA: move right 2\nbox wid (A.wid) ht (A.ht)");
    assert_box_size(&d.shapes[1], 0.12, 0.34);

    let d = draw(
        "arrowwid = 0.23; arrowht = 0.31\nA: spline from (0,0) to (2,1)\nbox wid (A.wid) ht (A.ht)",
    );
    assert_box_size(&d.shapes[1], 0.23, 0.31);
}

#[test]
fn radius_and_diameter_attrs_are_type_specific() {
    let d = draw("B: box rad 0.1 wid 1 ht 1\nbox wid (B.rad) ht 0.3");
    assert_box_size(&d.shapes[1], 0.1, 0.3);

    let d = draw("C: arc rad 0.7 from (0,0) to (0,1.4)\nbox wid (C.rad) ht (C.diam)");
    assert_box_size(&d.shapes[1], 0.7, 1.4);
}

#[test]
fn invalid_type_scalar_attrs_match_dpic_zero() {
    let prog = parse(
        "E: ellipse wid 2 ht 1\nB: box wid 1 ht 1\nA: arc rad .5\n\
             e_rad = E.rad\ne_diam = E.diam\nb_diam = B.diam\na_len = A.len",
    )
    .unwrap();
    let mut st = State::new();
    st.eval_stmts(&prog.stmts).unwrap();
    assert_eq!(st.vars["e_rad"], 0.0);
    assert_eq!(st.vars["e_diam"], 0.0);
    assert_eq!(st.vars["b_diam"], 0.0);
    assert_eq!(st.vars["a_len"], 0.0);
}

#[test]
fn arrowhead_type_open_vs_filled() {
    // default is a filled head; `arrowhead = 0` is an open (two-stroke) head
    let d = draw("arrow right 1");
    let Shape::Path { style, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(style.arrow_filled, "default should be filled");
    let d2 = draw("arrowhead = 0\narrow right 1");
    let Shape::Path { style, .. } = &d2.shapes[0] else {
        panic!()
    };
    assert!(!style.arrow_filled, "arrowhead=0 should be open");

    let d3 = draw("arrowhead = 0\nline <- 1 up");
    let Shape::Path { style, .. } = &d3.shapes[0] else {
        panic!()
    };
    assert!(style.arrow_filled, "`<- 1` should override the global");

    let d4 = draw("line <- 0 up");
    let Shape::Path { style, .. } = &d4.shapes[0] else {
        panic!()
    };
    assert!(!style.arrow_filled, "`<- 0` should override the global");
}

#[test]
fn maxps_clamps_oversized_drawing() {
    // larger than the default 8.5x11in page → scaled down to fit
    let d = draw("box wid 20 ht 30");
    assert!(
        d.bbox.width() <= 8.5 + 1e-6 && d.bbox.height() <= 11.0 + 1e-6,
        "{}x{}",
        d.bbox.width(),
        d.bbox.height()
    );
    // raising the limits disables the clamp
    let d2 = draw("maxpsht = 200; maxpswid = 50\nbox wid 20 ht 30");
    assert!(
        (d2.bbox.height() - (30.0 + DEFAULT_STROKE_IN)).abs() < 1e-6,
        "h = {}",
        d2.bbox.height()
    );
    // a small drawing is untouched
    let d3 = draw("box wid 2 ht 1");
    assert!((d3.bbox.width() - (2.0 + DEFAULT_STROKE_IN)).abs() < 1e-6);

    let d4 = draw("maxpswid = 2; maxpsht = 100\nmargin = 1\nbox wid 1 ht 0.5");
    assert!(
        d4.bbox.width() + d4.canvas_margin.horizontal() <= 2.0 + 1e-6,
        "canvas width = {}",
        d4.bbox.width() + d4.canvas_margin.horizontal()
    );
    assert!(
        d4.canvas_margin.left < 1.0 && d4.canvas_margin.right < 1.0,
        "{:?}",
        d4.canvas_margin
    );
}

#[test]
fn block_variable_assignments_are_local() {
    let d = draw("x = 1\n[ x = 5 ]\nbox wid x ht 0.3");
    let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9, "w = {w}");

    assert!(eval(&parse("[ x = 5 ]\nbox wid x ht 0.3").unwrap()).is_err());
}

#[test]
fn block_env_assignments_are_local() {
    let d = draw("[ boxwid = 2; box ]\nbox");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 2.0).abs() < 1e-9, "inner w = {w}");
    let Shape::Box { w, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((*w - 0.75).abs() < 1e-9, "outer w = {w}");
}

#[test]
fn block_mutating_var_assignments_update_inherited_vars() {
    let d = draw("x = 1\n[ x := 5 ]\nbox wid x ht 0.3");
    let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((*w - 5.0).abs() < 1e-9, "w = {w}");

    let d = draw("x = 1\n[ x += 2 ]\nbox wid x ht 0.3");
    let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((*w - 3.0).abs() < 1e-9, "w = {w}");

    assert!(eval(&parse("[ x = 1; x += 2 ]\nbox wid x ht 0.3").unwrap()).is_err());

    let d = draw("boxwid = 0.75\n[ boxwid := 2; box ]\nbox");
    let Shape::Box { w, .. } = &d.shapes[1] else {
        panic!()
    };
    assert!((*w - 0.75).abs() < 1e-9, "outer w = {w}");
}

#[test]
fn figuras_examples_compile() {
    // a few of André Leite's circuit_macros figures (examples/figuras/),
    // adapted with the compatibility shim — they must keep compiling/drawing.
    for src in [
        include_str!("../../../../examples/figuras/fig01.pic"),
        include_str!("../../../../examples/figuras/fig36.pic"),
        include_str!("../../../../examples/figuras/fig40.pic"),
    ] {
        let d = eval(&parse(src).unwrap()).unwrap();
        assert!(!d.shapes.is_empty());
    }
}

#[test]
fn figuras_element_examples_compile() {
    // André Leite's circuit_macros figures that use the *element API*
    // (resistor(dir len), bi_tr, opamp, …). These render with the circuit
    // library (-c) plus the compatibility shim, which reuses the native
    // element geometry. The shim is `copy`-d in by each file; here we splice
    // it in directly and prepend the circuit library.
    let shim = include_str!("../../../../examples/figuras/circuit_macros.pic");
    for body in [
        include_str!("../../../../examples/figuras/fig21.pic"),
        include_str!("../../../../examples/figuras/fig23.pic"),
        include_str!("../../../../examples/figuras/fig26.pic"),
        include_str!("../../../../examples/figuras/fig27.pic"),
        include_str!("../../../../examples/figuras/fig28.pic"),
        include_str!("../../../../examples/figuras/fig30.pic"),
        include_str!("../../../../examples/figuras/fig33.pic"),
        include_str!("../../../../examples/figuras/fig45.pic"),
        include_str!("../../../../examples/figuras/fig46.pic"),
        include_str!("../../../../examples/figuras/fig09.pic"),
        include_str!("../../../../examples/figuras/fig11.pic"),
    ] {
        let body = body.replace("copy \"circuit_macros.pic\"", shim);
        let src = format!("{}\n{}", crate::CIRCUITS, body);
        let d = eval(&parse(&src).unwrap()).unwrap();
        assert!(!d.shapes.is_empty());
    }
}

#[test]
fn lib3d_examples_compile() {
    // The lib3D shim (3D -> 2D axonometric projection) and its demos must
    // keep compiling and drawing. The demos `copy` the shim; splice it in.
    let shim = include_str!("../../../../examples/lib3d/lib3d.pic");
    for body in [
        include_str!("../../../../examples/lib3d/frame.pic"),
        include_str!("../../../../examples/lib3d/views.pic"),
    ] {
        let src = body.replace("copy \"lib3d.pic\"", shim);
        let d = eval(&parse(&src).unwrap()).unwrap();
        assert!(!d.shapes.is_empty());
    }
}

#[test]
fn brace_ncount_as_place() {
    // `{expr}th last box` — a brace-counted ordinal used as a place
    let d =
        draw("box at 0,0\nbox at 2,0\nbox at 4,0\narrow from {2}th last box.e to {1}th last box.w");
    let Shape::Path { pts, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!(pts[0].x > 2.0 && pts.last().unwrap().x < 4.0, "{pts:?}");
}

#[test]
fn dpic_unit_suffix() {
    // `72bp__` == 72 * scale/72 == 1 inch
    let d = draw("box wid 72bp__ ht 0.3");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9, "w = {w}");
}

#[test]
fn block_sees_outer_labels() {
    // a label defined before a block is visible (read-only) inside it
    let d = draw("A: (0,0)\n[ line from A to (2,0) ]");
    assert!(
        d.shapes.iter().any(|s| matches!(s, Shape::Path { .. })),
        "block should draw a line referencing the outer label A"
    );
    // outer labels must not pollute the block's `last`/nth: a box drawn
    // before the block isn't the block's `last box`.
    assert!(eval(&parse("box\n[ circle; \"x\" at last box ]").unwrap()).is_err());
}

#[test]
fn arg_count_macro() {
    // `$+` is the number of arguments passed to the current macro
    let d = draw("define cnt { $+ }\nx = cnt(a, b, c)\nbox wid x ht 0.3");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 3.0).abs() < 1e-9, "w = {w}");
}

#[test]
fn exec_evaluates_generated_pic_in_macro_arg_scope() {
    let d = draw(
        "define array { for i_array=2 to $+ do { exec sprintf(\"$1[%g] = $%g\", i_array-1, i_array) } }\narray(a, 0, 1, 3)\nbox wid a[2] ht a[3]",
    );
    let Shape::Box { w, h, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(
        (*w - 1.0).abs() < 1e-9 && (*h - 3.0).abs() < 1e-9,
        "{w} x {h}"
    );
}

#[test]
fn exec_unescapes_generated_quoted_text() {
    let d = draw("exec sprintf(\"\\\"x\\\" at Here\")");
    let Shape::Text { text, .. } = &d.shapes[0] else {
        panic!()
    };
    assert_eq!(text[0].s, "x");
}

#[test]
fn macro_token_pasting_concatenates_adjacent_args() {
    let d = draw("define mark { $1$2: (1,0) }\nmark(A,B)\nbox wid 0.2 ht 0.2 at AB");
    let Shape::Box { c, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!(c.dist(Point::new(1.0, 0.0)) < 1e-9, "c = {c:?}");
}

#[test]
fn macro_string_substitution_preserves_dot_prefixed_arguments() {
    let d = draw("define label { \"$1\"; \"$2\" }\nlabel(.ne,above)");
    let labels: Vec<&str> = d
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text { text, .. } => Some(text[0].s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, [".ne", "above"]);
}

#[test]
fn recursive_macro_terminates() {
    // a self-calling macro bounded by `if`: textual pre-expansion would
    // diverge, but lazy (eval-time) expansion of the taken branch stops it.
    let d = draw("define rec { if $1 <= 0 then { circle } else { box; rec($1-1) } }\nrec(3)");
    let boxes = d
        .shapes
        .iter()
        .filter(|s| matches!(s, Shape::Box { .. }))
        .count();
    let circles = d
        .shapes
        .iter()
        .filter(|s| matches!(s, Shape::Circle { .. }))
        .count();
    assert_eq!((boxes, circles), (3, 1), "shapes = {:?}", d.shapes.len());
}

#[test]
fn default_argument_idiom() {
    // empty argument: the dead `else { w = $1 }` becomes `w =`, which must
    // not be parsed because the then-branch is taken.
    let d =
        draw("define b { if \"$1\"==\"\" then { w = 1 } else { w = $1 }\n box wid w ht 0.2 }\nb()");
    let Shape::Box { w, .. } = &d.shapes[0] else {
        panic!()
    };
    assert!((*w - 1.0).abs() < 1e-9, "w = {w}");
    // and with an argument supplied, the else-branch value is used
    let d2 = draw(
        "define b { if \"$1\"==\"\" then { w = 1 } else { w = $1 }\n box wid w ht 0.2 }\nb(2.5)",
    );
    let Shape::Box { w, .. } = &d2.shapes[0] else {
        panic!()
    };
    assert!((*w - 2.5).abs() < 1e-9, "w = {w}");
}

#[test]
fn last_ordinal() {
    let d = draw("box at 0,0\nbox at 2,0\narrow from 1st box.e to 2nd box.w");
    let Shape::Path { pts, .. } = &d.shapes[2] else {
        panic!()
    };
    // from first box east edge to second box west edge
    assert!(pts[0].x > 0.0 && pts.last().unwrap().x < 2.0);
}

#[test]
fn untyped_last_references_any_kind() {
    // `last.c` after a circle resolves to that circle (no `last circle`).
    let d = draw("circle rad 0.5 at (3,1)\n\"x\" at last.c");
    let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((at.x - 3.0).abs() < 1e-9 && (at.y - 1.0).abs() < 1e-9);
}

#[test]
fn untyped_last_corner_after_box() {
    // `last.n` (north of the most recent object, whatever its kind).
    let d = draw("box wid 2 ht 1 at (0,0)\n\"y\" at last.n");
    let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((at.x - 0.0).abs() < 1e-9 && (at.y - 0.5).abs() < 1e-9);
}

#[test]
fn untyped_nth_last_spans_kinds() {
    // `2nd last` counts across kinds: box, then circle -> 2nd last is the box.
    let d = draw("box at (0,0)\ncircle at (2,0)\n\"z\" at 2nd last.c");
    let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((at.x - 0.0).abs() < 1e-9, "x = {}", at.x);
}

#[test]
fn typed_last_still_filters_by_kind() {
    // an explicit type keyword keeps filtering: `last box` skips the circle.
    let d = draw("box at (0,0)\ncircle at (2,0)\n\"w\" at last box.c");
    let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
        panic!()
    };
    assert!((at.x - 0.0).abs() < 1e-9, "x = {}", at.x);
}

#[test]
fn untyped_last_with_no_object_errors() {
    assert!(eval(&parse("\"q\" at last.c").unwrap()).is_err());
}
