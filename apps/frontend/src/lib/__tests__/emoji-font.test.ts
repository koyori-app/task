import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { runInThisContext } from 'node:vm';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_EMOJI_FONT,
  EMOJI_FONT_LINK_ID,
  EMOJI_FONT_VAR,
  EMOJI_NARROW_FAMILY,
  EMOJI_NARROW_VAR,
  applyEmojiFont,
  installNarrowEmojiFaces,
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
      'twemoji',
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

  it.each(['fluent', '', null, undefined, true, 1, 'toString'])(
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
  const NARROW_SLOT = `var(${EMOJI_NARROW_VAR},) `;

  it('--font-sans は二つの口を除けば Tailwind の既定と同じ（OS の既定で見た目が変わらない）', () => {
    expect(themeValue(tailwindCss, '--font-sans')?.replace(NARROW_SLOT, '').replace(SLOT, '')).toBe(
      themeValue(tailwindThemeCss, '--font-sans'),
    );
  });

  it('絵文字の範囲に絞った写しの口は、並びの頭、-apple-system の前にある（macOS の Safari で効かせる）', () => {
    // -apple-system は己の中の Apple Color Emoji で絵文字を描くので、その前に置かないと届かない
    expect(themeValue(tailwindCss, '--font-sans')).toMatch(
      new RegExp(`^var\\(${EMOJI_NARROW_VAR},\\) -apple-system,`),
    );
    expect(tailwindCss.match(/var\(--koyori-emoji-narrow,\)/g)).toHaveLength(1);
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
    ['選び肢に無い値', JSON.stringify({ values: { emojiFont: 'fluent' } })],
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

describe('Twemoji の字（public/fonts/twemoji/）', () => {
  const stylesheet = emojiFontStylesheet('twemoji')!;
  const css = readText(resolve(frontendRoot, 'public', stylesheet.slice(1)));
  const range = /unicode-range:\s*([^;]+);/.exec(css)?.[1] ?? '';
  const points = new Set<number>();
  for (const part of range.split(',')) {
    const [from, to] = part.trim().replace(/^U\+/i, '').split('-');
    const start = Number.parseInt(from!, 16);
    const end = to === undefined ? start : Number.parseInt(to, 16);
    for (let point = start; point <= end; point += 1) points.add(point);
  }

  it('Twemoji は選び肢の一行で、独自の名で並びに入り、この app が配る CSS を指す', () => {
    expect(emojiFontStackPrefix('twemoji')).toBe("'Koyori Twemoji',");
    expect(stylesheet).toMatch(/^\/fonts\/twemoji\/twemoji-[\d.]+\.css$/);
    expect(existsSync(resolve(frontendRoot, 'public', stylesheet.slice(1))), stylesheet).toBe(true);
  });

  it('名は Tailwind の既定の並びの名とぶつからない', () => {
    const tailwindFonts = themeValue(tailwindThemeCss, '--font-sans')!
      .split(',')
      .map((font) => font.trim().replace(/^['"]|['"]$/g, ''));
    expect(tailwindFonts).not.toContain('Koyori Twemoji');
    expect(css).toContain("font-family: 'Koyori Twemoji';");
  });

  it('@font-face は同じ所の WOFF2 を指し、外の URL に繋がない', () => {
    const src = /src:\s*url\('([^']+)'\) format\('woff2'\);/.exec(css)?.[1] ?? '';
    expect(src).toMatch(/^\/fonts\/twemoji\/twemoji-[\d.]+\.woff2$/);
    expect(existsSync(resolve(frontendRoot, 'public', src.slice(1))), src).toBe(true);
    expect(css).not.toMatch(/https?:\/\/(?!github\.com|creativecommons\.org)/);
  });

  it('@font-face の注釈に名（Twitter, Inc. and other contributors、jdecked、CC-BY 4.0）を置く', () => {
    expect(css).toContain('Twitter, Inc. and other contributors');
    expect(css).toContain('jdecked/twemoji');
    expect(css).toContain('CC-BY 4.0');
  });

  it('unicode-range は keycap と ZWJ と tag の旗に要る字を持ち、数字を削らない', () => {
    const needed = [
      ...Array.from({ length: 10 }, (_, i) => 0x30 + i),
      0x23,
      0x2a,
      0xfe0f,
      0x20e3,
      0x200d,
      0x1f3f4,
      0xe0067,
      0xe007f,
    ];
    for (const point of needed) {
      expect(points.has(point), `U+${point.toString(16)}`).toBe(true);
    }
  });
});

describe('Twemoji の組み直しの手順（scripts/fonts/build-twemoji.sh）', () => {
  const script = readText(resolve(frontendRoot, 'scripts/fonts/build-twemoji.sh'));
  const copies = readText(resolve(frontendRoot, 'scripts/fonts/twemoji-vs16-copies.txt'))
    .split('\n')
    .filter((line) => line && !line.startsWith('#'))
    .map((line) => line.split(' <- ')[0]);

  it('元の版と Emoji の版と道具の版を固定する', () => {
    expect(script).toMatch(/^TAG=v17\.0\.3$/m);
    expect(script).toContain('https://www.unicode.org/Public/17.0.0/emoji/emoji-test.txt');
    expect(script).toMatch(/^EMOJI_TEST_SHA256=[0-9a-f]{64}$/m);
    expect(script).toContain('twemoji-requirements.txt');
    expect(script).toMatch(/^export SOURCE_DATE_EPOCH=\d+$/m);
  });

  it('emoji-test.txt から VS16 の有る無しを埋める写しを足す手順を持つ', () => {
    for (const status of ['fully-qualified', 'minimally-qualified', 'unqualified']) {
      expect(script).toContain(`'${status}'`);
    }
    expect(script).toContain('shutil.copyfile');
  });

  it('十二の keycap（0 から 9、#、*）の写しを足している', () => {
    const keycaps = ['0023', '002A', ...Array.from({ length: 10 }, (_, i) => `003${i}`)].map(
      (base) => `${base} FE0F 20E3`,
    );
    for (const keycap of keycaps) expect(copies).toContain(keycap);
  });

  it('keycap の他に、FE0F 抜きの名で配られる絵文字（❤️ など）にも写しを足している', () => {
    expect(copies).toContain('2764 FE0F');
    expect(copies.length).toBeGreaterThan(12);
  });

  it('VS16 入りの名で配られる ZWJ の並びに、VS16 を抜いた写しを足している（❤️‍🔥 と 🏳️‍🌈）', () => {
    // Chromium と WebKit は並びの途中の VS16 を落として組むことがあり、この写しが無いと二つの絵に割れる
    expect(copies).toContain('2764 200D 1F525');
    expect(copies).toContain('1F3F3 200D 1F308');
  });
});

/** FontFace と document.fonts の代わり（happy-dom には無い）。足した face を family と unicodeRange で残す */
class FakeFontFace {
  constructor(
    public family: string,
    public source: string,
    public descriptors: { unicodeRange?: string } = {},
  ) {}
}
function fakeFonts() {
  const faces = new Set<FakeFontFace>();
  return {
    faces,
    add: (face: FakeFontFace) => faces.add(face),
    delete: (face: FakeFontFace) => faces.delete(face),
    [Symbol.iterator]: () => faces.values(),
  };
}
/** @font-face の規則の代わり。cssText と src と unicode-range だけを持つ */
function fontFaceRule(src: string, range?: string) {
  return {
    cssText: `@font-face { src: ${src}; }`,
    style: {
      getPropertyValue: (name: string) =>
        name === 'src' ? src : name === 'unicode-range' ? (range ?? '') : '',
    },
  };
}

describe('絵文字の範囲に絞った写し（installNarrowEmojiFaces）', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function setup() {
    vi.stubGlobal('FontFace', FakeFontFace);
    const fonts = fakeFonts();
    const doc = { fonts } as unknown as Document;
    return { doc, fonts };
  }

  it('ASCII と Latin-1 と ™ を除いた範囲の写しを、規則ごとに足す', () => {
    const { doc, fonts } = setup();
    const sheet = {
      cssRules: [
        fontFaceRule(
          "url('/a.woff2') format('woff2')",
          'U+23, U+2a, U+30-39, U+a9, U+ae, U+200d, U+2122, U+1f600-1f64f',
        ),
        { cssText: 'body { color: red; }' },
        fontFaceRule("url('/b.woff2') format('woff2')", 'U+1f1e6-1f1ff'),
      ],
    } as unknown as CSSStyleSheet;

    expect(installNarrowEmojiFaces(doc, sheet, EMOJI_NARROW_FAMILY)).toBe(2);
    const added = [...fonts.faces];
    expect(added.map((face) => face.family)).toEqual([EMOJI_NARROW_FAMILY, EMOJI_NARROW_FAMILY]);
    expect(added.map((face) => face.source)).toEqual([
      "url('/a.woff2') format('woff2')",
      "url('/b.woff2') format('woff2')",
    ]);
    expect(added.map((face) => face.descriptors.unicodeRange)).toEqual([
      'U+200d, U+1f600-1f64f',
      'U+1f1e6-1f1ff',
    ]);
  });

  it('範囲が書かれていない規則は全ての範囲から除き、除く字だけの規則は足さない', () => {
    const { doc, fonts } = setup();
    const sheet = {
      cssRules: [
        fontFaceRule("url('/c.woff2')"),
        fontFaceRule("url('/d.woff2')", 'U+30-39, U+2122'),
      ],
    } as unknown as CSSStyleSheet;

    expect(installNarrowEmojiFaces(doc, sheet, EMOJI_NARROW_FAMILY)).toBe(1);
    expect([...fonts.faces][0]!.descriptors.unicodeRange).toBe('U+100-2121, U+2123-10ffff');
  });

  it('前に足した同じ名の写しを外してから足す（字を替えた時に古い写しが残らない）', () => {
    const { doc, fonts } = setup();
    const sheet = {
      cssRules: [fontFaceRule("url('/e.woff2')", 'U+1f600')],
    } as unknown as CSSStyleSheet;
    const other = new FakeFontFace('Other', "url('/x.woff2')");
    fonts.add(other);

    installNarrowEmojiFaces(doc, sheet, EMOJI_NARROW_FAMILY);
    installNarrowEmojiFaces(doc, sheet, EMOJI_NARROW_FAMILY);

    expect([...fonts.faces].filter((face) => face.family === EMOJI_NARROW_FAMILY)).toHaveLength(1);
    expect(fonts.faces.has(other)).toBe(true);
  });

  it('FontFace を持たない所では何もしない', () => {
    const doc = { fonts: undefined } as unknown as Document;
    expect(
      installNarrowEmojiFaces(
        doc,
        { cssRules: [] } as unknown as CSSStyleSheet,
        EMOJI_NARROW_FAMILY,
      ),
    ).toBe(0);
  });

  it('源（toString）だけで閉じている（描く前の script に埋めて走らせても動く）', () => {
    const { doc, fonts } = setup();
    const sheet = {
      cssRules: [fontFaceRule("url('/f.woff2')", 'U+30-39, U+1f600')],
    } as unknown as CSSStyleSheet;
    const copied = runInThisContext(
      `(${installNarrowEmojiFaces.toString()})`,
    ) as typeof installNarrowEmojiFaces;

    expect(copied(doc, sheet, EMOJI_NARROW_FAMILY)).toBe(1);
    expect([...fonts.faces][0]!.descriptors.unicodeRange).toBe('U+1f600');
  });
});

describe('絞った写しの口（applyEmojiFont と描く前の script）', () => {
  afterEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty(EMOJI_FONT_VAR);
    document.documentElement.style.removeProperty(EMOJI_NARROW_VAR);
    document.getElementById(EMOJI_FONT_LINK_ID)?.remove();
  });

  it('字を選ぶと、絞った写しの口に写しの名を置き、字の CSS を crossorigin 付きで読み込む', () => {
    applyEmojiFont(document, 'noto');

    expect(document.documentElement.style.getPropertyValue(EMOJI_NARROW_VAR)).toBe(
      `'${EMOJI_NARROW_FAMILY}',`,
    );
    expect(document.getElementById(EMOJI_FONT_LINK_ID)?.getAttribute('crossorigin')).toBe(
      'anonymous',
    );
  });

  it('OS の既定に戻すと、絞った写しの口も外す', () => {
    applyEmojiFont(document, 'twemoji');
    applyEmojiFont(document, 'os');

    expect(document.documentElement.style.getPropertyValue(EMOJI_NARROW_VAR)).toBe('');
  });

  it('描く前の script も、絞った写しの口を置き、字の CSS を crossorigin 付きで読み込む', () => {
    localStorage.setItem('preferences', JSON.stringify({ values: { emojiFont: 'twemoji' } }));
    runInThisContext(emojiFontBootScript('preferences'));

    expect(document.documentElement.style.getPropertyValue(EMOJI_NARROW_VAR)).toBe(
      `'${EMOJI_NARROW_FAMILY}',`,
    );
    expect(document.getElementById(EMOJI_FONT_LINK_ID)?.getAttribute('crossorigin')).toBe(
      'anonymous',
    );
  });

  it('描く前の script は、字の CSS を読み終えたら絞った写しを足す', () => {
    vi.stubGlobal('FontFace', FakeFontFace);
    const fonts = fakeFonts();
    Object.defineProperty(document, 'fonts', { value: fonts, configurable: true });
    try {
      localStorage.setItem('preferences', JSON.stringify({ values: { emojiFont: 'twemoji' } }));
      runInThisContext(emojiFontBootScript('preferences'));
      const link = document.getElementById(EMOJI_FONT_LINK_ID)!;
      Object.defineProperty(link, 'sheet', {
        value: { cssRules: [fontFaceRule("url('/g.woff2')", 'U+30-39, U+1f600')] },
        configurable: true,
      });
      link.dispatchEvent(new Event('load'));

      const added = [...fonts.faces];
      expect(added.map((face) => face.family)).toEqual([EMOJI_NARROW_FAMILY]);
      expect(added[0]!.descriptors.unicodeRange).toBe('U+1f600');
    } finally {
      Reflect.deleteProperty(document, 'fonts');
      vi.unstubAllGlobals();
    }
  });

  it('OS の既定では、描く前の script は絞った写しの口を置かない', () => {
    localStorage.setItem('preferences', JSON.stringify({ values: { emojiFont: 'os' } }));
    runInThisContext(emojiFontBootScript('preferences'));

    expect(document.documentElement.style.getPropertyValue(EMOJI_NARROW_VAR)).toBe('');
  });
});
