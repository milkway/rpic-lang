# rpic.geo: pic drawings as GeoJSON / GeoDataFrames (docs/geopic.md).
import math
import warnings

import pytest

import rpic

SRC = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 400\nB: box \"hub\" at (1, 0.5)\narrow from (0,0) to B.sw"


def test_to_geojson_is_a_feature_collection_in_wgs84():
    fc = rpic.geo.to_geojson(SRC)
    assert fc["type"] == "FeatureCollection"
    kinds = [f["properties"]["geopic:kind"] for f in fc["features"]]
    assert "box" in kinds and "arrow" in kinds and "arrowhead" in kinds
    for f in fc["features"]:
        assert f["properties"]["geopic:schema"] == 1
    box = next(f for f in fc["features"] if f["properties"]["geopic:kind"] == "box")
    assert box["geometry"]["type"] == "Polygon"
    lon, lat = box["geometry"]["coordinates"][0][0]
    assert -34.9 < lon < -34.8 and -8.1 < lat < -8.0
    # the box's label is its own Point (#435), right after the polygon
    i = fc["features"].index(box)
    title = fc["features"][i + 1]
    assert title["geometry"]["type"] == "Point"
    assert title["properties"]["geopic:kind"] == "title"
    assert title["properties"]["geopic:for"] == box["properties"]["geopic:id"]
    assert title["properties"]["geopic:text"] == "hub" and box["properties"]["title"] == "hub"


def test_compile_returns_registration_and_structured_warnings():
    out = rpic.geo.compile("geo_lon = 0\ngeo_lat = 0\nbox gradient \"red\" \"blue\"", warn=False)
    assert set(out) == {"geojson", "warnings", "diagnostics", "registration", "canvas"}
    assert out["canvas"] == {"rescale": 1.0, "scale": 1.0}
    r = out["registration"]
    assert r["method"] == "anchor" and r["frame"] == "enu"
    assert r["scale_m_per_in"] == 100  # the default
    assert any(w["kind"] == "unsupported_style" for w in out["warnings"])
    # the same warnings are raised as GeoWarning by default
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        rpic.geo.compile("geo_lon = 0\ngeo_lat = 0\nbox gradient \"red\" \"blue\"")
    assert any(isinstance(w.message, rpic.geo.GeoWarning) and w.message.kind == "unsupported_style" for w in caught)


def test_overrides_and_frames():
    a = rpic.geo.compile(SRC, warn=False)["registration"]
    b = rpic.geo.compile(SRC, anchor=(-46.6333, -23.5505), scale=50, bearing=90, frame="mercator", warn=False)["registration"]
    assert a["anchor"] == {"lon": -34.8711, "lat": -8.0631}
    assert b["anchor"] == {"lon": -46.6333, "lat": -23.5505}
    assert b["scale_m_per_in"] == 50 and b["bearing_deg"] == 90 and b["frame"] == "mercator"
    with pytest.raises(ValueError):
        rpic.geo.compile(SRC, frame="utm")


def test_errors_are_structured():
    with pytest.raises(rpic.geo.GeoError) as e:
        rpic.geo.to_geojson("box")
    assert e.value.info["kind"] == "no_registration"
    assert "geo_lon" in e.value.info["hint"]
    assert isinstance(e.value, ValueError)
    with pytest.raises(rpic.CompileError):
        rpic.geo.to_geojson("geo_lon = 0\ngeo_lat = 0\nbox )")


def test_engine_warnings_and_print_lines_ride_along():
    out = rpic.geo.compile('geo_lon = 0\ngeo_lat = 0\nbox color "nocolour"\nprint "hi"', warn=False)
    assert out["diagnostics"] == ["hi"]
    assert any(w["kind"] == "invalid_color" and w["line"] == 3 for w in out["warnings"]), out["warnings"]


def test_animate_is_warned_not_silently_dropped():
    out = rpic.geo.compile("geo_lon = 0\ngeo_lat = 0\nbox\nanimate last box with \"fade\"", warn=False)
    assert any(w["kind"] == "ignored" and "animation" in w["message"] for w in out["warnings"])


def test_features_are_injected_before_compiling():
    # the source never defines Recife or Olinda: the host does
    src = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\narrow from Recife to Olinda\n\"R\" at Recife"
    feats = {"Recife": (-34.8711, -8.0631), "Olinda": (-34.8553, -8.0089)}
    out = rpic.geo.compile(src, features=feats, warn=False)
    assert out["registration"]["method"] == "two_points"
    arrow = next(f for f in out["geojson"]["features"] if f["properties"]["geopic:kind"] == "arrow")
    first, last = arrow["geometry"]["coordinates"][0], arrow["geometry"]["coordinates"][-1]
    assert math.isclose(first[0], -34.8711, abs_tol=1e-6) and math.isclose(first[1], -8.0631, abs_tol=1e-6)
    # the arrow's end is pulled back for the head; the head's tip sits on Olinda
    head = next(f for f in out["geojson"]["features"] if f["properties"]["geopic:kind"] == "arrowhead")
    ring = head["geometry"]["coordinates"][0]
    assert any(math.isclose(p[0], -34.8553, abs_tol=1e-6) and math.isclose(p[1], -8.0089, abs_tol=1e-6) for p in ring), ring
    assert last != first
    # the declared anchor/scale positioned the features; the fit reproduces
    # them, so no "ignored" warning blames the source
    assert not any(w["kind"] == "ignored_with_control_points" for w in out["warnings"]), out["warnings"]
    # a bbox feature exposes corners: `Name.n` is north of `Name.c`
    src2 = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\ndot at Zone.c\ndot at Zone.n"
    out2 = rpic.geo.compile(src2, features=[("Zone", -34.86, -8.05, (-34.87, -8.06, -34.85, -8.04))], warn=False)
    dots = [f["geometry"]["coordinates"] for f in out2["geojson"]["features"] if f["properties"]["geopic:kind"] == "dot"]
    assert math.isclose(dots[1][1], -8.04, abs_tol=1e-4) and math.isclose(dots[0][1], -8.05, abs_tol=1e-4)
    # iterables of tuples work too; a feature the source never uses is still
    # defined (injected as a place), so it resolves and anchors the drawing
    out3 = rpic.geo.compile("geo_lon = 0\ngeo_lat = 0\nbox", features=[("Nowhere", 1.0, 2.0)], warn=False)
    assert out3["registration"]["control_points"] == 1
    assert out3["registration"]["anchor"] == {"lon": 1.0, "lat": 2.0}
    assert not out3["warnings"], out3["warnings"]


def test_to_geodataframe_needs_geopandas_or_returns_a_geodataframe():
    try:
        import geopandas  # noqa: F401
    except ImportError:
        with pytest.raises(ImportError) as e:
            rpic.geo.to_geodataframe(SRC)
        assert "rpiclang[geo]" in str(e.value)
        return
    gdf = rpic.geo.to_geodataframe(SRC, warn=False)
    assert gdf.crs.to_epsg() == 4326
    assert {"stroke", "geopic:kind", "geopic:id"} <= set(gdf.columns)
    assert len(gdf) == 4  # box, arrow, arrowhead, the box's title point
    assert set(gdf.geometry.geom_type) == {"Polygon", "LineString", "Point"}


def test_features_the_source_defines_pin_its_own_objects():
    # the CLI idiom: the source draws Recife and Olinda itself, the host says
    # where they are — no anchor needed, and the boxes (not placeholders)
    # are the control points
    src = "Recife: box at (2, 1)\nOlinda: box at (5, 3)\narrow from Recife to Olinda"
    feats = {"Recife": (-34.8711, -8.0631), "Olinda": (-34.8553, -8.0089)}
    out = rpic.geo.compile(src, features=feats, warn=False)
    r = out["registration"]
    assert r["method"] == "two_points" and r["rms_m"] < 1e-6
    boxes = [f for f in out["geojson"]["features"] if f["properties"]["geopic:kind"] == "box"]
    assert len(boxes) == 2
    # Recife's box is centred on Recife: its ring surrounds the point
    ring = boxes[0]["geometry"]["coordinates"][0]
    lons = [p[0] for p in ring]
    lats = [p[1] for p in ring]
    assert min(lons) < -34.8711 < max(lons) and min(lats) < -8.0631 < max(lats)
    # mixed: one defined by the source, one injected
    src2 = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\nRecife: (0, 0)\narrow from Recife to Olinda"
    out2 = rpic.geo.compile(src2, features=feats, warn=False)
    assert out2["registration"]["method"] == "two_points"


def test_injected_features_respect_pic_scale_and_rescale():
    # `scale = 10` (ten source units per inch) and `.PS 2` (the picture is
    # rescaled to 2 in wide): the injected place must still land where the
    # host said, so a dot drawn at it is the feature's position
    src = ".PS 2\nscale = 10\ngeo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\nline from (0,0) to (40, 0)\ndot at Olinda\n.PE"
    out = rpic.geo.compile(src, features={"Olinda": (-34.8553, -8.0089)}, warn=False)
    assert out["canvas"]["scale"] == 10 and out["canvas"]["rescale"] < 1
    dot = next(f for f in out["geojson"]["features"] if f["properties"]["geopic:kind"] == "dot")
    lon, lat = dot["geometry"]["coordinates"]
    assert math.isclose(lon, -34.8553, abs_tol=1e-6) and math.isclose(lat, -8.0089, abs_tol=1e-6)


def test_bbox_features_follow_the_bearing_and_diagnostics_keep_caller_lines():
    # compass corners are canvas directions: under a 90° bearing the canvas
    # +y axis points east, so Zone.n is the zone's EAST edge on the Earth and
    # Zone.e its SOUTH edge — and the invisible box still spans the zone's
    # full extent (its four corners, not two)
    src = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\ngeo_bearing = 90\ndot at Zone.c\ndot at Zone.n\ndot at Zone.e"
    out = rpic.geo.compile(src, features=[("Zone", -34.86, -8.05, (-34.87, -8.06, -34.85, -8.04))], warn=False)
    c, n, e = [f["geometry"]["coordinates"] for f in out["geojson"]["features"] if f["properties"]["geopic:kind"] == "dot"]
    assert math.isclose(c[0], -34.86, abs_tol=1e-4) and math.isclose(c[1], -8.05, abs_tol=1e-4)
    assert math.isclose(n[0], -34.85, abs_tol=1e-4) and math.isclose(n[1], -8.05, abs_tol=1e-4)
    assert math.isclose(e[0], -34.86, abs_tol=1e-4) and math.isclose(e[1], -8.06, abs_tol=1e-4)
    # a syntax error on the caller's line 3 is reported as line 3, not 3 + prelude
    bad = "geo_lon = 0\ngeo_lat = 0\nbox )\n"
    with pytest.raises(rpic.CompileError) as err:
        rpic.geo.compile(bad, features={"A": (0.0, 0.0), "B": (0.001, 0.0)}, warn=False)
    assert err.value.info["line"] == 3
    # and a warning's span too
    src3 = "geo_lon = 0\ngeo_lat = 0\nbox gradient \"red\" \"blue\"\ndot at A"
    out3 = rpic.geo.compile(src3, features={"A": (0.001, 0.0)}, warn=False)
    grad = next(w for w in out3["warnings"] if w["kind"] == "unsupported_style")
    assert grad["span"]["line"] == 3


def test_feature_names_are_validated_never_interpolated():
    src = "geo_lon = 0\ngeo_lat = 0\nbox"
    for bad in ['A; copy "/etc/passwd"', "lower", "1A", "A B", "A\nbox", ""]:
        with pytest.raises(ValueError, match="not a pic label"):
            rpic.geo.compile(src, features={bad: (0.0, 0.0)}, warn=False)
    with pytest.raises(ValueError, match="finite"):
        rpic.geo.compile(src, features={"A": (float("nan"), 0.0)}, warn=False)
    with pytest.raises(ValueError, match="bbox"):
        rpic.geo.compile(src, features=[("A", 0.0, 0.0, (1, 2, 3))], warn=False)
    for bad_form in ["Recife", [("A", 1.0)], [("A", 1.0, 2.0, None, 5)]]:
        with pytest.raises(ValueError, match="features must be"):
            rpic.geo.compile(src, features=bad_form, warn=False)
    # a legitimate name still works
    out = rpic.geo.compile(src, features={"Marco_Zero_1": (0.0, 0.0)}, warn=False)
    assert out["registration"]["control_points"] == 1



def test_preludes_reach_geo_compile_and_feature_injection():
    pre = [("lib", "define spot { [ circle rad 0.1 ] }\nRecife: (0,0)\n")]
    head = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\n"
    fc = rpic.geo.to_geojson(head + "spot()\n", preludes=pre, warn=False)
    assert fc["features"]
    out = rpic.geo.compile(
        head + "arrow from Recife to Olinda\n",
        preludes=pre,
        features={"Olinda": (-34.8553, -8.0089)},
        warn=False,
    )
    assert out["geojson"]["features"]
