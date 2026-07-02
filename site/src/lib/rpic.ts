// Build-time rendering of pic sources by the rpic binary itself — every
// example on the site is compiled by the real engine at build, so code and
// drawing can never drift apart. A broken example fails the build.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

export interface RenderOptions {
  /** load the native circuit-element library (`rpic -c`) */
  circuits?: boolean;
}

/** Resolve the rpic binary: $RPIC_BIN, the workspace release build, or PATH. */
function rpicBin(): string {
  if (process.env.RPIC_BIN) return process.env.RPIC_BIN;
  // anchored at the site dir (astro runs with cwd = site/), not the bundle
  const workspace = resolve(process.cwd(), '../target/release/rpic');
  if (existsSync(workspace)) return workspace;
  return 'rpic'; // hope for PATH; execFileSync will throw a clear ENOENT otherwise
}

const CACHE_DIR = resolve(process.cwd(), 'node_modules/.rpic-cache');

/** Render pic source to an SVG string (cached by content+options hash). */
export function renderPic(code: string, opts: RenderOptions = {}): string {
  const key = createHash('sha256')
    .update(JSON.stringify([code, opts.circuits ?? false]))
    .digest('hex')
    .slice(0, 24);
  const cached = join(CACHE_DIR, `${key}.svg`);
  if (existsSync(cached)) return readFileSync(cached, 'utf8');

  const src = join(tmpdir(), `rpic-doc-${key}.pic`);
  writeFileSync(src, code.endsWith('\n') ? code : code + '\n');
  const args = [...(opts.circuits ? ['-c'] : []), '--svg', src];
  let svg: string;
  try {
    svg = execFileSync(rpicBin(), args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  } catch (e: any) {
    const stderr = e?.stderr?.toString?.() ?? '';
    throw new Error(
      `rpic failed for a docs example (${stderr.trim() || e.message}).\n--- source ---\n${code}`
    );
  } finally {
    rmSync(src, { force: true });
  }

  mkdirSync(CACHE_DIR, { recursive: true });
  writeFileSync(cached, svg);
  return svg;
}
