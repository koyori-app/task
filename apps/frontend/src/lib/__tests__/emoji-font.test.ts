import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { runInThisContext } from 'node:vm';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_EMOJI_FONT,
  EMOJI_FONT_LINK_ID,
  EMOJI_FONT_VAR,
  applyEmojiFont,
  emojiFontBootScript,
  emojiFontStackPrefix,
  emojiFontStylesheet,
} from '@/lib/emoji-font';
import { PREFERENCE_TABLE } from '@/lib/preferences';

// 絵文字の字の CSS（外の URL）の link を差す。happy-dom は外の CSS を読まず、読まなかったことを
// error として log に出すので、読まなかった CSS を読めた物として扱わせる
(
  window as unknown as { happyDOM: { settings: { handleDisabledFileLoadingAsSuccess: boolean } } }
).happyDOM.settings.handleDisabledFileLoadingAsSuccess = true;

// CSS は vitest の変換を通すと中身が落ちるので、file をそのまま読む
const require = createRequire(resolve(__dirname, 'emoji-font.test.ts'));
const frontendRoot = resolve(__dirname, '../../..');
const readText = (path: string) => readFileSync(path, 'utf8');
const tailwindCss = readText(resolve(frontendRoot, 'src/assets/css/tailwind.css'));
const tailwindThemeCss = readText(require.resolve('tailwindcss/theme.css'));

/** @theme の中の custom property の値（改行と連続する空白は一つにまとめる） */
function themeValue(css: string, name: string): string | undefined {
  const match = new RegExp(`${name}:\\s*([^;]+);`).exec(css);
  return match?.[1]!.replace(/\s+/g, ' ').trim();
}

describe('絵文字の字の選び肢', () => {
  it('既定は OS の既定で、環境設定の表の既定もそれに揃う', () => {
    expect(DEFAULT_EMOJI_FONT).toBe('os');
    expect(PREFERENCE_TABLE.emojiFont.default).toBe('os');
    expect(PREFERENCE_TABLE.emojiFont.choices.map((choice) => choice.value)).toEqual([
      'os',
      'noto',
    ]);
  });

  it('OS の既定は字も字の CSS も指定しない', () => {
    expect(emojiFontStackPrefix('os')).toBeNull();
    expect(emojiFontStylesheet('os')).toBeNull();
  });

  it('Noto は Google が名乗る名で並びに入り、字の CSS は Google Fonts から引く', () => {
    expect(emojiFontStackPrefix('noto')).toBe("'Noto Color Emoji',");
    expect(emojiFontStylesheet('noto')).toBe(
      'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji&display=swap',
    );
  });

  it.each(['twemoji', '', null, undefined, true, 1, 'toString'])(
    '選び肢に無い値 %j は字も字の CSS も指定しない',
    (value) => {
      expect(emojiFontStackPrefix(value)).toBeNull();
      expect(emojiFontStylesheet(value)).toBeNull();
    },
  );

  it('applyEmojiFont は html に変数を置いて字の CSS を一つだけ読み込み、OS の既定に戻すと両方を外す', () => {
    const doc = document.implementation.createHTMLDocument('');
    const links = () => doc.head.querySelectorAll(`link#${EMOJI_FONT_LINK_ID}`);

    applyEmojiFont(doc, 'noto');
    applyEmojiFont(doc, 'noto');
    expect(doc.documentElement.style.getPropertyValue(EMOJI_FONT_VAR)).toBe("'Noto Color Emoji',");
    expect(links()).toHaveLength(1);
    expect(links()[0]!.getAttribute('href')).toBe(emojiFontStylesheet('noto'));
    expect(links()[0]!.getAttribute('rel')).toBe('stylesheet');

    applyEmojiFont(doc, 'os');
    expect(doc.documentElement.style.getPropertyValue(EMOJI_FONT_VAR)).toBe('');
    expect(links()).toHaveLength(0);
  });
});

describe('字の並び（tailwind.css）', () => {
  const SLOT = `var(${EMOJI_FONT_VAR},) `;

  it('--font-sans は口を除けば Tailwind の既定と同じ（OS の既定で見た目が変わらない）', () => {
    expect(themeValue(tailwindCss, '--font-sans')?.replace(SLOT, '')).toBe(
      themeValue(tailwindThemeCss, '--font-sans'),
    );
  });

  it('等幅の並び（--font-mono）は定め直さず、絵文字の字の口も置かない（コードの数字を守る）', () => {
    expect(themeValue(tailwindCss, '--font-mono')).toBeUndefined();
    // 口は --font-sans の一つだけ
    expect(tailwindCss.match(/var\(--koyori-emoji-font,\)/g)).toHaveLength(1);
  });

  it.each([['--font-sans', 'sans-serif']])(
    '%s の口は本文の字の後ろ、総称 %s の直前にある（数字や # は本文の字で描かれる）',
    (name, generic) => {
      const value = themeValue(tailwindCss, name)!;
      expect(value).toContain(`${SLOT}${generic}`);
      // 口より前に本文の字が並んでいる（頭に置くと、数字まで絵文字の字で描かれる）
      const before = value.slice(0, value.indexOf(SLOT));
      expect(before.split(',').filter((font) => font.trim()).length).toBeGreaterThan(3);
      // OS の絵文字の字（Apple Color Emoji など）より前にある（選んだ字が OS の字に勝つ）
      const osEmoji = value.indexOf("'Apple Color Emoji'");
      if (osEmoji >= 0) expect(value.indexOf(SLOT)).toBeLessThan(osEmoji);
    },
  );
});

describe('既定の CSS（tailwind.css）', () => {
  const imports = [...tailwindCss.matchAll(/@import url\('([^']+)'\)/g)].map(([, url]) => url!);

  it('Google Fonts の取り込みは一本だけで、本文の Noto Sans だけを引く（字の CSS を取りに行く回数を増やさない）', () => {
    expect(imports).toEqual([
      'https://fonts.googleapis.com/css2?family=Noto+Sans:wght@400;500;600;700&display=swap',
    ]);
  });

  it('既定の CSS は絵文字の字を引かない（末尾の Noto Color Emoji が Google の字を指さない）', () => {
    expect(tailwindCss).not.toMatch(/family=Noto\+Color\+Emoji/);
    expect(tailwindCss).not.toMatch(/@font-face\s*\{/);
  });
});

describe('描く前に当てる script（emojiFontBootScript）', () => {
  afterEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty(EMOJI_FONT_VAR);
    document.getElementById(EMOJI_FONT_LINK_ID)?.remove();
    vi.restoreAllMocks();
  });

  /** head に置く文字列と同じ物をそのまま走らせ、置いた変数と読み込んだ CSS を返す */
  function run() {
    runInThisContext(emojiFontBootScript('preferences'));
    const link = document.getElementById(EMOJI_FONT_LINK_ID);
    return {
      variable: document.documentElement.style.getPropertyValue(EMOJI_FONT_VAR),
      stylesheet: link?.getAttribute('rel') === 'stylesheet' ? link.getAttribute('href') : null,
    };
  }

  it('Noto が残っていれば、html に字を置き、字の CSS を読み込む', () => {
    localStorage.setItem('preferences', JSON.stringify({ values: { emojiFont: 'noto' } }));
    expect(run()).toEqual({
      variable: "'Noto Color Emoji',",
      stylesheet: emojiFontStylesheet('noto'),
    });
  });

  it.each([
    ['OS の既定', JSON.stringify({ values: { emojiFont: 'os' } })],
    ['選び肢に無い値', JSON.stringify({ values: { emojiFont: 'twemoji' } })],
    ['prototype の名前', JSON.stringify({ values: { emojiFont: 'toString' } })],
    ['項目の無い古い値', JSON.stringify({ values: { showExperiments: true } })],
    ['壊れた JSON', '{'],
  ])('%s では何も置かず、字の CSS も読み込まない', (_, stored) => {
    localStorage.setItem('preferences', stored);
    expect(run()).toEqual({ variable: '', stylesheet: null });
  });

  it('何も残っていなければ何も置かない', () => {
    expect(run()).toEqual({ variable: '', stylesheet: null });
  });

  it('localStorage が読めなくても投げない', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(run()).toEqual({ variable: '', stylesheet: null });
  });

  it('script の中で </script> が閉じない', () => {
    expect(emojiFontBootScript('preferences')).not.toMatch(/<\/script/i);
  });
});
