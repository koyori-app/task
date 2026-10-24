import { describe, expect, it } from 'vitest';

import {
  MEMBER_AVATAR_COLORS,
  USER_AVATAR_INACTIVE_CLASS,
  USER_AVATAR_INITIALS_CLASS,
} from '@/lib/user-avatar';

import { hex, ratio, token } from './design-token';

/**
 * 頭文字の字と地の対比（WCAG 2.x の相対輝度）。色は task の design token（tailwind.css の
 * `:root` と `.dark`）から読み、頭文字の組が明るい面でも暗い面でも 4.5:1 以上であることを縛る。
 */
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

  it('灰色（USER_AVATAR_INACTIVE_CLASS の opacity）にしても、面の上で 4.5:1 以上', () => {
    const opacity = /\bopacity-(\d+)\b/.exec(USER_AVATAR_INACTIVE_CLASS);
    if (!opacity) throw new Error('USER_AVATAR_INACTIVE_CLASS に opacity-* が無い');
    const alpha = Number(opacity[1]) / 100;
    // ブラウザは符号化した sRGB の値のまま重ねる（線形のまま混ぜると値がずれる）
    const encode = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);
    const decode = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    const mix = (fg: number[], bg: number[]) =>
      fg.map((v, i) => decode(alpha * encode(v) + (1 - alpha) * encode(bg[i])));
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
