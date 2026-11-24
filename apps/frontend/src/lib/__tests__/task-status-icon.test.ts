import { describe, expect, it } from 'vitest';
import { statusFill } from '@/lib/task-status-icon';

const s = (id: string, position: number, done = false) => ({ id, position, is_done_state: done });

describe('statusFill', () => {
  it('ステータスが 2 個なら、未完了は点線の円、完了は塗りの円', () => {
    const statuses = [s('todo', 0), s('done', 1, true)];
    expect(statusFill(statuses, 'todo')).toEqual({ kind: 'empty', fraction: 0 });
    expect(statusFill(statuses, 'done')).toEqual({ kind: 'done', fraction: 1 });
  });

  it('4 個なら、未完了の 3 つを 0・1/3・2/3 で埋める', () => {
    const statuses = [s('a', 0), s('b', 1), s('c', 2), s('d', 3, true)];
    expect(statusFill(statuses, 'a')).toEqual({ kind: 'empty', fraction: 0 });
    expect(statusFill(statuses, 'b')).toEqual({ kind: 'partial', fraction: 1 / 3 });
    expect(statusFill(statuses, 'c')).toEqual({ kind: 'partial', fraction: 2 / 3 });
    expect(statusFill(statuses, 'd').kind).toBe('done');
  });

  it('7 個で完了扱いが複数あっても、未完了の数だけで割る', () => {
    const statuses = [
      s('a', 0),
      s('b', 1),
      s('c', 2),
      s('d', 3),
      s('e', 4),
      s('done', 5, true),
      s('closed', 6, true),
    ];
    expect(statusFill(statuses, 'e')).toEqual({ kind: 'partial', fraction: 4 / 5 });
    expect(statusFill(statuses, 'done').kind).toBe('done');
    expect(statusFill(statuses, 'closed').kind).toBe('done');
    // どの未完了も 1 に届かない（塗りの円は完了だけ）
    for (const id of ['a', 'b', 'c', 'd', 'e']) {
      expect(statusFill(statuses, id).fraction).toBeLessThan(1);
    }
  });

  it('position が飛び飛びで並びが逆でも、position の昇順で数える', () => {
    const statuses = [s('done', 90, true), s('late', 40), s('first', 10), s('mid', 25)];
    expect(statusFill(statuses, 'first').kind).toBe('empty');
    expect(statusFill(statuses, 'mid')).toEqual({ kind: 'partial', fraction: 1 / 3 });
    expect(statusFill(statuses, 'late')).toEqual({ kind: 'partial', fraction: 2 / 3 });
  });

  it('完了扱いが未完了より前の position にあっても、未完了の並びには数えない', () => {
    const statuses = [s('done', 0, true), s('a', 1), s('b', 2)];
    expect(statusFill(statuses, 'a').kind).toBe('empty');
    expect(statusFill(statuses, 'b')).toEqual({ kind: 'partial', fraction: 1 / 2 });
  });

  it('知らない id と未設定は unknown', () => {
    const statuses = [s('a', 0)];
    expect(statusFill(statuses, 'x')).toEqual({ kind: 'unknown', fraction: 0 });
    expect(statusFill(statuses, null)).toEqual({ kind: 'unknown', fraction: 0 });
    expect(statusFill([], 'a').kind).toBe('unknown');
  });
});
