import { describe, expect, it } from 'vitest';

import { placeAnchored } from '../anchored-position';

const VIEWPORT = { width: 1000, height: 800 };
const SIZE = { width: 360, height: 440 };

function rect(left: number, top: number) {
  return { left, right: left + 28, top, bottom: top + 24 };
}

describe('placeAnchored', () => {
  it('余裕があれば下に左揃えで出す', () => {
    expect(placeAnchored(rect(100, 100), SIZE, VIEWPORT)).toEqual({ left: 100, top: 128 });
  });

  it('下で切れるなら上へ反転する', () => {
    expect(placeAnchored(rect(100, 700), SIZE, VIEWPORT)).toEqual({ left: 100, top: 256 });
  });

  it('右で切れるなら右揃えへ反転する', () => {
    expect(placeAnchored(rect(900, 100), SIZE, VIEWPORT).left).toBe(928 - 360);
  });

  it('上下どちらにも入らなければ広い側に出して画面内へ収める', () => {
    const small = { width: 320, height: 500 };
    expect(placeAnchored(rect(10, 400), SIZE, small)).toEqual({ left: 8, top: 8 });
  });
});
