import { describe, expect, it } from 'vitest';

import { codePointLength } from '../code-points';

describe('codePointLength', () => {
  it('サロゲートペアを 1 文字と数える（backend の chars().count() と一致）', () => {
    // '😀' は UTF-16 で 2 単位・コードポイントで 1。String.length は 200 と数えるため、
    // 画面だけが backend の通す名前を弾く
    expect(codePointLength('😀'.repeat(100))).toBe(100);
    expect('😀'.repeat(100)).toHaveLength(200);
  });

  it('BMP 内の文字と空文字は素直に数える', () => {
    expect(codePointLength('')).toBe(0);
    expect(codePointLength('abc')).toBe(3);
    expect(codePointLength('界'.repeat(100))).toBe(100);
  });
});
