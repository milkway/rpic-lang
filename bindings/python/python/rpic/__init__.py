"""rpic — the pic graphics language rendered to SVG, PNG and PDF.

The engine lives in the native extension ``rpic._native``; this package
re-exports it and adds the pure-Python ``rpic.geo`` layer (maps).
"""

from . import _native
from ._native import (  # noqa: F401
    CompileError,
    GeoError,
    __version__,
    compile,
    compile_json,
    render_pdf,
    render_png,
    render_svg,
    scene,
)
from . import geo  # noqa: E402,F401

__all__ = [
    "CompileError",
    "GeoError",
    "__version__",
    "compile",
    "compile_json",
    "geo",
    "render_pdf",
    "render_png",
    "render_svg",
    "scene",
]

__doc__ = _native.__doc__ or __doc__
