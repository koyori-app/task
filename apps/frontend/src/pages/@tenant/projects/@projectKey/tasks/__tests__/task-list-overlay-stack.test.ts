import { describe, expect, it } from 'vitest';

import { popOverlayParent, pushOverlayParent } from '../task-list-overlay-stack';

describe('pushOverlayParent', () => {
  it('子を開くと今のタスクが積まれる', () => {
    expect(pushOverlayParent([], 'ENG-1', 'ENG-21')).toEqual(['ENG-1']);
    expect(pushOverlayParent(['ENG-1'], 'ENG-21', 'ENG-30')).toEqual(['ENG-1', 'ENG-21']);
  });

  // 「親タスク」ボタンで積んである親へ戻ったら、その親を二重に積まない
  it('積んである親を開き直すと積みを 1 段降ろす', () => {
    expect(pushOverlayParent(['ENG-1'], 'ENG-21', 'ENG-1')).toEqual([]);
  });

  it('同じタスクを開き直しても積まない', () => {
    expect(pushOverlayParent(['ENG-1'], 'ENG-21', 'ENG-21')).toEqual(['ENG-1']);
  });

  it('何も開いていなければ積まない', () => {
    expect(pushOverlayParent([], null, 'ENG-1')).toEqual([]);
  });

  it('渡した積みを書き換えない', () => {
    const stack = ['ENG-1'];
    pushOverlayParent(stack, 'ENG-21', 'ENG-30');
    pushOverlayParent(stack, 'ENG-21', 'ENG-1');
    expect(stack).toEqual(['ENG-1']);
  });
});

describe('popOverlayParent', () => {
  it('閉じると直前の親が返り、積みが減る', () => {
    expect(popOverlayParent(['ENG-1', 'ENG-21'])).toEqual({ stack: ['ENG-1'], next: 'ENG-21' });
    expect(popOverlayParent(['ENG-1'])).toEqual({ stack: [], next: 'ENG-1' });
  });

  it('積みが空なら null（全部閉じる）', () => {
    expect(popOverlayParent([])).toEqual({ stack: [], next: null });
  });
});
