// Build a studio.rpic.dev share link for a pic source. Studio carries the
// source lz-string-compressed in the URL fragment (`#pic=…`, decoded
// client-side) and auto-detects `-c`/`-t` from the source, so no flags ride
// along. A source that `copy`s a *local* shim (the m4 circuit_macros / lib3D
// compat files) can't resolve in the browser, so it gets no link — but the
// reserved in-source include `copy "circuits"` works under wasm and is fine.
import lzString from 'lz-string'; // CommonJS module — default-import for ESM interop

const { compressToEncodedURIComponent } = lzString;

const STUDIO = 'https://studio.rpic.dev/';

/** A `copy "X"` whose target is anything other than the reserved "circuits". */
function copiesLocalShim(source: string): boolean {
  const re = /\bcopy\s+"((?:\\.|[^"\\])*)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source))) {
    if (m[1] !== 'circuits') return true;
  }
  return false;
}

/** Studio share URL for `source`, or `null` when it can't resolve there. */
export function studioUrl(source: string): string | null {
  if (copiesLocalShim(source)) return null;
  return `${STUDIO}#pic=${compressToEncodedURIComponent(source)}`;
}
