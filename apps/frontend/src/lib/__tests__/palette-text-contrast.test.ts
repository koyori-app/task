import { readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { oklch, ratio } from './design-token';

/**
 * Tailwind のパレット色（text-amber-600 など）を、変種なしで文字色に使う箇所の対比。
 * 変種なしの値は明るい配色（地は白）でも暗い配色でも当たるので、白地で 4.5:1 以上でなければならない。
 * 暗い配色だけに当てる色は `dark:` を付けて書く（ここでは見ない）。
 * 地に色が付く札やバッジの字も、白地で足りる濃さ（700 以上）に揃えている。
 */
const THEME_CSS = readFileSync(
  createRequire(import.meta.url).resolve('tailwindcss/theme.css'),
  'utf8',
);
const WHITE: [number, number, number] = [1, 1, 1];

/** パレット色（`amber-700` など）を線形 sRGB へ。 */
function palette(name: string): [number, number, number] {
  const m = new RegExp(`--color-${name}:\\s*oklch\\(([\\d.]+)% ([\\d.]+) ([\\d.]+)\\)`).exec(
    THEME_CSS,
  );
  if (!m) throw new Error(`Tailwind のパレットに ${name} が無い`);
  return oklch(Number(m[1]) / 100, Number(m[2]), Number(m[3]));
}

/** 変種なしの `text-<色>-<濃さ>`（`dark:text-…` や `hover:text-…` は含めない）。 */
const PLAIN_PALETTE_TEXT =
  /(?<![\w:-])text-(amber|yellow|orange|green|emerald|lime|red|rose|sky|blue|cyan|teal|pink|purple|violet|indigo|fuchsia|slate|gray|zinc|neutral|stone)-(\d{2,3})(?![\w/-])/g;

describe('パレット色の文字の対比', () => {
  const root = resolve(__dirname, '../..');
  const uses = readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((file) => /\.(vue|ts)$/.test(file) && !file.includes('__tests__'))
    .flatMap((file) =>
      readFileSync(resolve(root, file), 'utf8')
        .split('\n')
        .flatMap((line, i) =>
          [...line.matchAll(PLAIN_PALETTE_TEXT)].map((m) => ({
            where: `${file}:${i + 1}`,
            color: `${m[1]}-${m[2]}`,
          })),
        ),
    );

  it('文字色にパレット色を使う箇所を拾えている', () => {
    expect(uses.length).toBeGreaterThan(0);
  });

  it('変種なしの text-<パレット色> は白地で 4.5:1 以上', () => {
    const offenders = uses
      .filter(({ color }) => ratio(palette(color), WHITE) < 4.5)
      .map(({ where, color }) => `${where} text-${color}`);
    expect(offenders).toEqual([]);
  });

  it('amber-600 は白地で 4.5:1 に届かない（検査が効いている）', () => {
    expect(ratio(palette('amber-600'), WHITE)).toBeLessThan(4.5);
    expect(ratio(palette('amber-700'), WHITE)).toBeGreaterThanOrEqual(4.5);
  });
});
