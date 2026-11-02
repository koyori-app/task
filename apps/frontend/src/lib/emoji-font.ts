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
 * 選んだ字を画面に当てる。html に変数を置き、字の CSS を head に読み込む。
 * OS の既定などでは変数と CSS の両方を外す（既定の並びの末尾の名が Google の字を指さないように）。
 */
export function applyEmojiFont(doc: Document, value: unknown) {
  const prefix = emojiFontStackPrefix(value);
  const stylesheet = emojiFontStylesheet(value);
  const current = doc.getElementById(EMOJI_FONT_LINK_ID);
  if (prefix && stylesheet) {
    doc.documentElement.style.setProperty(EMOJI_FONT_VAR, prefix);
    if (current?.getAttribute('href') === stylesheet) return;
    current?.remove();
    const link = doc.createElement('link');
    link.id = EMOJI_FONT_LINK_ID;
    link.rel = 'stylesheet';
    link.href = stylesheet;
    doc.head.appendChild(link);
  } else {
    doc.documentElement.style.removeProperty(EMOJI_FONT_VAR);
    current?.remove();
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
    'var l=document.createElement("link");',
    `l.id=${JSON.stringify(EMOJI_FONT_LINK_ID)};l.rel="stylesheet";l.href=m[v][1];`,
    'document.head.appendChild(l);',
    '}catch(e){}})();',
  ].join('');
}
