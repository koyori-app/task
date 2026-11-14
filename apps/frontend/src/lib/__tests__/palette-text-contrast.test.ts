import { readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { oklch, ratio } from './design-token';

/**
 * Tailwind のパレット色（text-amber-600 など）を、変種なしで文字色に使う箇所の対比。
 * 変種なしの値は明るい配色（地は白）でも暗い配色でも当たるので、白地で 4.5:1 以上でなければならない。
 * 暗い配色だけに当てる色は `dark:` を付けて書く（ここでは見ない）。
 * 同じ行に変種なしの `bg-<色>-<濃さ>` があれば、その地（不透明度があれば白に重ねた色）で測る。
 * 文字色の不透明度（`text-amber-700/80`）も、地に重ねた色で測る。
 */
const THEME_CSS = readFileSync(
  createRequire(import.meta.url).resolve('tailwindcss/theme.css'),
  'utf8',
);
const WHITE: [number, number, number] = [1, 1, 1];
type Rgb = [number, number, number];

/** theme.css にあるパレットの色名（`--color-<名前>-500` を持つもの）。 */
const COLOR_NAMES = [...THEME_CSS.matchAll(/--color-([a-z]+)-500:/g)].map((m) => m[1]).join('|');

const encode = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const decode = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

/** 上に重ねた色を、不透明度で下の色と混ぜる。ブラウザーと同じく符号化した sRGB の上で混ぜ、線形に戻す。 */
function over(top: Rgb, alpha: number, bottom: Rgb): Rgb {
  return top.map((c, i) => decode(encode(c) * alpha + encode(bottom[i]) * (1 - alpha))) as Rgb;
}

/** パレット色（`amber-700` など）を線形 sRGB へ。 */
function palette(name: string): [number, number, number] {
  const m = new RegExp(`--color-${name}:\\s*oklch\\(([\\d.]+)% ([\\d.]+) ([\\d.]+)\\)`).exec(
    THEME_CSS,
  );
  if (!m) throw new Error(`Tailwind のパレットに ${name} が無い`);
  return oklch(Number(m[1]) / 100, Number(m[2]), Number(m[3]));
}

/** 変種なしの `text-<色>-<濃さ>[/<不透明度>]`（`dark:text-…` や `hover:text-…` は含めない）。 */
const PLAIN_PALETTE_TEXT = new RegExp(
  `(?<![\\w:-])text-(${COLOR_NAMES})-(\\d{2,3})(?:/(\\d{1,3}))?(?![\\w-])`,
  'g',
);
/** 同じ行の変種なしの `bg-<色>-<濃さ>[/<不透明度>]`。 */
const PLAIN_PALETTE_BG = new RegExp(
  `(?<![\\w:-])bg-(${COLOR_NAMES})-(\\d{2,3})(?:/(\\d{1,3}))?(?![\\w-])`,
);

describe('パレット色の文字の対比', () => {
  const root = resolve(__dirname, '../..');
  const uses = readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((file) => /\.(vue|ts)$/.test(file) && !file.includes('__tests__'))
    .flatMap((file) =>
      readFileSync(resolve(root, file), 'utf8')
        .split('\n')
        .flatMap((line, i) =>
          [...line.matchAll(PLAIN_PALETTE_TEXT)].map((m) => {
            const bg = PLAIN_PALETTE_BG.exec(line);
            return {
              where: `${file}:${i + 1}`,
              color: `${m[1]}-${m[2]}`,
              alpha: m[3] ? Number(m[3]) / 100 : 1,
              bg: bg ? `${bg[1]}-${bg[2]}` : null,
              bgAlpha: bg?.[3] ? Number(bg[3]) / 100 : 1,
            };
          }),
        ),
    );

  it('文字色にパレット色を使う箇所を拾えている', () => {
    expect(uses.length).toBeGreaterThan(0);
  });

  it('変種なしの text-<パレット色> は、置かれた地（無ければ白）で 4.5:1 以上', () => {
    const offenders = uses
      .filter(({ color, alpha, bg, bgAlpha }) => {
        const ground = bg ? over(palette(bg), bgAlpha, WHITE) : WHITE;
        return ratio(over(palette(color), alpha, ground), ground) < 4.5;
      })
      .map(({ where, color, bg }) => `${where} text-${color}${bg ? ` on bg-${bg}` : ''}`);
    expect(offenders).toEqual([]);
  });

  it('色付きの地では白地より対比が下がる（地を測れている）', () => {
    const ground = over(palette('green-600'), 0.1, WHITE);
    expect(ratio(palette('green-700'), WHITE)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(palette('green-700'), ground)).toBeLessThan(4.5);
    expect(ratio(palette('green-800'), ground)).toBeGreaterThanOrEqual(4.5);
  });

  it('amber-600 は白地で 4.5:1 に届かない（検査が効いている）', () => {
    expect(ratio(palette('amber-600'), WHITE)).toBeLessThan(4.5);
    expect(ratio(palette('amber-700'), WHITE)).toBeGreaterThanOrEqual(4.5);
  });
});
