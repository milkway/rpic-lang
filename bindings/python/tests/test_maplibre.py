"""rpic.geo.maplibre_layers / compile(layers=True): the engine's MapLibre
layers (docs/geopic.md § 6), pinned to the snapshots in
examples/geo/maplibre/ — the layer specs rpic.dev drew before the styling
moved into geopic-core."""

import json
import math
import pathlib

import pytest

import rpic

GEO = pathlib.Path(__file__).resolve().parents[3] / "examples" / "geo"
IDS = ["pic-fill", "pic-stroke", "pic-stroke-dashed", "pic-dot", "pic-text", "pic-title"]
SRC = "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 400\nB: box \"hub\"\ndot at (2, 0)"


def close(a, b, path=""):
    """Deep equality, numbers within a relative 1e-12 (cos φ is the platform libm's)."""
    if isinstance(a, (int, float)) and not isinstance(a, bool):
        assert isinstance(b, (int, float)) and not isinstance(b, bool), path
        assert math.isclose(a, b, rel_tol=1e-12, abs_tol=0.0), f"{path}: {a} vs {b}"
    elif isinstance(a, list):
        assert isinstance(b, list) and len(a) == len(b), path
        for i, (x, y) in enumerate(zip(a, b)):
            close(x, y, f"{path}[{i}]")
    elif isinstance(a, dict):
        assert isinstance(b, dict) and sorted(a) == sorted(b), path
        for k in a:
            close(a[k], b[k], f"{path}.{k}")
    else:
        assert a == b, path


def cases():
    out = {p.stem: json.loads(p.read_text()) for p in sorted(GEO.glob("*.geojson"))}
    assert len(out) >= 4
    merc = json.loads(json.dumps(out["callout"]))
    merc["geopic"]["registration"]["frame"] = "mercator"
    out["callout-mercator"] = merc
    out["empty"] = {"type": "FeatureCollection", "features": []}
    return out


@pytest.mark.parametrize("name,fc", list(cases().items()))
def test_layers_match_the_snapshots(name, fc):
    layers = rpic.geo.maplibre_layers(fc)
    assert [l["id"] for l in layers] == IDS
    assert all(l["source"] == "pic" for l in layers)
    close(layers, json.loads((GEO / "maplibre" / f"{name}.layers.json").read_text()), name)
    # JSON text in, same layers out
    assert rpic.geo.maplibre_layers(json.dumps(fc)) == layers


def test_compile_layers_true_adds_layers_only_when_asked():
    plain = rpic.geo.compile(SRC)
    assert "layers" not in plain
    out = rpic.geo.compile(SRC, layers=True)
    assert out["geojson"] == plain["geojson"]
    assert out["layers"] == rpic.geo.maplibre_layers(out["geojson"])
    # ground-true sizes: 400 m/in at lat −8.0631, the z0 factor of line-width
    lw = out["layers"][1]["paint"]["line-width"]
    p0 = 1 / (78271.517 * math.cos(math.radians(-8.0631)))
    assert lw[:3] == ["interpolate", ["exponential", 2], ["zoom"]]
    assert math.isclose(lw[4][2][2], p0, rel_tol=1e-12)
    # through feature injection too
    inj = rpic.geo.compile(
        "geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\narrow from Recife to Olinda",
        features={"Recife": (-34.8711, -8.0631), "Olinda": (-34.8553, -8.0089)},
        layers=True,
    )
    assert [l["id"] for l in inj["layers"]] == IDS


def test_bad_input_is_a_geo_error():
    with pytest.raises(rpic.GeoError) as e:
        rpic.geo.maplibre_layers({"type": "Feature"})
    assert e.value.info["kind"] == "invalid_geojson"
    with pytest.raises(rpic.GeoError):
        rpic.geo.maplibre_layers("not json")
