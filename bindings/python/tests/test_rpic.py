# Smoke tests for the Python binding. Run against a built wheel:
#   maturin build && pip install target/wheels/*.whl pytest && pytest tests -q
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
