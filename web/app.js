// rpic playground: compile pic source in WASM, inject the SVG, and drive the
// animation manifest with GSAP.

import init, { compile, compile_circuits } from './pkg/rpic_wasm.js';

const SAMPLE = `# Kernighan's pipeline, drawn in sequence.
.PS
A: ellipse "document"
arrow
box "PIC"
arrow
box "TBL/EQN" "(optional)" dashed
arrow
box "TROFF"
arrow
Z: ellipse "typesetter"
.PE

animate A          with "pop"  for 0.4
animate 1st arrow  with "draw" for 0.3
animate 1st box    with "pop"  for 0.3
animate 2nd arrow  with "draw" for 0.3
animate 2nd box    with "pop"  for 0.3
animate 3rd arrow  with "draw" for 0.3
animate 3rd box    with "pop"  for 0.3
animate 4th arrow  with "draw" for 0.3
animate Z          with "pop"  for 0.4
`;

const $ = (id) => document.getElementById(id);
const srcEl = $('src');
const stage = $('stage');
const errEl = $('err');
let timeline = null;

await init();
srcEl.value = SAMPLE;
run();

$('run').addEventListener('click', run);
$('replay').addEventListener('click', () => timeline && timeline.restart());
$('circuits').addEventListener('change', run);

function run() {
  let bundle;
  try {
    const useCircuits = document.getElementById('circuits').checked;
    const json = useCircuits ? compile_circuits(srcEl.value) : compile(srcEl.value);
    bundle = JSON.parse(json);
  } catch (e) {
    return showError('internal: ' + e);
  }
  if (bundle.error) return showError(bundle.error);
  hideError();
  // Trusted source: `bundle.svg` is produced by our own WASM compiler (text is
  // XML-escaped in rpic-core::svg) from the user's own input, in their browser.
  stage.innerHTML = bundle.svg;
  play(bundle.animations || []);
}

function showError(msg) {
  errEl.textContent = 'error: ' + msg;
  errEl.style.display = 'block';
}
function hideError() {
  errEl.style.display = 'none';
}

// Build a GSAP timeline from the manifest. Each entry is placed at its absolute
// `start` time (seconds).
function play(anims) {
  if (timeline) timeline.kill();
  timeline = gsap.timeline();
  for (const a of anims) {
    const el = stage.querySelector('#' + CSS.escape(a.id));
    if (!el) continue;
    switch (a.effect) {
      case 'fade':
        timeline.from(el, { opacity: 0, duration: a.duration, ease: 'power1.out' }, a.start);
        break;
      case 'pop':
        timeline.from(
          el,
          { scale: 0, transformOrigin: '50% 50%', duration: a.duration, ease: 'back.out(1.7)' },
          a.start
        );
        break;
      case 'draw':
        drawOn(el, a, timeline);
        break;
      default:
        timeline.from(el, { opacity: 0, duration: a.duration }, a.start);
    }
  }
}

// "draw" effect: stroke-on each drawable child via dash offset animation, with
// any attached text fading in over the same window.
function drawOn(group, a, tl) {
  const strokables = group.querySelectorAll('path, polyline, line, rect, circle, ellipse, polygon');
  strokables.forEach((el) => {
    // Filled, unstroked elements (arrowheads) can't be dash-traced. Pop them
    // in as the shaft reaches the tip instead, matching the package player.
    const fill = el.getAttribute('fill');
    if (
      el.getAttribute('stroke-width') === '0' ||
      (fill && fill !== 'none' && !el.getAttribute('stroke'))
    ) {
      tl.from(
        el,
        {
          opacity: 0,
          scale: 0,
          transformOrigin: '50% 50%',
          duration: Math.min(0.2, a.duration * 0.4),
          ease: 'back.out(1.7)',
        },
        a.start + a.duration * 0.8
      );
      return;
    }
    let len;
    try {
      len = el.getTotalLength();
    } catch {
      len = 0;
    }
    if (len > 0) {
      tl.fromTo(
        el,
        { strokeDasharray: len, strokeDashoffset: len },
        { strokeDashoffset: 0, duration: a.duration, ease: 'none' },
        a.start
      );
    } else {
      tl.from(el, { opacity: 0, duration: a.duration }, a.start);
    }
  });
  const texts = group.querySelectorAll('text');
  if (texts.length) {
    tl.from(texts, { opacity: 0, duration: a.duration * 0.6 }, a.start + a.duration * 0.4);
  }
}
