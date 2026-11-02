import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { runInThisContext } from 'node:vm';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_EMOJI_FONT,
  EMOJI_FONT_CHOICES,
  EMOJI_FONT_VAR,
  applyEmojiFont,
  emojiFontBootScript,
  emojiFontStackPrefix,
} from '@/lib/emoji-font';
import { PREFERENCE_TABLE } from '@/lib/preferences';

// CSS は vitest の変換を通すと中身が落ちるので、file をそのまま読む
const require = createRequire(resolve(__dirname, 'emoji-font.test.ts'));
const frontendRoot = resolve(__dirname, '../../..');
const readText = (path: string) => readFileSync(path, 'utf8');
const emojiFontsCss = readText(resolve(frontendRoot, 'src/assets/css/emoji-fonts.css'));
const tailwindCss = readText(resolve(frontendRoot, 'src/assets/css/tailwind.css'));
const tailwindThemeCss = readText(require.resolve('tailwindcss/theme.css'));

/** CSS の @font-face を family と src と unicode-range の組にして返す */
function fontFaces(css: string) {
  return [...css.matchAll(/@font-face\s*\{([^}]*)\}/g)].map(([, body]) => ({
    family: /font-family:\s*'([^']+)'/.exec(body!)?.[1],
    sources: [...body!.matchAll(/url\('([^']+)'\) format\('woff2'\) tech\(([^)]+)\)/g)].map(
      ([, url, tech]) => ({ url: url!, tech: tech! }),
    ),
    src: /src:\s*([^;]+);/.exec(body!)?.[1] ?? '',
    range: /unicode-range:\s*([^;]+);/.exec(body!)?.[1]?.trim(),
  }));
}

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

  it('OS の既定は字を指定しない（並びの頭に何も入れない）', () => {
    expect(emojiFontStackPrefix('os')).toBeNull();
  });

  it('Noto を選ぶと、並びの頭に Noto の字と区切りの , を入れる', () => {
    expect(emojiFontStackPrefix('noto')).toBe("'Koyori Noto Color Emoji',");
  });

  it.each(['twemoji', '', null, undefined, true, 1])(
    '選び肢に無い値 %j は字を指定しない',
    (value) => {
      expect(emojiFontStackPrefix(value)).toBeNull();
    },
  );

  it('applyEmojiFont は選んだ字を html の変数に置き、OS の既定に戻すと外す', () => {
    const root = document.createElement('html');

    applyEmojiFont(root, 'noto');
    expect(root.style.getPropertyValue(EMOJI_FONT_VAR)).toBe("'Koyori Noto Color Emoji',");

    applyEmojiFont(root, 'os');
    expect(root.style.getPropertyValue(EMOJI_FONT_VAR)).toBe('');
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

describe('絵文字の字の @font-face（emoji-fonts.css）', () => {
  const faces = fontFaces(emojiFontsCss);
  const family = EMOJI_FONT_CHOICES.find((choice) => choice.value === 'noto')?.family;

  it('字の名は選び肢の family と揃う', () => {
    expect(faces.length).toBeGreaterThan(0);
    for (const face of faces) expect(face.family).toBe(family);
  });

  it('src は COLRv1 を先に、OpenType SVG を後に並べ、tech() で browser に選ばせる', () => {
    for (const face of faces) {
      expect(face.sources.map((source) => source.tech)).toEqual(['color-COLRv1', 'color-SVG']);
      expect(face.sources[0]!.url).toMatch(/^\/fonts\/noto-color-emoji\/colrv1\/\d+\.woff2$/);
      expect(face.sources[1]!.url).toMatch(/^\/fonts\/noto-color-emoji\/svg\/\d+\.woff2$/);
    }
  });

  it('字の file はこの app の public に在り、外の URL に繋がない', () => {
    for (const face of faces) {
      expect(face.src).not.toMatch(/https?:/);
      for (const source of face.sources) {
        expect(existsSync(resolve(frontendRoot, 'public', source.url.slice(1))), source.url).toBe(
          true,
        );
      }
    }
  });

  it('許諾（OFL）を字と同じ所に置く', () => {
    const license = readText(resolve(frontendRoot, 'public/fonts/noto-color-emoji/OFL.txt'));
    expect(license).toContain('SIL Open Font License');
  });

  it('どの切れも unicode-range を持つ（選ぶ字が要る切れだけを読む）', () => {
    for (const face of faces) expect(face.range).toMatch(/^U\+[0-9a-f]/i);
  });
});

describe('描く前に当てる script（emojiFontBootScript）', () => {
  afterEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty(EMOJI_FONT_VAR);
    vi.restoreAllMocks();
  });

  function run() {
    // head に置く文字列と同じ物を、そのまま走らせる
    runInThisContext(emojiFontBootScript('preferences'));
    return document.documentElement.style.getPropertyValue(EMOJI_FONT_VAR);
  }

  it('Noto が残っていれば、html に字を置く', () => {
    localStorage.setItem('preferences', JSON.stringify({ values: { emojiFont: 'noto' } }));
    expect(run()).toBe("'Koyori Noto Color Emoji',");
  });

  it.each([
    ['OS の既定', JSON.stringify({ values: { emojiFont: 'os' } })],
    ['選び肢に無い値', JSON.stringify({ values: { emojiFont: 'twemoji' } })],
    ['prototype の名前', JSON.stringify({ values: { emojiFont: 'toString' } })],
    ['項目の無い古い値', JSON.stringify({ values: { showExperiments: true } })],
    ['壊れた JSON', '{'],
  ])('%s では何も置かない', (_, stored) => {
    localStorage.setItem('preferences', stored);
    expect(run()).toBe('');
  });

  it('何も残っていなければ何も置かない', () => {
    expect(run()).toBe('');
  });

  it('localStorage が読めなくても投げない', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(run()).toBe('');
  });

  it('script の中で </script> が閉じない', () => {
    expect(emojiFontBootScript('preferences')).not.toMatch(/<\/script/i);
  });
});
