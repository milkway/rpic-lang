"""rpic.geo — pic drawings as GeoJSON / GeoDataFrames.

A drawing is placed on the Earth by a similarity declared in the source
(``geo_lon``, ``geo_lat``, ``geo_scale``, ``geo_bearing``, ``geo_frame``) or
recovered from control points (``<Label>_lon`` / ``<Label>_lat``); every
argument here overrides the corresponding variable, like ``-D`` in troff.
Format, properties and warnings: ``docs/geopic.md`` in the repository.

    import rpic
    fc = rpic.geo.to_geojson(src)                 # dict (a FeatureCollection)
    out = rpic.geo.compile(src)                   # {"geojson", "warnings", "diagnostics", "registration", "canvas"}
    out = rpic.geo.compile(src, layers=True)      # … + "layers": the MapLibre layers that draw it
    out = rpic.geo.compile(src, layers=True, text_scale=1.5)  # … with labels 1.5× larger
    layers = rpic.geo.maplibre_layers(fc)         # the same, for any geopic FeatureCollection
    gdf = rpic.geo.to_geodataframe(src)           # needs `pip install "rpiclang[geo]"`
"""

from __future__ import annotations

import json
import re
import warnings as _warnings
from typing import Any, Iterable, Mapping

from . import _native

GeoError = _native.GeoError
CompileError = _native.CompileError

__all__ = [
    "GeoError",
    "GeoWarning",
    "compile",
    "maplibre_layers",
    "to_geojson",
    "to_geodataframe",
]

_COMPILE_KEYS = ("circuits", "texlabels", "base", "include_policy", "max_loop_iterations", "max_shapes", "preludes")


class GeoWarning(UserWarning):
    """A non-fatal loss or substitution while georeferencing (never silent).

    ``warning.kind`` / ``warning.info`` carry the structured warning.
    """

    def __init__(self, info: Mapping[str, Any]):
        self.info = dict(info)
        self.kind = self.info.get("kind")
        super().__init__(f"{self.kind}: {self.info.get('message')}")


FeatureLike = Mapping[str, Any] | Iterable[Any] | None
_Feature = tuple[str, float, float, tuple[float, float, float, float] | None]

# pic labels: an uppercase letter, then letters, digits or underscores. Feature
# names are written into pic source, so anything else is rejected outright —
# never interpolated.
_LABEL = re.compile(r"^[A-Z][A-Za-z0-9_]*$")
_FORMS = (
    "features must be a {name: (lon, lat[, bbox])} mapping, an iterable of "
    "(name, lon, lat[, bbox]) tuples, or a GeoDataFrame indexed by name"
)


def _label(name: Any) -> str:
    name = str(name)
    if not _LABEL.match(name):
        raise ValueError(
            f"feature name {name!r} is not a pic label (an uppercase letter followed by letters, digits or underscores)"
        )
    return name


def _coord(v: Any, what: str) -> float:
    try:
        f = float(v)
    except (TypeError, ValueError) as e:
        raise ValueError(f"feature {what} must be a number, got {v!r}") from e
    if f != f or f in (float("inf"), float("-inf")):
        raise ValueError(f"feature {what} must be a finite number, got {v!r}")
    return f


def _bbox(name: str, bbox: Any) -> tuple[float, float, float, float] | None:
    if bbox is None:
        return None
    vals = tuple(_coord(v, f"{name} bbox") for v in bbox)
    if len(vals) != 4:
        raise ValueError(f"feature {name!r}: bbox must be (west, south, east, north)")
    return vals  # type: ignore[return-value]


def _normalize_features(features: FeatureLike) -> list[_Feature]:
    """``features`` as a list of ``(name, lon, lat, bbox)``.

    Accepts a ``dict`` ``{name: (lon, lat[, bbox])}``, any iterable of
    ``(name, lon, lat[, bbox])`` tuples, or a GeoDataFrame (``geopandas``)
    whose index holds the names: the centroid is the point and the bounds
    the bbox.
    """
    if features is None:
        return []
    if isinstance(features, (str, bytes)):
        raise ValueError(_FORMS)
    out: list[_Feature] = []
    # GeoDataFrame (duck-typed so geopandas stays optional)
    geom = getattr(features, "geometry", None)
    if geom is not None and hasattr(features, "index") and hasattr(geom, "centroid"):
        gdf = features
        if gdf.crs is not None and str(gdf.crs).upper() not in ("EPSG:4326", "OGC:CRS84"):
            gdf = gdf.to_crs("EPSG:4326")
        for name, g in zip(gdf.index, gdf.geometry):
            if g is None or getattr(g, "is_empty", False):
                raise ValueError(f"feature {name!r} has no geometry")
            c = g.centroid
            out.append((_label(name), float(c.x), float(c.y), _bbox(str(name), g.bounds)))
        return out
    if isinstance(features, Mapping):
        items: Iterable[Any] = ((name, *pos) for name, pos in features.items())
    else:
        items = features
    for item in items:
        try:
            n = len(item)
        except TypeError as e:
            raise ValueError(_FORMS) from e
        if n not in (3, 4) or isinstance(item, (str, bytes)):
            raise ValueError(_FORMS)
        name = _label(item[0])
        out.append((name, _coord(item[1], f"{name} lon"), _coord(item[2], f"{name} lat"), _bbox(name, item[3] if n == 4 else None)))
    return out


def _shift(d: Any, k: int) -> None:
    """Re-base one position (``line``/``file``) past the ``k`` prelude lines.
    A position *inside* the prelude is not the caller's: it is tagged
    ``file = "features"`` (``line`` stays relative to the prelude)."""
    if not k or not isinstance(d, dict) or d.get("file") is not None:
        return
    line = d.get("line")
    if not isinstance(line, int) or line < 1:
        return
    if line > k:
        d["line"] = line - k
    else:
        d["file"] = "features"


def _rebase(out: dict, k: int) -> dict:
    """Shift diagnostics back by the ``k`` prelude lines the host prepended,
    so positions refer to the caller's own source (positions inside an
    include/library carry a ``file`` and are left alone)."""
    for w in out.get("warnings", []):
        _shift(w["span"] if isinstance(w.get("span"), dict) else w, k)
    return out


def _compile(src: str, k: int, **kwargs: Any) -> dict:
    """``_native.geo_compile`` with diagnostics re-based by ``k`` lines."""
    try:
        return _rebase(_native.geo_compile(src, **kwargs), k)
    except CompileError as e:
        _shift(getattr(e, "info", None), k)
        raise


def _prelude(reg: Mapping[str, Any], canvas: Mapping[str, Any], feats: list[_Feature]) -> str:
    """pic statements defining every feature at its position, in the source's
    own units (pic ``scale``, before the whole-picture ``rescale``).

    A feature with a bbox becomes an invisible labelled box spanning the
    axis-aligned canvas extent of its four corners, so it covers the feature
    under any bearing; a point feature a plain place. Compass corners are
    canvas directions, as always in pic: under a bearing ``Name.n`` is the
    edge in the canvas +y direction, not the feature's northern edge.
    """
    if not feats:
        return ""
    units = canvas.get("scale", 1.0) / canvas.get("rescale", 1.0)  # source units per emitted inch
    pts: list[tuple[float, float]] = []
    for _, lon, lat, bbox in feats:
        pts.append((lon, lat))
        if bbox is not None:
            w, s, e, n = bbox
            pts.extend([(w, s), (e, s), (e, n), (w, n)])
    canvas_pts = iter(_native.geo_to_canvas_many(json.dumps(reg), pts))
    lines = []
    for name, _, _, bbox in feats:
        x, y = next(canvas_pts)
        if bbox is None:
            lines.append(f"{name}: ({x * units:.9g}, {y * units:.9g})")
        else:
            corners = [next(canvas_pts) for _ in range(4)]
            xs = [c[0] for c in corners]
            ys = [c[1] for c in corners]
            wid, ht = (max(xs) - min(xs)) * units, (max(ys) - min(ys)) * units
            lines.append(f"{name}: box invis wid {wid:.9g} ht {ht:.9g} at ({x * units:.9g}, {y * units:.9g})")
    return "\n".join(lines) + "\n"


def _compile_with_features(src: str, feats: list[_Feature], kwargs: dict) -> dict:
    """Two passes. Features the source itself defines (as places or labelled
    objects) are control points of the source's own geometry; the others are
    injected — placed by the registration of a first pass — as places or
    invisible boxes, then everything is georeferenced together."""
    names = [f[0] for f in feats]
    k = len(feats)
    placeholders = "".join(f"{n}: (0, 0)\n" for n in names)
    # which names does the source define? their definitions sit after the
    # placeholder lines (a span is relative to the caller's text + prelude)
    compile_kw = {key: kwargs[key] for key in _COMPILE_KEYS if key in kwargs}
    try:
        sc = _native.scene(placeholders + src, **compile_kw)
    except CompileError as e:
        _shift(getattr(e, "info", None), k)
        raise
    def own_def(span: Any) -> bool:
        # past the placeholders, or inside a named prelude (``file`` set)
        span = span or {}
        return span.get("file") is not None or span.get("line", 0) > k

    defined = set()
    for p in sc.get("places", []):
        if p.get("label") in names and own_def(p.get("span")):
            defined.add(p["label"])
    for it in sc.get("items", []):
        if it.get("label") in names and own_def(it.get("span")):
            defined.add(it["label"])
    own = [f for f in feats if f[0] in defined]
    inject = [f for f in feats if f[0] not in defined]
    # pass 1: the registration — declared anchor, or the source's control
    # points, the source-defined features among them. Placeholders only for
    # the features nobody defines: a placeholder sits in the caller's text,
    # after any prelude, and would clobber a prelude's definition (last
    # label wins); the source's own definitions need none either.
    holders = "".join(f"{f[0]}: (0, 0)\n" for f in inject)
    first = _compile(holders + src, len(inject), features=own, **kwargs)
    if not inject:
        return first
    prelude = _prelude(first["registration"], first["canvas"], inject)
    out = _compile(prelude + src, prelude.count("\n"), features=feats, **kwargs)
    if first["registration"]["method"] == "anchor" and first["registration"]["control_points"] == 0:
        # the injected features were placed *by* the declared values; the
        # second pass's "declared values ignored" would blame the source
        out["warnings"] = [w for w in out["warnings"] if w.get("kind") != "ignored_with_control_points"]
    return out


def compile(
    src: str,
    *,
    warn: bool = True,
    features: FeatureLike = None,
    layers: bool = False,
    text_scale: float | None = None,
    _stacklevel: int = 2,
    **kwargs: Any,
) -> dict:
    """Georeference ``src``: ``{"geojson": dict, "warnings": [dict], "diagnostics": [str], "registration": dict, "canvas": dict}``.

    ``layers=True`` adds ``"layers"``: the MapLibre style-spec layers that
    draw the collection with sizes true to the ground (see
    :func:`maplibre_layers`); ``text_scale`` multiplies every label's font
    size. It is validated before anything compiles: a bad value raises
    :class:`GeoError` (``invalid_options``); passing it without
    ``layers=True`` raises :class:`ValueError`.

    Keyword arguments: ``frame`` (``"enu"`` | ``"mercator"``), ``anchor``
    ``(lon, lat)``, ``scale`` (metres per inch), ``bearing`` (degrees,
    clockwise from north), ``features``, ``flatten`` (inches, default
    ``1e-3``), ``view_scale`` (px per inch under ``geo_px = 1``),
    ``screen_classes``, plus the usual compile options (``circuits``,
    ``texlabels``, ``base``, ``include_policy``, ``max_loop_iterations``,
    ``max_shapes``, ``preludes`` — named source units compiled before
    ``src``, as in :func:`rpic.compile`).

    ``features`` names points the source may use without defining them
    (``arrow from Recife to Olinda``): features the source does define act as
    control points of its own geometry; the others are projected by a first
    pass's registration and injected as places — invisible labelled boxes
    when they have a bbox, so ``Name.n``/``Name.e`` follow its extent — before
    the final pass. Diagnostics keep the caller's line numbers. Every warning
    is also raised as :class:`GeoWarning` unless ``warn=False``. Raises
    :class:`rpic.CompileError` or :class:`GeoError`.
    """
    if text_scale is not None:
        if not layers:
            raise ValueError("text_scale only applies with layers=True")
        _native.validate_layer_options(_layer_options(text_scale))  # fail fast
    feats = _normalize_features(features)
    out = _compile_with_features(src, feats, kwargs) if feats else _native.geo_compile(src, **kwargs)
    if layers:
        out["layers"] = maplibre_layers(out["geojson"], text_scale=1.0 if text_scale is None else text_scale)
    if warn:
        for w in out["warnings"]:
            _warnings.warn(GeoWarning(w), stacklevel=_stacklevel)
    return out


def maplibre_layers(fc: Mapping[str, Any] | str, text_scale: float = 1.0) -> list:
    """The MapLibre style-spec layers that draw a geopic FeatureCollection.

    ``fc`` is the collection (a dict, as :func:`to_geojson` returns it, or its
    JSON text). The result is a list of six layer dicts — ``pic-fill``,
    ``pic-stroke``, ``pic-stroke-dashed``, ``pic-dot``, ``pic-text``,
    ``pic-title``, in draw order — that all read one GeoJSON source named
    ``"pic"``: add the collection as that source and the layers as they are
    (MapLibre GL JS, ``mapgl`` in R, ``maplibre`` for Python…). Every stroke
    width, dot radius and font size is an ``interpolate`` on zoom that keeps
    it true to the ground, from the collection's ``geopic.registration``
    (``docs/geopic.md`` § 6). Only that member and the features'
    ``geopic:font_size`` are read.

    ``text_scale`` multiplies every label's font size (a finite number in
    ``(0, 16]``; 1 = true to the ground). The upper zoom stop of
    ``text-size`` is chosen from the scaled font, so the largest label still
    fits MapLibre's 255 px symbol limit; strokes and dots do not change
    (DECISIONS G11).

    Raises :class:`GeoError` (``invalid_geojson``) when ``fc`` is not a
    FeatureCollection, (``invalid_options``) when ``text_scale`` is out of
    range or not a number.
    """
    text = fc if isinstance(fc, str) else json.dumps(fc)
    return json.loads(_native.maplibre_layers(text, _layer_options(text_scale)))


def _layer_options(text_scale: Any) -> str:
    """The engine's layer-options JSON. The engine validates it (NaN/inf
    serialise as non-JSON tokens it rejects as ``invalid_options``)."""
    return json.dumps({"text_scale": text_scale})


def to_geojson(src: str, **kwargs: Any) -> dict:
    """The GeoJSON ``FeatureCollection`` for ``src`` as a plain dict."""
    kwargs.setdefault("_stacklevel", 3)
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
    kwargs.setdefault("_stacklevel", 3)
    fc = compile(src, **kwargs)["geojson"]
    return geopandas.GeoDataFrame.from_features(fc["features"], crs="EPSG:4326")
