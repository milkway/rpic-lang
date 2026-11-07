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


def test_compile_returns_registration_and_structured_warnings():
    out = rpic.geo.compile("geo_lon = 0\ngeo_lat = 0\nbox gradient \"red\" \"blue\"", warn=False)
    assert set(out) == {"geojson", "warnings", "registration"}
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
    assert len(gdf) == 3
    assert set(gdf.geometry.geom_type) == {"Polygon", "LineString"}
