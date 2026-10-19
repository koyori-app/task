import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { MEMBER_AVATAR_COLORS, USER_AVATAR_INITIALS_CLASS } from '@/lib/user-avatar';

/**
 * 頭文字の字と地の対比（WCAG 2.x の相対輝度）。色は task の design token（tailwind.css の
 * `:root` と `.dark`）から読み、頭文字の組が明るい面でも暗い面でも 4.5:1 以上であることを縛る。
 */
const CSS = readFileSync(resolve(__dirname, '../../assets/css/tailwind.css'), 'utf8');

function block(selector: string): string {
  const start = CSS.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`${selector} が見つからぬ`);
  return CSS.slice(start, CSS.indexOf('\n}', start));
}

/** `--name: oklch(L C H)` を線形 sRGB へ。 */
function token(scope: string, name: string): [number, number, number] {
  const m = new RegExp(`--${name}:\\s*oklch\\(([\\d.]+) ([\\d.]+)(?: ([\\d.]+))?\\)`).exec(
    block(scope),
  );
  if (!m) throw new Error(`${scope} の --${name} が oklch で読めぬ`);
  const [L, C, h] = [Number(m[1]), Number(m[2]), (Number(m[3] ?? 0) * Math.PI) / 180];
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const mm = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * mm + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * mm - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * mm + 1.707614701 * s,
  ];
}

const luminance = ([r, g, b]: number[]) => 0.2126 * r + 0.7152 * g + 0.0722 * b;

function ratio(a: number[], b: number[]): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function hex(value: string): [number, number, number] {
  const lin = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  const n = value.replace('#', '');
  return [0, 2, 4].map((i) => lin(parseInt(n.slice(i, i + 2), 16) / 255)) as [
    number,
    number,
    number,
  ];
}

describe('頭文字の対比', () => {
  it('頭文字の組は bg-muted と text-foreground', () => {
    expect(USER_AVATAR_INITIALS_CLASS.split(' ').sort()).toEqual(['bg-muted', 'text-foreground']);
  });

  it.each([
    [':root', '明るい面'],
    ['.dark', '暗い面'],
  ])('%s（%s）の text-foreground は bg-muted 上で 4.5:1 以上', (scope) => {
    expect(ratio(token(scope, 'foreground'), token(scope, 'muted'))).toBeGreaterThanOrEqual(4.5);
  });

  it('従来の薄い字（text-muted-foreground）は明るい面で 4.5:1 に届かなかった（陽性対照）', () => {
    const r = ratio(token(':root', 'muted-foreground'), token(':root', 'muted'));
    expect(r).toBeLessThan(4.5);
    expect(r).toBeGreaterThan(4.3);
  });

  it('灰色（opacity-60）にしても、面の上で 4.5:1 以上', () => {
    // ブラウザは符号化した sRGB の値のまま重ねる（線形のまま混ぜると値がずれる）
    const encode = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);
    const decode = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    const mix = (fg: number[], bg: number[]) =>
      fg.map((v, i) => decode(0.6 * encode(v) + 0.4 * encode(bg[i])));
    for (const scope of [':root', '.dark']) {
      const page = token(scope, 'background');
      const text = mix(token(scope, 'foreground'), page);
      const ground = mix(token(scope, 'muted'), page);
      expect(ratio(text, ground)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it.each([...MEMBER_AVATAR_COLORS])('メンバー一覧の地 %s は白い字で 4.5:1 以上', (color) => {
    expect(ratio([1, 1, 1], hex(color))).toBeGreaterThanOrEqual(4.5);
  });
});
