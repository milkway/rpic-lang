# geopic-core

Georeference an [rpic](https://rpic.dev) scene: turn a pic drawing into a
GeoJSON `FeatureCollection` (WGS84).

```rust
use geopic_core::{georeference, GeoOptions};

let scene = rpic_core::scene_json(&rpic_core::compile(src)?, &Default::default()).json;
let geo = georeference(&scene, &GeoOptions::default())?;
println!("{}", geo.features.to_json());      // GeoJSON text
for w in &geo.warnings { eprintln!("{}: {}", w.kind, w.message); }
```

The drawing is placed on the Earth by a four-parameter similarity — anchor,
scale (metres per inch), bearing — declared in the pic source
(`geo_lon`, `geo_lat`, `geo_scale`, `geo_bearing`, `geo_frame`) or recovered
from control points (`<Label>_lon` / `<Label>_lat` for labelled places or
objects). Geometry is densified in the plane and carried to longitude/latitude
through a local tangent plane (`enu`, Karney geodesics) or Web Mercator.

Pure Rust, no I/O, no `unsafe`, builds for `wasm32-unknown-unknown`. The
engine (`rpic-core`) does not depend on this crate; it is a dev-dependency here
only so the tests can compile real pic sources.

Format, properties, warnings and versioning: `docs/geopic.md` in the
repository.
