// Build-time rendering of pic sources by the rpic binary itself — every
// example on the site is compiled by the real engine at build, so code and
// drawing can never drift apart. A broken example fails the build.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, isAbsolute, join, resolve } from 'node:path';

export interface RenderOptions {
  /** load the native circuit-element library (`rpic -c`) */
  circuits?: boolean;
  /** typeset $…$ labels as TeX math (`rpic -t`) */
  texlabels?: boolean;
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
const CACHE_VERSION = 2;

/** One animation manifest entry. The trailing keys ride only when the source
 *  sets them — mirrors `Anim` in bindings/js/index.d.ts and the player here. */
export interface Anim {
  id: string;
  effect: string;
  start: number;
  duration: number;
  repeat?: number;
  yoyo?: boolean;
  ease?: string;
  path?: string;
  color?: string;
  out?: boolean;
  from?: string;
  morph?: string;
  unit?: string;
  chars?: string;
  wiggles?: number;
}

export interface Interaction {
  id: string;
  kind: string;
  inertia?: boolean;
  bounds?: string;
  axis?: string;
}

export interface Bundle {
  svg: string;
  animations: Anim[];
  interactions: Interaction[];
}

/** Render pic source to {svg, animations, interactions} via `rpic --json`. */
export function renderPicBundle(code: string, opts: RenderOptions = {}): Bundle {
  const raw = run(code, opts, true);
  const out = JSON.parse(raw);
  if (out.error) {
    throw new Error(`rpic failed for a docs example (${out.error}).\n--- source ---\n${code}`);
  }
  return { svg: out.svg, animations: out.animations ?? [], interactions: out.interactions ?? [] };
}

/** Render pic source to an SVG string (cached by content+options hash). */
export function renderPic(code: string, opts: RenderOptions = {}): string {
  return run(code, opts, false);
}

/** Render a corpus .pic file (path relative to the repo root) — `copy`
 *  resolves next to the file. Cached by file content + options + literal
 *  copy dependencies + the selected rpic binary signature. */
export function renderPicFile(relPath: string, opts: RenderOptions = {}): string {
  const abs = resolve(process.cwd(), '..', relPath);
  const code = readFileSync(abs, 'utf8');
  const key = cacheKey([
    'file',
    relPath,
    code,
    opts.circuits ?? false,
    opts.texlabels ?? false,
    rpicBinarySignature(),
    copyDependencySignature(abs),
  ]);
  const cached = join(CACHE_DIR, `${key}.svg`);
  if (existsSync(cached)) return readFileSync(cached, 'utf8');
  const args = [
    ...(opts.circuits ? ['-c'] : []),
    ...(opts.texlabels ? ['-t'] : []),
    '--svg',
    abs,
  ];
  const svg = execFileSync(rpicBin(), args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  mkdirSync(CACHE_DIR, { recursive: true });
  writeFileSync(cached, svg);
  return svg;
}

function run(code: string, opts: RenderOptions, json: boolean): string {
  const key = cacheKey([
    'inline',
    code,
    opts.circuits ?? false,
    opts.texlabels ?? false,
    json,
    rpicBinarySignature(),
  ]);
  const cached = join(CACHE_DIR, `${key}.${json ? 'json' : 'svg'}`);
  if (existsSync(cached)) return readFileSync(cached, 'utf8');

  const src = join(tmpdir(), `rpic-doc-${key}.pic`);
  writeFileSync(src, code.endsWith('\n') ? code : code + '\n');
  const args = [
    ...(opts.circuits ? ['-c'] : []),
    ...(opts.texlabels ? ['-t'] : []),
    json ? '--json' : '--svg',
    src,
  ];
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

function cacheKey(parts: unknown[]): string {
  return createHash('sha256')
    .update(JSON.stringify([CACHE_VERSION, process.env.RPIC_CACHE_BUST ?? '', ...parts]))
    .digest('hex')
    .slice(0, 24);
}

function rpicBinarySignature(): unknown {
  const bin = rpicBin();
  const resolved = resolveExecutablePath(bin);
  if (!resolved) {
    throw new Error(
      `rpic binary not found: ${bin}. Set RPIC_BIN or put rpic on PATH before building docs.`
    );
  }

  const stat = statSync(resolved);
  return {
    bin,
    path: realpathSync(resolved),
    size: stat.size,
    mtimeMs: Math.trunc(stat.mtimeMs),
  };
}

function resolveExecutablePath(bin: string): string | null {
  if (isAbsolute(bin) || bin.includes('/') || bin.includes('\\')) {
    const path = isAbsolute(bin) ? bin : resolve(process.cwd(), bin);
    return existsSync(path) ? path : null;
  }

  for (const dir of (process.env.PATH ?? '').split(delimiter)) {
    if (!dir) continue;
    for (const name of executableNames(bin)) {
      const path = join(dir, name);
      if (existsSync(path)) return path;
    }
  }
  return null;
}

function executableNames(bin: string): string[] {
  if (process.platform !== 'win32' || /\.[^\\/]+$/.test(bin)) return [bin];
  const exts = (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM').split(';').filter(Boolean);
  return [bin, ...exts.map((ext) => `${bin}${ext}`)];
}

function copyDependencySignature(abs: string): unknown[] {
  return collectCopyDependencies(abs, new Set([canonicalPath(abs)]));
}

function collectCopyDependencies(abs: string, seen: Set<string>): unknown[] {
  const code = readFileSync(abs, 'utf8');
  const deps: unknown[] = [];

  for (const include of copyIncludes(code)) {
    const depPath = isAbsolute(include) ? include : resolve(dirname(abs), include);
    const key = canonicalPath(depPath);
    if (seen.has(key)) continue;
    seen.add(key);

    if (!existsSync(depPath)) {
      deps.push({ path: key, missing: true });
      continue;
    }

    const depCode = readFileSync(depPath, 'utf8');
    deps.push({ path: key, code: depCode });
    deps.push(...collectCopyDependencies(depPath, seen));
  }

  return deps;
}

function copyIncludes(code: string): string[] {
  const includes: string[] = [];
  const re = /\bcopy\s+"((?:\\.|[^"\\])*)"/g;
  const source = stripPicComments(code);
  let match: RegExpExecArray | null;

  while ((match = re.exec(source))) {
    includes.push(match[1].replace(/\\(["\\])/g, '$1'));
  }

  return includes;
}

function stripPicComments(code: string): string {
  let out = '';
  let inString = false;
  let escaped = false;

  for (let i = 0; i < code.length; i++) {
    const ch = code[i];

    if (inString) {
      out += ch;
      if (escaped) {
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }

    if (ch === '"') {
      inString = true;
      out += ch;
      continue;
    }

    if (ch === '#') {
      while (i < code.length && code[i] !== '\n') i++;
      if (i < code.length) out += '\n';
      continue;
    }

    out += ch;
  }

  return out;
}

function canonicalPath(path: string): string {
  return existsSync(path) ? realpathSync(path) : path;
}
