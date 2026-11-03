// 絵文字の字の切り替えが browser で効くかを、画素の比べで判じる（TASK-410）。
// macOS の runner で、本物の Safari（safaridriver）と Playwright の Chromium、Firefox、WebKit を撃つ。
//
//   node run.mjs <apps/frontend の道> <出力の dir> [browser,...]
//
// 字の並び（variant）ごと、字（os、noto、twemoji）ごとに同じ見本を撮り、
//   emoji:  絵文字の行が OS の既定と違うか（違えば切り替えが効いた）
//   text:   数字と記号の行が OS の既定と同じか（同じなら本文の字のまま）
//   keycap: 1️⃣ #️⃣ *️⃣ がそれぞれ一つの絵の幅か
//   zwj:    ❤️‍🔥 👨‍👩‍👧 がそれぞれ一つの絵の幅か
// を results.json と results.md に書く。像は <出力の dir>/png/ に置く。
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { chromium, firefox, webkit } from 'playwright';
import { Builder, By } from 'selenium-webdriver';
import { PNG } from 'pngjs';
import pixelmatch from 'pixelmatch';

const FRONTEND = path.resolve(process.argv[2] ?? '../..');
const OUT = path.resolve(process.argv[3] ?? 'out');
const ONLY = process.argv[4]?.split(',');
fs.mkdirSync(path.join(OUT, 'png'), { recursive: true });

const NOTO_SANS =
  'https://fonts.googleapis.com/css2?family=Noto+Sans:wght@400;500;600;700&display=swap';
const NOTO_EMOJI = 'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji&display=swap';
const TWEMOJI_CSS = fs
  .readdirSync(path.join(FRONTEND, 'public/fonts/twemoji'))
  .find((name) => name.endsWith('.css'));

// 絞った写しから除く字: ASCII と Latin-1 と ™
const EXCLUDE = [
  [0x0, 0xff],
  [0x2122, 0x2122],
];

const BODY =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', 'Noto Sans', Arial";
const BODY_NO_SYSTEM = "'Segoe UI', Roboto, 'Helvetica Neue', 'Noto Sans', Arial";
const TAIL =
  "sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol', 'Noto Color Emoji'";

const FONTS = {
  os: { full: null, narrow: null, css: [] },
  noto: {
    full: "'Noto Color Emoji'",
    narrow: "'Koyori Noto Narrow'",
    css: ['/noto.css', '/noto-narrow.css'],
    cssZero: ['/noto.css', '/noto-narrow.css?zero=1'],
    direct: NOTO_EMOJI,
  },
  twemoji: {
    full: "'Koyori Twemoji'",
    narrow: "'Koyori Twemoji Narrow'",
    css: [`/fonts/twemoji/${TWEMOJI_CSS}`, '/twemoji-narrow.css'],
    cssZero: [`/fonts/twemoji/${TWEMOJI_CSS}`, '/twemoji-narrow.css?zero=1'],
    direct: `/fonts/twemoji/${TWEMOJI_CSS}`,
  },
};
const NARROW_JS = "'Koyori Emoji Narrow'";

// 字の並び。f は選んだ字（full と narrow）。os では字を入れない
const VARIANTS = {
  // 今の並び（本文の字の後ろ）
  now: (f) => [BODY, f.full, TAIL],
  // 見当の確かめ: -apple-system と BlinkMacSystemFont を抜く
  'no-system': (f) => [BODY_NO_SYSTEM, f.full, TAIL],
  // 二段: 絵文字の範囲に絞った写しを頭に、全ての範囲の物を本文の字の後ろに
  'two-stage': (f) => [f.narrow, BODY, f.full, TAIL],
  // 頭に全ての範囲の物を置く（数字が奪われる見込み）
  head: (f) => [f.full, BODY, TAIL],
  // 二段の写しを browser の中で作る（字の CSS の @font-face から FontFace で足す）
  'two-stage-js': (f) => [f.full && NARROW_JS, BODY, f.full, TAIL],
  // 二段の写しの行の高さを 0 に上書きする（Safari の一画素のずれを消せるか）
  'two-stage-zero': (f) => [f.narrow, BODY, f.full, TAIL],
};

function stack(variant, font) {
  return VARIANTS[variant](FONTS[font]).filter(Boolean).join(', ');
}

// ---- unicode-range の引き算 ----
function parseRange(text) {
  return text.split(',').map((part) => {
    const [a, b] = part.trim().replace(/^U\+/i, '').split('-');
    return [parseInt(a, 16), parseInt(b ?? a, 16)];
  });
}
function subtract(ranges) {
  let out = ranges;
  for (const [ea, eb] of EXCLUDE) {
    out = out.flatMap(([a, b]) => {
      if (b < ea || a > eb) return [[a, b]];
      const parts = [];
      if (a < ea) parts.push([a, ea - 1]);
      if (b > eb) parts.push([eb + 1, b]);
      return parts;
    });
  }
  return out;
}
function formatRange(ranges) {
  return ranges
    .map(([a, b]) => (a === b ? `U+${a.toString(16)}` : `U+${a.toString(16)}-${b.toString(16)}`))
    .join(', ');
}
// @font-face の名と unicode-range を替えた写しを作る（範囲が空になった物は落とす）
function narrowCss(css, family, zero = false) {
  return css.replace(/@font-face\s*\{([^}]*)\}/g, (_, body) => {
    const range = /unicode-range:\s*([^;]+);/.exec(body);
    const narrowed = subtract(range ? parseRange(range[1]) : [[0, 0x10ffff]]);
    if (narrowed.length === 0) return '';
    let next = body.replace(/font-family:\s*[^;]+;/, `font-family: ${family};`);
    next = range
      ? next.replace(/unicode-range:\s*[^;]+;/, `unicode-range: ${formatRange(narrowed)};`)
      : `${next}\n  unicode-range: ${formatRange(narrowed)};`;
    if (zero) next += '\n  ascent-override: 0%;\n  descent-override: 0%;\n  line-gap-override: 0%;';
    return `@font-face {${next}}`;
  });
}

async function googleCss(url, userAgent) {
  const res = await fetch(url, { headers: { 'User-Agent': userAgent || 'Mozilla/5.0' } });
  return res.text();
}

// browser の中で、字の CSS の @font-face から絵文字の範囲に絞った写しを FontFace で足す
const NARROW_SCRIPT = `
window.__narrow = { done: false, faces: 0, error: null };
const EXCLUDE = ${JSON.stringify(EXCLUDE)};
function parse(t) { return t.split(',').map((p) => { const [a, b] = p.trim().replace(/^U\\+/i, '').split('-'); return [parseInt(a, 16), parseInt(b ?? a, 16)]; }); }
function subtract(r) { for (const [ea, eb] of EXCLUDE) r = r.flatMap(([a, b]) => { if (b < ea || a > eb) return [[a, b]]; const o = []; if (a < ea) o.push([a, ea - 1]); if (b > eb) o.push([eb + 1, b]); return o; }); return r; }
function fmt(r) { return r.map(([a, b]) => a === b ? 'U+' + a.toString(16) : 'U+' + a.toString(16) + '-' + b.toString(16)).join(', '); }
function addNarrow(sheet) {
  for (const rule of sheet.cssRules) {
    if (!(rule instanceof CSSFontFaceRule)) continue;
    const range = subtract(parse(rule.style.getPropertyValue('unicode-range') || 'U+0-10FFFF'));
    if (!range.length) continue;
    document.fonts.add(new FontFace(${JSON.stringify(NARROW_JS.replace(/'/g, ''))}, rule.style.getPropertyValue('src'), { unicodeRange: fmt(range), display: 'swap' }));
    window.__narrow.faces++;
  }
}
const link = document.getElementById('emoji-css');
const run = () => { try { addNarrow(link.sheet); } catch (e) { window.__narrow.error = String(e); } window.__narrow.done = true; };
if (link) { if (link.sheet) run(); else link.addEventListener('load', run); } else window.__narrow.done = true;
`;

function page(variant, font) {
  const f = FONTS[font];
  const sources =
    variant === 'two-stage-js'
      ? f.direct
        ? [`<link id="emoji-css" rel="stylesheet" crossorigin="anonymous" href="${f.direct}">`]
        : []
      : (variant === 'two-stage-zero' ? (f.cssZero ?? f.css) : f.css).map(
          (href) => `<link rel="stylesheet" href="${href}">`,
        );
  const links = [`<link rel="stylesheet" href="${NOTO_SANS}">`, ...sources].join('\n');
  const script =
    variant === 'two-stage-js'
      ? `<script>${NARROW_SCRIPT}</script>`
      : '<script>window.__narrow = { done: true };</script>';
  return `<!doctype html><html><head><meta charset="utf-8">${links}
<style>
body { margin: 16px; font-size: 32px; line-height: 1.6; font-family: ${stack(variant, font)}; background: #fff; color: #222; }
p { margin: 0 0 8px; white-space: nowrap; }
.g { display: inline-block; }
</style></head><body>
<p id="emoji">😀 👍🏽 ❤️ ✅ 🎉 🇯🇵 🏴󠁧󠁢󠁳󠁣󠁴󠁿 ☺️</p>
<p id="zwj"><span class="g">❤️‍🔥</span> <span class="g">👨‍👩‍👧</span></p>
<p id="keycap"><span class="g">1️⃣</span> <span class="g">#️⃣</span> <span class="g">*️⃣</span></p>
<p id="text">0123456789 # * © ® ™</p>
<p><span id="ref" class="g">😀</span></p>
${script}
</body></html>`;
}

// ---- 見本の server ----
const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const ua = req.headers['user-agent'];
  try {
    if (url.pathname === '/page') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(page(url.searchParams.get('variant'), url.searchParams.get('font')));
    } else if (url.pathname === '/noto.css') {
      res.writeHead(200, { 'Content-Type': 'text/css' });
      res.end(await googleCss(NOTO_EMOJI, ua));
    } else if (url.pathname === '/noto-narrow.css') {
      res.writeHead(200, { 'Content-Type': 'text/css' });
      res.end(
        narrowCss(
          await googleCss(NOTO_EMOJI, ua),
          "'Koyori Noto Narrow'",
          url.searchParams.has('zero'),
        ),
      );
    } else if (url.pathname === '/twemoji-narrow.css') {
      const css = fs.readFileSync(path.join(FRONTEND, 'public/fonts/twemoji', TWEMOJI_CSS), 'utf8');
      res.writeHead(200, { 'Content-Type': 'text/css' });
      res.end(narrowCss(css, "'Koyori Twemoji Narrow'", url.searchParams.has('zero')));
    } else if (url.pathname.startsWith('/fonts/')) {
      const file = path.join(FRONTEND, 'public', url.pathname);
      res.writeHead(200, { 'Content-Type': file.endsWith('.css') ? 'text/css' : 'font/woff2' });
      fs.createReadStream(file).pipe(res);
    } else {
      res.writeHead(404);
      res.end();
    }
  } catch (error) {
    res.writeHead(500);
    res.end(String(error));
  }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const BASE = `http://127.0.0.1:${server.address().port}`;

// ---- 撮る ----
const MEASURE = `return (() => {
  const ref = document.getElementById('ref').getBoundingClientRect().width;
  const widths = (id) => [...document.querySelectorAll('#' + id + ' .g')].map((el) => el.getBoundingClientRect().width / ref);
  return { keycap: widths('keycap'), zwj: widths('zwj'), narrow: window.__narrow };
})()`;
const WAIT = `const done = arguments[arguments.length - 1];
const wait = () => (window.__narrow && window.__narrow.done ? document.fonts.ready.then(() => setTimeout(done, 1500)) : setTimeout(wait, 100));
wait();`;

const ROWS = ['emoji', 'zwj', 'keycap', 'text'];

async function shootPlaywright(type, name) {
  const browser = await type.launch();
  const context = await browser.newContext({
    viewport: { width: 900, height: 420 },
    deviceScaleFactor: 1,
  });
  const out = {};
  for (const variant of Object.keys(VARIANTS)) {
    for (const font of Object.keys(FONTS)) {
      const p = await context.newPage();
      await p.goto(`${BASE}/page?variant=${variant}&font=${font}`);
      await p.waitForFunction(() => window.__narrow?.done === true);
      await p.evaluate(() => document.fonts.ready);
      await p.waitForTimeout(1500);
      const shots = {};
      for (const row of ROWS) {
        const file = path.join(OUT, 'png', `${name}-${variant}-${font}-${row}.png`);
        await p.locator(`#${row}`).screenshot({ path: file });
        shots[row] = file;
      }
      out[`${variant}/${font}`] = { shots, widths: await p.evaluate(new Function(MEASURE)) };
      await p.close();
    }
  }
  await browser.close();
  return out;
}

async function shootSafari() {
  const driver = await new Builder().forBrowser('safari').build();
  const out = {};
  try {
    await driver.manage().window().setRect({ width: 900, height: 600 });
    for (const variant of Object.keys(VARIANTS)) {
      for (const font of Object.keys(FONTS)) {
        await driver.get(`${BASE}/page?variant=${variant}&font=${font}`);
        await driver.executeAsyncScript(WAIT);
        const shots = {};
        for (const row of ROWS) {
          const file = path.join(OUT, 'png', `safari-${variant}-${font}-${row}.png`);
          const data = await driver.findElement(By.id(row)).takeScreenshot();
          fs.writeFileSync(file, Buffer.from(data, 'base64'));
          shots[row] = file;
        }
        out[`${variant}/${font}`] = { shots, widths: await driver.executeScript(MEASURE) };
      }
    }
  } finally {
    await driver.quit();
  }
  return out;
}

// 二枚の像の違う画素の割合（大きさが違えば 1）
function diffRatio(a, b) {
  const x = PNG.sync.read(fs.readFileSync(a));
  const y = PNG.sync.read(fs.readFileSync(b));
  if (x.width !== y.width || x.height !== y.height) return 1;
  const n = pixelmatch(x.data, y.data, null, x.width, x.height, { threshold: 0.1 });
  return n / (x.width * x.height);
}

const BROWSERS = {
  chromium: () => shootPlaywright(chromium, 'chromium'),
  firefox: () => shootPlaywright(firefox, 'firefox'),
  webkit: () => shootPlaywright(webkit, 'webkit'),
  safari: () => shootSafari(),
};

const results = { errors: {} };
for (const [name, shoot] of Object.entries(BROWSERS)) {
  if (ONLY && !ONLY.includes(name)) continue;
  try {
    const shots = await shoot();
    results[name] = {};
    for (const variant of Object.keys(VARIANTS)) {
      const os = shots[`${variant}/os`];
      for (const font of ['noto', 'twemoji']) {
        const got = shots[`${variant}/${font}`];
        results[name][`${variant}/${font}`] = {
          emojiDiff: diffRatio(os.shots.emoji, got.shots.emoji),
          zwjDiff: diffRatio(os.shots.zwj, got.shots.zwj),
          textDiff: diffRatio(os.shots.text, got.shots.text),
          keycapWidths: got.widths.keycap,
          zwjWidths: got.widths.zwj,
          narrow: got.widths.narrow,
        };
      }
      results[name][`${variant}/os`] = { keycapWidths: os.widths.keycap, zwjWidths: os.widths.zwj };
    }
  } catch (error) {
    results.errors[name] = String(error?.stack ?? error).slice(0, 2000);
  }
}
server.close();

// ---- 判じ ----
// emoji: OS の既定と 5% 以上の画素が違えば「効いた」。text: 0.5% 未満なら「本文の字のまま」。
// 一つの絵: 幅が基準の絵文字（😀）の 1.3 倍以下
const single = (ws) => ws.every((w) => w <= 1.3);
const lines = [
  '| browser | 並び | 字 | 絵文字が変わった | 数字と記号は同じ | keycap が一つの絵 | ZWJ が一つの絵 |',
  '| --- | --- | --- | --- | --- | --- | --- |',
];
for (const [name, rows] of Object.entries(results)) {
  if (name === 'errors') continue;
  for (const [key, r] of Object.entries(rows)) {
    const [variant, font] = key.split('/');
    if (font === 'os') continue;
    r.verdict = {
      switched: r.emojiDiff >= 0.05,
      textKept: r.textDiff < 0.005,
      keycapSingle: single(r.keycapWidths),
      zwjSingle: single(r.zwjWidths),
    };
    const v = r.verdict;
    const mark = (ok, value) => `${ok ? 'yes' : 'NO'} (${value})`;
    lines.push(
      `| ${name} | ${variant} | ${font} | ${mark(v.switched, r.emojiDiff.toFixed(3))} | ${mark(v.textKept, r.textDiff.toFixed(4))} | ${mark(v.keycapSingle, r.keycapWidths.map((w) => w.toFixed(2)).join(' '))} | ${mark(v.zwjSingle, r.zwjWidths.map((w) => w.toFixed(2)).join(' '))} |`,
    );
  }
}
for (const [name, error] of Object.entries(results.errors))
  lines.push(`\n${name}: ${error.split('\n')[0]}`);
fs.writeFileSync(path.join(OUT, 'results.json'), JSON.stringify(results, null, 1));
fs.writeFileSync(path.join(OUT, 'results.md'), lines.join('\n') + '\n');
console.log(lines.join('\n'));
