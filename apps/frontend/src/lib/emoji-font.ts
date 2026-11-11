/**
 * 絵文字の字の選び肢と、それを画面に当てる道具。
 *
 * 選んだ字は、本文の字の並び（tailwind.css の --font-sans）の本文の字の後ろに
 * `var(--koyori-emoji-font,)` として入る。OS の既定ではこの変数を置かず、並びは今までと同じになる。
 * コードブロック（等幅の並び）には効かせない（訳は tailwind.css の注釈）。
 *
 * 字の @font-face は選び肢ごとの CSS（stylesheet）にあり、選んだ人の画面にだけ読み込む。
 * Noto は Google Fonts の CSS で、Google Fonts は browser ごとに描ける形の字を返す（Chromium と Firefox に
 * COLRv1、Safari に OpenType SVG）。Twemoji はこの app が配る COLRv0 の字（scripts/fonts/build-twemoji.sh）。
 * 既定の CSS に入れないのは、Tailwind の既定の並びの末尾の 'Noto Color Emoji' が Google の字を指してしまい、
 * OS の既定の人まで絵文字の字を取りに行き、OS の字に無い絵文字の見た目も変わるため。
 *
 * 選び肢を足す時は、EMOJI_FONT_CHOICES に一行足す。
 */
export const EMOJI_FONT_CHOICES = [
  { value: 'os', label: 'OS の既定', family: null, stylesheet: null },
  {
    value: 'noto',
    label: 'Noto Color Emoji',
    family: 'Noto Color Emoji',
    stylesheet: 'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji&display=swap',
  },
  // 字と CSS は scripts/fonts/build-twemoji.sh で組み、この app から配る（public/fonts/twemoji/）
  {
    value: 'twemoji',
    label: 'Twemoji',
    family: 'Koyori Twemoji',
    stylesheet: '/fonts/twemoji/twemoji-17.0.3.css',
  },
] as const;

export type EmojiFont = (typeof EMOJI_FONT_CHOICES)[number]['value'];

export const DEFAULT_EMOJI_FONT: EmojiFont = 'os';

/** 字の並びに入る CSS 変数。 */
export const EMOJI_FONT_VAR = '--koyori-emoji-font';

/** 選んだ字の CSS を読み込む link 要素の id。 */
export const EMOJI_FONT_LINK_ID = 'koyori-emoji-font';

/**
 * 絵文字の範囲に絞った写しの字の名と、それを字の並びの頭に置く CSS 変数（TASK-410）。
 * macOS の Safari と WebKit では、並びの頭の -apple-system（San Francisco）が己の中に控えの字の
 * 連なりを持ち、そこに Apple Color Emoji が入っている。絵文字は並びの後ろに置いた選んだ字まで
 * 辿り着かず、Apple の絵で描かれる。そこで選んだ字の写しを、ASCII と Latin-1 と ™ を除いた範囲に
 * 絞って並びの頭にも置く。数字や # などは写しに無いので本文の字で描かれ、絵文字だけが写しで描かれる。
 * keycap（1️⃣）のように数字で始まる並びは、写しでは描けず、本文の字の後ろの全ての範囲の字で描かれる。
 */
export const EMOJI_NARROW_FAMILY = 'Koyori Emoji Narrow';
export const EMOJI_NARROW_VAR = '--koyori-emoji-narrow';

function findChoice(value: unknown) {
  return EMOJI_FONT_CHOICES.find((entry) => entry.value === value);
}

/**
 * 字の並びに置く値。`var(--koyori-emoji-font,)` の位置にそのまま入るので、末尾に区切りの `,` を含む。
 * OS の既定、選び肢に無い値、字を持たない値なら null（変数を置かない）。
 */
export function emojiFontStackPrefix(value: unknown): string | null {
  const choice = findChoice(value);
  return choice?.family ? `'${choice.family}',` : null;
}

/** 選んだ字の CSS の URL。OS の既定や選び肢に無い値なら null。 */
export function emojiFontStylesheet(value: unknown): string | null {
  return findChoice(value)?.stylesheet ?? null;
}

/**
 * 読み込んだ字の CSS の @font-face から、絵文字の範囲に絞った写しを FontFace で足し、足した数を返す。
 * 写しの名は family。前に足した同じ名の写しは先に外す。
 * Noto の CSS は Google Fonts が browser ごとに中身を変えて返すので、写しを前もって置けない。
 * browser が読んだ CSS の規則から作る（他の origin の CSS は link に crossorigin が要る）。
 *
 * 描く前に走る head の script にも、この関数の源（toString）をそのまま埋めるので、外の名
 * （import や module の定数）に頼らず、この中だけで閉じること。
 */
export function installNarrowEmojiFaces(
  doc: Document,
  sheet: CSSStyleSheet,
  family: string,
): number {
  // FontFace を持たない所（古い browser など）では何もしない
  if (!doc.fonts || typeof FontFace === 'undefined') return 0;
  // 写しから除く字: ASCII と Latin-1 と ™
  const exclude = [
    [0x0, 0xff],
    [0x2122, 0x2122],
  ];
  for (const face of Array.from(doc.fonts)) {
    if (face.family.replace(/["']/g, '') === family) doc.fonts.delete(face);
  }
  let added = 0;
  for (const rule of Array.from(sheet.cssRules)) {
    if (!rule.cssText.startsWith('@font-face')) continue;
    const style = (rule as CSSFontFaceRule).style;
    const src = style.getPropertyValue('src');
    if (!src) continue;
    let ranges = (style.getPropertyValue('unicode-range') || 'U+0-10FFFF')
      .split(',')
      .map((part) => {
        const [from, to] = part.trim().replace(/^U\+/i, '').split('-');
        return [parseInt(from!, 16), parseInt(to ?? from!, 16)];
      });
    for (const [lo, hi] of exclude) {
      ranges = ranges.flatMap(([a, b]) => {
        if (b! < lo! || a! > hi!) return [[a!, b!]];
        const parts: number[][] = [];
        if (a! < lo!) parts.push([a!, lo! - 1]);
        if (b! > hi!) parts.push([hi! + 1, b!]);
        return parts;
      });
    }
    if (ranges.length === 0) continue;
    const unicodeRange = ranges
      .map(([a, b]) =>
        a === b ? `U+${a!.toString(16)}` : `U+${a!.toString(16)}-${b!.toString(16)}`,
      )
      .join(', ');
    doc.fonts.add(new FontFace(family, src, { unicodeRange, display: 'swap' }));
    added += 1;
  }
  return added;
}

/** 絵文字の範囲に絞った写しを全て外す。 */
function removeNarrowEmojiFaces(doc: Document) {
  if (!doc.fonts) return;
  for (const face of Array.from(doc.fonts)) {
    if (face.family.replace(/["']/g, '') === EMOJI_NARROW_FAMILY) doc.fonts.delete(face);
  }
}

/**
 * 選んだ字を画面に当てる。html に変数を置き、字の CSS を head に読み込み、読めたら絵文字の範囲に
 * 絞った写しを足す。OS の既定などでは変数と CSS と写しを全て外す（既定の並びの末尾の名が Google の
 * 字を指さないように）。
 */
export function applyEmojiFont(doc: Document, value: unknown) {
  const prefix = emojiFontStackPrefix(value);
  const stylesheet = emojiFontStylesheet(value);
  const current = doc.getElementById(EMOJI_FONT_LINK_ID);
  if (prefix && stylesheet) {
    doc.documentElement.style.setProperty(EMOJI_FONT_VAR, prefix);
    doc.documentElement.style.setProperty(EMOJI_NARROW_VAR, `'${EMOJI_NARROW_FAMILY}',`);
    if (current?.getAttribute('href') === stylesheet) return;
    current?.remove();
    removeNarrowEmojiFaces(doc);
    const link = doc.createElement('link');
    link.id = EMOJI_FONT_LINK_ID;
    link.rel = 'stylesheet';
    link.crossOrigin = 'anonymous';
    link.href = stylesheet;
    link.addEventListener('load', () => {
      if (link.sheet) installNarrowEmojiFaces(doc, link.sheet, EMOJI_NARROW_FAMILY);
    });
    doc.head.appendChild(link);
  } else {
    doc.documentElement.style.removeProperty(EMOJI_FONT_VAR);
    doc.documentElement.style.removeProperty(EMOJI_NARROW_VAR);
    current?.remove();
    removeNarrowEmojiFaces(doc);
  }
}

/**
 * 描く前に head で走らせる script の本文。
 * 好みは localStorage にあり SSR では読めないので、mount の後に当てると最初の一瞬だけ OS の字で描かれ、
 * それから入れ替わる。これを避けるため、body を描く前に localStorage を読み、applyEmojiFont と
 * 同じく html に変数を置いて字の CSS を読み込む。
 * storageKey は環境設定の store が残す key（pinia-plugin-persistedstate の既定で store の id）。
 */
export function emojiFontBootScript(storageKey: string): string {
  const choices: Record<string, [string, string]> = {};
  for (const choice of EMOJI_FONT_CHOICES) {
    const prefix = emojiFontStackPrefix(choice.value);
    const stylesheet = emojiFontStylesheet(choice.value);
    if (prefix && stylesheet) choices[choice.value] = [prefix, stylesheet];
  }
  return [
    '(function(){try{',
    `var m=${JSON.stringify(choices)};`,
    `var s=JSON.parse(localStorage.getItem(${JSON.stringify(storageKey)})||'null');`,
    'var v=s&&s.values&&s.values.emojiFont;',
    'if(typeof v!=="string"||!Object.prototype.hasOwnProperty.call(m,v))return;',
    `document.documentElement.style.setProperty(${JSON.stringify(EMOJI_FONT_VAR)},m[v][0]);`,
    `document.documentElement.style.setProperty(${JSON.stringify(EMOJI_NARROW_VAR)},${JSON.stringify(`'${EMOJI_NARROW_FAMILY}',`)});`,
    'var l=document.createElement("link");',
    `l.id=${JSON.stringify(EMOJI_FONT_LINK_ID)};l.rel="stylesheet";l.crossOrigin="anonymous";l.href=m[v][1];`,
    `var install=${installNarrowEmojiFaces.toString()};`,
    `l.addEventListener("load",function(){try{if(l.sheet)install(document,l.sheet,${JSON.stringify(EMOJI_NARROW_FAMILY)});}catch(e){}});`,
    'document.head.appendChild(l);',
    '}catch(e){}})();',
  ].join('');
}
