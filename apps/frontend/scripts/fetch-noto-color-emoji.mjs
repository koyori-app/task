// 絵文字の字（Noto Color Emoji）を Google Fonts から取り、この app から配る形に置く（TASK-385）。
// build では走らせない。字を取り直す時（版を上げる時）にだけ、手で走らせる。
//
//   node scripts/fetch-noto-color-emoji.mjs
//
// 走らせた後は、git diff で字の版と unicode-range の変わり方を見てから commit する。
//
// Google Fonts は browser によって違う形の字を返す。
//   - Chromium と Firefox には COLRv1（表 COLR と CPAL）
//   - Safari には OpenType SVG（表 SVG）
// Chromium は OpenType SVG を色で描かず、空の控えの形を描いて絵文字が消える。
// Safari は COLRv1 を描かない。そこで両方を置き、@font-face の src の tech() で browser に選ばせる。
//
// 置く物:
//   public/fonts/noto-color-emoji/colrv1/<番号>.woff2
//   public/fonts/noto-color-emoji/svg/<番号>.woff2
//   public/fonts/noto-color-emoji/OFL.txt（許諾。SIL Open Font License 1.1）
//   src/assets/css/emoji-fonts.css（@font-face。unicode-range は Google Fonts の切り分けのまま）
import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const fontDir = join(root, 'public/fonts/noto-color-emoji');
const cssPath = join(root, 'src/assets/css/emoji-fonts.css');
const FAMILY = 'Koyori Noto Color Emoji';
const CSS_URL = 'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji&display=swap';
const LICENSE_URL =
  'https://raw.githubusercontent.com/google/fonts/main/ofl/notocoloremoji/OFL.txt';

// Google Fonts は User-Agent で返す形を変える。browser の名だけを送る
const KINDS = {
  colrv1: {
    tech: 'color-COLRv1',
    userAgent:
      'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36',
  },
  svg: {
    tech: 'color-SVG',
    userAgent:
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15',
  },
};

async function text(url, userAgent) {
  const res = await fetch(url, { headers: userAgent ? { 'User-Agent': userAgent } : {} });
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return res.text();
}

/** css2 の応答から、切れごとの url と unicode-range を順に取り出す */
function parseFaces(css) {
  return [...css.matchAll(/src: url\(([^)]+)\) format\('woff2'\);\s*unicode-range: ([^;]+);/g)].map(
    ([, url, range]) => ({ url, range: range.trim() }),
  );
}

const faces = {};
for (const [kind, { userAgent }] of Object.entries(KINDS)) {
  faces[kind] = parseFaces(await text(CSS_URL, userAgent));
  if (faces[kind].length === 0) throw new Error(`${kind}: @font-face が無い`);
}
const count = faces.colrv1.length;
if (faces.svg.length !== count) throw new Error('COLRv1 と SVG で切れの数が違う');
faces.colrv1.forEach((face, i) => {
  if (face.range !== faces.svg[i].range) throw new Error(`切れ ${i} の unicode-range が違う`);
});
const version = faces.colrv1[0].url.match(/\/notocoloremoji\/(v\d+)\//)?.[1] ?? '不明';

rmSync(fontDir, { recursive: true, force: true });
for (const kind of Object.keys(KINDS)) {
  mkdirSync(join(fontDir, kind), { recursive: true });
  for (const [i, face] of faces[kind].entries()) {
    const res = await fetch(face.url, { headers: { 'User-Agent': KINDS[kind].userAgent } });
    if (!res.ok) throw new Error(`${face.url}: ${res.status}`);
    writeFileSync(join(fontDir, kind, `${i}.woff2`), Buffer.from(await res.arrayBuffer()));
  }
}
writeFileSync(join(fontDir, 'OFL.txt'), await text(LICENSE_URL));

const blocks = faces.colrv1.map((face, i) => {
  const src = Object.entries(KINDS)
    .map(
      ([kind, { tech }]) =>
        `url('/fonts/noto-color-emoji/${kind}/${i}.woff2') format('woff2') tech(${tech})`,
    )
    .join(',\n    ');
  return `@font-face {
  font-family: '${FAMILY}';
  font-style: normal;
  font-display: swap;
  font-weight: 400;
  src:
    ${src};
  unicode-range: ${face.range};
}`;
});

writeFileSync(
  cssPath,
  `/*
 * 絵文字の字（Noto Color Emoji ${version}）の @font-face。scripts/fetch-noto-color-emoji.mjs が書く。手で直さない。
 *
 * 字の file は Google Fonts から取り、public/fonts/noto-color-emoji/ に置いてこの app から配る
 * （外の CDN には繋がない）。許諾は同じ所の OFL.txt（SIL Open Font License 1.1）。
 * src は二つの形を並べ、tech() で browser に選ばせる。Chromium と Firefox は COLRv1、
 * Safari は OpenType SVG を描く（Chromium は OpenType SVG を描かず、絵文字が消える）。
 * unicode-range は Google Fonts の切り分けのままで、選ぶ字が要る切れだけが読まれる。
 *
 * この字は数字や # なども持つが、字の並び（tailwind.css）では本文の字の後ろに置くので、
 * 本文の字が持つ字はそちらで描かれる。public に置くのは、Vite の asset にすると vike が
 * どの page でも preload し、選んでいない人まで字を読むことになるため。
 */

${blocks.join('\n\n')}
`,
);

// 書いた CSS を repo の formatter に通す（lint-staged と同じ形にそろえる）
execFileSync('pnpm', ['exec', 'vp', 'fmt', cssPath], { cwd: root, stdio: 'inherit' });

console.log(`Noto Color Emoji ${version}: ${count} 切れ x ${Object.keys(KINDS).length} 形を置いた`);
