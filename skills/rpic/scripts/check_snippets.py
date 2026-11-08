#!/usr/bin/env python3
"""Render every ```pic fenced block of the skill's reference files with the real
rpic binary and report the ones that fail. A fence may carry flags after the
language: ```pic -c   ```pic -t   ```pic theme=light   (space separated).

    python3 scripts/check_snippets.py            # all references
    python3 scripts/check_snippets.py --png DIR  # also write PNGs for a contact sheet
"""
import re, subprocess, sys, pathlib, tempfile, os

here = pathlib.Path(__file__).resolve().parent.parent
refs = sorted((here / "references").glob("*.md")) + [here / "SKILL.md"]
fence = re.compile(r"^```pic([^\n]*)\n(.*?)^```", re.M | re.S)
png_dir = None
if "--png" in sys.argv:
    png_dir = pathlib.Path(sys.argv[sys.argv.index("--png") + 1]); png_dir.mkdir(parents=True, exist_ok=True)

ok = bad = 0
for ref in refs:
    text = ref.read_text()
    for n, m in enumerate(fence.finditer(text), 1):
        opts, body = m.group(1).split(), m.group(2)
        if "skip" in opts:
            continue
        flags = [o for o in opts if o in ("-c", "-t")]
        theme = next((o.split("=")[1] for o in opts if o.startswith("theme=")), None)
        if theme:
            flags += ["--prelude", f"theme={here}/assets/editorial-{theme}.pic"]
        with tempfile.TemporaryDirectory() as td:
            src = pathlib.Path(td) / "snippet.pic"; src.write_text(body)
            r = subprocess.run(["rpic", *flags, "--json", str(src)], capture_output=True, text=True)
            name = f"{ref.stem}-{n:02d}"
            import json
            doc = json.loads(r.stdout) if r.returncode == 0 and r.stdout.strip().startswith("{") else {}
            # a compile error rides in the JSON with exit 0: {"error": "...", "error_info": {...}}
            if r.returncode != 0 or "error" in doc:
                msg = doc.get("error") or (r.stderr.strip().splitlines()[-1] if r.stderr.strip() else r.stdout[:200])
                bad += 1; print(f"FAIL {name}: {msg}")
                continue
            warns = doc.get("warnings", [])
            for w in warns:
                print(f"warn {name}: {w.get('kind')} {w.get('message')}")
            ok += 1
            if png_dir:
                subprocess.run(["rpic", *flags, "--png", "--scale", "2", "-o", str(png_dir / f"{name}.png"), str(src)], check=False)
print(f"{ok} ok, {bad} failed")
sys.exit(1 if bad else 0)
