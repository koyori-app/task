/**
 * 絵文字の字の選び肢と、それを画面に当てる道具。
 *
 * 選んだ字は、本文の字の並び（tailwind.css の --font-sans）の本文の字の後ろに
 * `var(--koyori-emoji-font,)` として入る。OS の既定ではこの変数を置かず、並びは今までと同じになる。
 * コードブロック（等幅の並び）には効かせない（訳は tailwind.css の注釈）。
 * 字の @font-face は emoji-fonts.css にある（scripts/fetch-noto-color-emoji.mjs が書く）。
 *
 * 選び肢を足す時は、EMOJI_FONT_CHOICES に一行足し、その字の @font-face を足す。
 */
export const EMOJI_FONT_CHOICES = [
  { value: 'os', label: 'OS の既定', family: null },
  { value: 'noto', label: 'Noto Color Emoji', family: 'Koyori Noto Color Emoji' },
] as const;

export type EmojiFont = (typeof EMOJI_FONT_CHOICES)[number]['value'];

export const DEFAULT_EMOJI_FONT: EmojiFont = 'os';

/** 字の並びの頭に入る CSS 変数。 */
export const EMOJI_FONT_VAR = '--koyori-emoji-font';

/**
 * 字の並びの頭に置く値。`var(--koyori-emoji-font,)` の位置にそのまま入るので、末尾に区切りの `,` を含む。
 * OS の既定、選び肢に無い値、字を持たない値なら null（変数を置かない）。
 */
export function emojiFontStackPrefix(value: unknown): string | null {
  const choice = EMOJI_FONT_CHOICES.find((entry) => entry.value === value);
  return choice?.family ? `'${choice.family}',` : null;
}

/** 選んだ字を root（html 要素）に当てる。OS の既定などでは変数を外す。 */
export function applyEmojiFont(root: HTMLElement, value: unknown) {
  const prefix = emojiFontStackPrefix(value);
  if (prefix) root.style.setProperty(EMOJI_FONT_VAR, prefix);
  else root.style.removeProperty(EMOJI_FONT_VAR);
}

/**
 * 描く前に head で走らせる script の本文。
 * 好みは localStorage にあり SSR では読めないので、mount の後に当てると最初の一瞬だけ OS の字で描かれ、
 * それから入れ替わる。これを避けるため、body を描く前に localStorage を読んで html に変数を置く。
 * storageKey は環境設定の store が残す key（pinia-plugin-persistedstate の既定で store の id）。
 */
export function emojiFontBootScript(storageKey: string): string {
  const prefixes: Record<string, string> = {};
  for (const choice of EMOJI_FONT_CHOICES) {
    const prefix = emojiFontStackPrefix(choice.value);
    if (prefix) prefixes[choice.value] = prefix;
  }
  return [
    '(function(){try{',
    `var m=${JSON.stringify(prefixes)};`,
    `var s=JSON.parse(localStorage.getItem(${JSON.stringify(storageKey)})||'null');`,
    'var v=s&&s.values&&s.values.emojiFont;',
    'if(typeof v==="string"&&Object.prototype.hasOwnProperty.call(m,v))',
    `document.documentElement.style.setProperty(${JSON.stringify(EMOJI_FONT_VAR)},m[v]);`,
    '}catch(e){}})();',
  ].join('');
}
