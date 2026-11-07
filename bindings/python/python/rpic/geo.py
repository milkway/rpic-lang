"""rpic.geo — pic drawings as GeoJSON / GeoDataFrames.

A drawing is placed on the Earth by a similarity declared in the source
(``geo_lon``, ``geo_lat``, ``geo_scale``, ``geo_bearing``, ``geo_frame``) or
recovered from control points (``<Label>_lon`` / ``<Label>_lat``); every
argument here overrides the corresponding variable, like ``-D`` in troff.
Format, properties and warnings: ``docs/geopic.md`` in the repository.

    import rpic
    fc = rpic.geo.to_geojson(src)                 # dict (a FeatureCollection)
    out = rpic.geo.compile(src)                   # {"geojson", "warnings", "registration"}
    gdf = rpic.geo.to_geodataframe(src)           # needs `pip install "rpiclang[geo]"`
"""

from __future__ import annotations

import json
import warnings as _warnings
from typing import Any, Iterable, Mapping

from . import _native

GeoError = _native.GeoError

__all__ = [
    "GeoError",
    "GeoWarning",
    "compile",
    "to_geojson",
    "to_geodataframe",
]


class GeoWarning(UserWarning):
    """A non-fatal loss or substitution while georeferencing (never silent).

    ``warning.kind`` / ``warning.info`` carry the structured warning.
    """

    def __init__(self, info: Mapping[str, Any]):
        self.info = dict(info)
        self.kind = self.info.get("kind")
        super().__init__(f"{self.kind}: {self.info.get('message')}")


FeatureLike = Mapping[str, Any] | Iterable[Any] | None


def _normalize_features(features: FeatureLike) -> list[tuple[str, float, float, tuple | None]]:
    """``features`` as a list of ``(name, lon, lat, bbox)``.

    Accepts a ``dict`` ``{name: (lon, lat)}``, any iterable of ``(name, lon,
    lat)`` tuples, or a GeoDataFrame (``geopandas``) whose index holds the
    names: the centroid is the point and the bounds the bbox.
    """
    if features is None:
        return []
    out: list[tuple[str, float, float, tuple | None]] = []
    # GeoDataFrame (duck-typed so geopandas stays optional)
    geom = getattr(features, "geometry", None)
    if geom is not None and hasattr(features, "index") and hasattr(geom, "centroid"):
        gdf = features
        if gdf.crs is not None and str(gdf.crs).upper() not in ("EPSG:4326", "OGC:CRS84"):
            gdf = gdf.to_crs("EPSG:4326")
        for name, g in zip(gdf.index, gdf.geometry):
            c = g.centroid
            w, s, e, n = g.bounds
            out.append((str(name), float(c.x), float(c.y), (float(w), float(s), float(e), float(n))))
        return out
    if isinstance(features, Mapping):
        for name, pos in features.items():
            lon, lat = pos[0], pos[1]
            bbox = tuple(pos[2]) if len(pos) > 2 and pos[2] is not None else None
            out.append((str(name), float(lon), float(lat), bbox))
        return out
    for item in features:
        name, lon, lat = item[0], item[1], item[2]
        bbox = tuple(item[3]) if len(item) > 3 and item[3] is not None else None
        out.append((str(name), float(lon), float(lat), bbox))
    return out


def _prelude(reg: Mapping[str, Any], feats: list[tuple[str, float, float, tuple | None]]) -> str:
    """pic statements that define every feature at its canvas position.

    A feature with a bbox becomes an invisible labelled box (so ``Name.n``,
    ``Name.e`` … follow its extent); a point feature a plain place.
    """
    reg_json = json.dumps(reg)
    lines = []
    for name, lon, lat, bbox in feats:
        x, y = _native.geo_to_canvas(reg_json, lon, lat)
        if bbox is None:
            lines.append(f"{name}: ({x:.9g}, {y:.9g})")
        else:
            w, s, e, n = bbox
            x0, y0 = _native.geo_to_canvas(reg_json, w, s)
            x1, y1 = _native.geo_to_canvas(reg_json, e, n)
            wid, ht = abs(x1 - x0), abs(y1 - y0)
            lines.append(f"{name}: box invis wid {wid:.9g} ht {ht:.9g} at ({x:.9g}, {y:.9g})")
    return "\n".join(lines) + "\n" if lines else ""


def compile(src: str, *, warn: bool = True, features: FeatureLike = None, **kwargs: Any) -> dict:
    """Georeference ``src``: ``{"geojson": dict, "warnings": [dict], "registration": dict}``.

    Keyword arguments: ``frame`` (``"enu"`` | ``"mercator"``), ``anchor``
    ``(lon, lat)``, ``scale`` (metres per inch), ``bearing`` (degrees,
    clockwise from north), ``features``, ``flatten`` (inches, default
    ``1e-3``), ``view_scale`` (px per inch under ``geo_px = 1``),
    ``screen_classes``, plus the usual compile options (``circuits``,
    ``texlabels``, ``base``, ``include_policy``, ``max_loop_iterations``,
    ``max_shapes``).

    ``features`` places named points on the canvas **before** compiling, so
    the source can say ``arrow from Recife to Olinda`` without defining
    them: the registration (anchor or source control points) is obtained
    from a first pass, each feature is projected to its canvas position and
    injected as a place (or an invisible box when it has a bbox), and the
    second pass georeferences the result with the features as control
    points. Every warning is also raised as a :class:`GeoWarning` unless
    ``warn=False``. Raises :class:`rpic.CompileError` or :class:`GeoError`.
    """
    feats = _normalize_features(features)
    if feats:
        # pass 1: the registration, with the features as placeholders so the
        # source compiles even where it refers to them
        placeholders = "".join(f"{name}: (0, 0)\n" for name, *_ in feats)
        first = _native.geo_compile(placeholders + src, **kwargs)
        src = _prelude(first["registration"], feats) + src
        out = _native.geo_compile(src, features=feats, **kwargs)
        # the features were placed *by* the declared registration, so the
        # fit reproduces it: the "declared values ignored" warnings of the
        # second pass would blame the source for nothing
        out["warnings"] = [w for w in out["warnings"] if w.get("kind") != "ignored_with_control_points"]
    else:
        out = _native.geo_compile(src, **kwargs)
    if warn:
        for w in out["warnings"]:
            _warnings.warn(GeoWarning(w), stacklevel=2)
    return out


def to_geojson(src: str, **kwargs: Any) -> dict:
    """The GeoJSON ``FeatureCollection`` for ``src`` as a plain dict."""
    return compile(src, **kwargs)["geojson"]


def to_geodataframe(src: str, **kwargs: Any):
    """The features of ``src`` as a ``geopandas.GeoDataFrame`` (EPSG:4326).

    Columns are the feature properties: simplestyle keys (``stroke``,
    ``fill``, …) and ``geopic:*``. Needs ``geopandas``::

        pip install "rpiclang[geo]"
    """
    try:
        import geopandas  # noqa: F401
    except ImportError as e:  # pragma: no cover - exercised in CI without geopandas
        raise ImportError(
            "rpic.geo.to_geodataframe needs geopandas: pip install \"rpiclang[geo]\""
        ) from e
    fc = compile(src, **kwargs)["geojson"]
    return geopandas.GeoDataFrame.from_features(fc["features"], crs="EPSG:4326")
