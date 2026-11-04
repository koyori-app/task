import { describe, expect, it } from 'vitest';

import { applyReaction, type ReactionSummary } from '../reactions';

const ME = { id: 'me', name: '自分', avatar_url: null };
const OTHER = { id: 'other', name: '他の人', avatar_url: null };

describe('applyReaction', () => {
  it('無い絵文字は末尾に足す', () => {
    const thumbs: ReactionSummary = { emoji: '👍', count: 1, reacted: false, users: [OTHER] };
    expect(applyReaction([thumbs], '🎉', true, ME)).toEqual([
      thumbs,
      { emoji: '🎉', count: 1, reacted: true, users: [ME] },
    ]);
  });

  it('他の人の札を押すと件数が増え、本人が先頭に来る', () => {
    const thumbs: ReactionSummary = { emoji: '👍', count: 1, reacted: false, users: [OTHER] };
    expect(applyReaction([thumbs], '👍', true, ME)).toEqual([
      { emoji: '👍', count: 2, reacted: true, users: [ME, OTHER] },
    ]);
  });

  it('カードの上限の 5 人を超えて持たない', () => {
    const users = Array.from({ length: 5 }, (_, index) => ({ ...OTHER, id: `u-${index}` }));
    const [next] = applyReaction(
      [{ emoji: '👍', count: 9, reacted: false, users }],
      '👍',
      true,
      ME,
    );
    expect(next?.count).toBe(10);
    expect(next?.users).toHaveLength(5);
    expect(next?.users[0]).toEqual(ME);
  });

  it('外すと件数が減り、本人が消える', () => {
    const thumbs: ReactionSummary = { emoji: '👍', count: 2, reacted: true, users: [ME, OTHER] };
    expect(applyReaction([thumbs], '👍', false, ME)).toEqual([
      { emoji: '👍', count: 1, reacted: false, users: [OTHER] },
    ]);
  });

  it('最後の 1 人が外すと札ごと消える', () => {
    const thumbs: ReactionSummary = { emoji: '👍', count: 1, reacted: true, users: [ME] };
    expect(applyReaction([thumbs], '👍', false, ME)).toEqual([]);
  });

  it('異体字セレクタの有無が違えば別の絵文字として扱う（backend と同じ）', () => {
    const heart: ReactionSummary = { emoji: '❤', count: 1, reacted: true, users: [ME] };
    expect(applyReaction([heart], '❤️', true, ME)).toHaveLength(2);
  });

  it('本人が不明でも件数は動く', () => {
    expect(applyReaction([], '👍', true, null)).toEqual([
      { emoji: '👍', count: 1, reacted: true, users: [] },
    ]);
  });
});
