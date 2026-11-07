# Smoke tests for the Python binding. Run against a built wheel:
#   maturin build && pip install target/wheels/*.whl pytest && pytest tests -q
import json

import pytest

import rpic


def test_render_svg():
    svg = rpic.render_svg('box "hi"; arrow; circle "x"')
    assert svg.startswith("<svg")
    assert "hi" in svg


def test_circuits_option():
    svg = rpic.render_svg("A:(0,0); B:(2,0)\nresistor(A,B)", circuits=True)
    assert "<svg" in svg


def test_texlabels_typesets():
    svg = rpic.render_svg('box "$-\\frac{T}{2}$" wid 1 ht 0.7', texlabels=True)
    assert "frac" not in svg  # typeset as glyph paths, no raw TeX leaks


def test_render_png_and_pdf():
    png = rpic.render_png('box "x"', scale=2.0)
    assert png[:4] == b"\x89PNG"
    pdf = rpic.render_pdf('box "x"')
    assert pdf[:4] == b"%PDF"


def test_compile_bundle():
    bundle = rpic.compile('box\nanimate last box with "pop"')
    assert bundle["svg"].startswith("<svg")
    assert bundle["animations"] == [
        {"id": "s0", "effect": "pop", "start": 0.0, "duration": bundle["animations"][0]["duration"]}
    ]
    assert bundle["diagnostics"] == []
    assert bundle["warnings"] == []


def test_compile_objects_geometry():
    # #227: per-object geometry (bbox in SVG user units + source span)
    bundle = rpic.compile("box wid 1 ht 0.5\narrow right 0.5")
    box, path = bundle["objects"]
    assert box["id"] == "s0" and box["kind"] == "box"
    assert box["bbox"]["w"] == 96 and box["bbox"]["h"] == 48
    assert path["kind"] == "path"
    assert path["line"] == 2 and path["col"] == 1
    invis = rpic.compile("circle invis")["objects"][0]
    assert invis["bbox"] is None


def test_compile_places_and_passthrough_ride_only_when_present():
    # named places in the `objects` frame (SVG user units) + troff lines
    bundle = rpic.compile("box wid 1 ht 0.5\nP: last box.e\n.ft B\n")
    (place,) = bundle["places"]
    bbox = bundle["objects"][0]["bbox"]
    assert place["label"] == "P"
    assert abs(place["x"] - (bbox["x"] + 96)) < 1e-3
    assert abs(place["y"] - (bbox["y"] + 24)) < 1e-3
    assert (place["line"], place["col"]) == (2, 1)
    assert bundle["passthrough"] == [{"text": ".ft B", "line": 3, "col": 1, "end_col": 6}]
    plain = rpic.compile("box")
    assert "places" not in plain and "passthrough" not in plain
    # a place inside `[ ]` names its block path, as the scene does (#453)
    nested_src = "A: (0,0)\nB: [ C: (1,1); box at C ]\nD: (2,0)"
    a, c, d = rpic.compile(nested_src)["places"]
    assert c["label"] == "C" and c["block"] == ["blk0"]
    assert c["block"] == rpic.scene(nested_src)["places"][1]["block"]
    assert "block" not in a and "block" not in d
    # a request continued by column-1 `+` lines is one entry (#425)
    cont = rpic.compile(".ft B\n+ more\nbox\n")
    assert cont["passthrough"] == [
        {"text": ".ft B\n+ more", "line": 1, "col": 1, "end_col": 7, "end_line": 2}
    ]


def test_compile_link_rides_only_when_set():
    # `link "<url>"` (#358): the object carries "link", in the CLI bundle's key order
    bundle = rpic.compile('box link "https://example.org"\ncircle')
    linked, plain = bundle["objects"]
    assert linked["link"] == "https://example.org"
    assert list(linked) == ["id", "kind", "bbox", "link", "line", "col", "end_col"]
    assert "link" not in plain
    assert all("link" not in o for o in rpic.compile("box\ncircle")["objects"])


def test_compile_interactions_ride_only_when_present():
    # `draggable` (#331): a top-level "interactions" array, absent when unused
    bundle = rpic.compile("B: box wid 3 ht 2\nN: circle at B.c\ndraggable N inertia bounds B x")
    assert bundle["interactions"] == [
        {"id": "s1", "kind": "drag", "inertia": True, "bounds": "s0", "axis": "x"}
    ]
    simple = rpic.compile('box "drag me"\ndraggable last')
    assert simple["interactions"] == [{"id": "s0", "kind": "drag"}]
    assert "interactions" not in rpic.compile("box")


def test_compile_matches_the_json_bundle_keys():
    # every key of the CLI `--json` bundle reaches the dict (#444): objects'
    # link, interactions and the optional animation keys
    src = "\n".join(
        [
            'A: box "one two" link "https://example.org"',
            'B: box "SECRET"',
            "L: line right 2",
            "C: circle",
            'animate A with "type" by word',
            'animate B with "scramble" by "01"',
            'animate C with "wiggle" wiggles 6',
            'animate L with "draw" from 20% to 60%',
            "draggable C inertia",
            "animate scroll",
        ]
    )
    bundle = rpic.compile(src)
    ref = json.loads(rpic.compile_json(src))
    assert set(bundle) == set(ref)
    for key in ("animations", "objects", "interactions"):
        # same keys in the same order (bbox values: the dict keeps full
        # floats where the JSON rounds to 4 places, so compare them apart)
        assert [list(x) for x in bundle[key]] == [list(x) for x in ref[key]], key
        strip = lambda xs: [{k: v for k, v in x.items() if k != "bbox"} for x in xs]  # noqa: E731
        assert strip(bundle[key]) == strip(ref[key]), key
    for got, want in zip(bundle["objects"], ref["objects"]):
        for k in ("x", "y", "w", "h"):
            assert abs(got["bbox"][k] - want["bbox"][k]) < 1e-3
    anims = bundle["animations"]
    assert anims[0]["unit"] == "word"
    assert anims[1]["chars"] == "01"
    assert anims[2]["wiggles"] == 6
    assert (anims[3]["drawFrom"], anims[3]["drawTo"]) == (0.2, 0.6)
    assert bundle["scroll"] is True


def test_compile_warnings():
    bundle = rpic.compile('box "a" dashd')
    (w,) = bundle["warnings"]
    assert w["kind"] == "ignored_attribute"
    assert w["found"] == "dashd"
    assert w["hint"] == "did you mean `dashed`?"


def test_compile_error_is_structured():
    with pytest.raises(rpic.CompileError) as excinfo:
        # #181: the circuits prelude must not shift user positions
        rpic.render_svg("bxo", circuits=True)
    info = excinfo.value.info
    assert info["line"] == 1
    assert info["col"] == 1
    assert info["end_col"] == 4
    assert info["file"] is None
    assert info["kind"] == "expected_token"
    assert info["hint"] == "did you mean `box`?"
    assert "expected an object" in str(excinfo.value)


def test_compile_error_is_a_value_error():
    # backward compatibility: pre-0.7 callers caught ValueError
    with pytest.raises(ValueError):
        rpic.render_svg("bxo")


def test_eval_budgets_are_configurable():
    with pytest.raises(rpic.CompileError, match="for loop exceeded 2 iterations"):
        rpic.render_svg("for i = 1 to 3 do { bxo }", max_loop_iterations=2)

    with pytest.raises(rpic.CompileError, match="drawing exceeded 1 shapes"):
        rpic.compile("box\nbox", max_shapes=1)


def test_base_dir_resolves_copy(tmp_path):
    (tmp_path / "inc.pic").write_text("box wid 0.5 ht 0.5\n")
    svg = rpic.render_svg('copy "inc.pic"\ncircle', base=tmp_path)
    assert "<rect" in svg
    assert "<circle" in svg


def test_diagnostic_inside_include_names_the_file(tmp_path):
    (tmp_path / "warn.pic").write_text('# comment\nbox "a" dashd\n')
    bundle = rpic.compile('circle\ncopy "warn.pic"', base=tmp_path)
    (w,) = bundle["warnings"]
    assert w["file"] == "warn.pic"
    assert w["line"] == 2  # include-relative


def test_copy_circuits_needs_no_base():
    svg = rpic.render_svg('copy "circuits"\nA:(0,0); B:(2,0)\nresistor(A,B)')
    assert "<svg" in svg


def test_include_policy(tmp_path):
    base = tmp_path / "base"
    base.mkdir()
    (tmp_path / "outside.pic").write_text("circle\n")
    (base / "inc.pic").write_text("box wid 0.5 ht 0.5\n")

    # sandboxed: in-base works, escapes are structured errors
    svg = rpic.render_svg('copy "inc.pic"\nbox', base=base, include_policy="sandboxed")
    assert "<svg" in svg
    with pytest.raises(rpic.CompileError) as excinfo:
        rpic.render_svg('copy "../outside.pic"\nbox', base=base, include_policy="sandboxed")
    assert excinfo.value.info["kind"] == "include_denied"

    # deny blocks file includes but the embedded library still loads
    with pytest.raises(rpic.CompileError):
        rpic.render_svg('copy "inc.pic"\nbox', base=base, include_policy="deny")
    assert "<svg" in rpic.render_svg('copy "circuits"\nbox', include_policy="deny")

    # unknown policy string is a plain ValueError
    with pytest.raises(ValueError, match="include_policy"):
        rpic.render_svg("box", include_policy="nope")


# ---- #450: named preludes --------------------------------------------------

_LIB = "define vsrc { [ line from $1 to $2; circle rad 0.1 at last line.c ] }\n"


def test_preludes_keep_user_positions_and_name_their_own():
    src = "A: (0,0); B: (1,0)\nV1: vsrc(A,B)\n"
    out = rpic.compile(src, preludes=[{"name": "dune", "text": _LIB}], scene=True)
    blk = next(i for i in out["scene"]["items"] if i["kind"] == "block")
    assert blk["label"] == "V1"
    assert blk["span"]["line"] == 2 and "file" not in blk["span"]
    assert all(o["file"] == "dune" for o in out["objects"])
    # (name, text) pairs work too, through every entry point
    assert rpic.render_svg(src, preludes=[("dune", _LIB)]) == out["svg"]
    assert json.loads(rpic.compile_json(src, preludes=[("dune", _LIB)]))["svg"] == out["svg"]
    assert rpic.scene(src, preludes=[("dune", _LIB)])["items"] == out["scene"]["items"]
    # no preludes = the plain compile
    assert rpic.compile("box", preludes=[]) == rpic.compile("box")


def test_prelude_errors():
    with pytest.raises(rpic.CompileError) as e:
        rpic.compile("box\nbxo\n", preludes=[("dune", _LIB)])
    assert (e.value.info["line"], e.value.info["file"]) == (2, None)
    with pytest.raises(rpic.CompileError) as e:
        rpic.compile("box", preludes=[("house", "circle\nbxo\n")])
    assert (e.value.info["line"], e.value.info["file"]) == (2, "house")
    for bad in ([("circuits", "box")], [("x", "box"), ("x", "box")], [("", "box")]):
        with pytest.raises(rpic.CompileError) as e:
            rpic.compile("box", preludes=bad)
        assert e.value.info["kind"] == "invalid_options"
    with pytest.raises(TypeError):
        rpic.compile("box", preludes=[{"name": "x"}])
    with pytest.raises(TypeError):
        rpic.compile("box", preludes=[42])
