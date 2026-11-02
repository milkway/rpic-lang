# Scene export (docs/scene.md) through the Python surface.
import pytest

import rpic

# The same five snippets are spot-checked by the JS and C binding tests —
# all three surfaces serialize via the single core emitter (parity by
# construction); these pin the shared observable facts.
PARITY = [
    'geo_lon = -34.8811\nB: box "hub" fit\narrow right from B.e',
    "K: [ box; N: [ circle ] with .w at last box.e ]",
    "spline from (0,0) to (0.5,0.4) then to (1,0) ->",
    "arc from (2,0) to (2.5,0.5) <->\ndot at (3,0)",
    "box fill 0.3 opacity 0.5\nline right dashed 0.1 color rgb(255,140,0)",
]


def test_scene_header_and_vars():
    v = rpic.scene(PARITY[0])
    assert v["scene"] == 1
    assert v["units"] == "in"
    assert v["y_up"] is True
    assert v["vars"]["geo_lon"] == -34.8811  # full f64 precision
    assert v["vars"]["boxwid"] == 0.75
    assert v["canvas"]["w"] > 0


def test_scene_items_ids_kinds():
    v = rpic.scene(PARITY[0])
    items = v["items"]
    assert [i["id"] for i in items] == ["s0", "s1"]
    assert [i["kind"] for i in items] == ["box", "arrow"]
    assert items[0]["label"] == "B"
    assert items[1]["heads"][0]["filled"] is True


def test_scene_blocks_and_paths():
    v = rpic.scene(PARITY[1])
    kinds = [i["kind"] for i in v["items"]]
    assert kinds == ["block", "box", "block", "circle"]
    assert v["items"][3]["block"] == ["blk1", "blk0"]

    curves = rpic.scene(PARITY[2])
    assert curves["items"][0]["kind"] == "spline"
    assert curves["items"][0]["paths"][0]["cubics"]


def test_scene_rides_the_compile_bundle_only_when_asked():
    out = rpic.compile(PARITY[0], scene=True)
    assert out["scene"] == rpic.scene(PARITY[0])
    assert "scene" not in rpic.compile(PARITY[0])


def test_scene_flatten_and_validation():
    flat = rpic.scene(PARITY[2], flatten=1e-3)
    path = flat["items"][0]["paths"][0]
    assert "cubics" not in path and len(path["points"]) > 2
    with pytest.raises(ValueError):
        rpic.scene("box", flatten=-1.0)


def test_scene_style_descriptors():
    v = rpic.scene(PARITY[4])
    box, line = v["items"]
    assert box["fill"]["color"] == "#4d4d4d"  # pic gray 0.3
    assert box["opacity"] == 0.5
    assert line["stroke"]["color"] == "#ff8c00"
    assert line["stroke"]["dash"] == {"kind": "dashed", "len": 0.1}


def test_scene_function_warns_on_unsupported():
    # the standalone scene() surfaces scene_unsupported via warnings.warn
    with pytest.warns(UserWarning, match="rpic scene:"):
        rpic.scene('box outlined "currentColor"')


def test_scene_unsupported_warning_surfaces_in_compile():
    out = rpic.compile('box outlined "currentColor"', scene=True)
    kinds = [w["kind"] for w in out["warnings"]]
    assert "scene_unsupported" in kinds
    # the plain bundle stays warning-inert
    assert all(
        w["kind"] != "scene_unsupported"
        for w in rpic.compile('box outlined "currentColor"')["warnings"]
    )
