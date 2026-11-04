import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import {
  DEFAULT_FREQUENT_EMOJI,
  EMOJI_CATEGORIES,
  emojiName,
  FREQUENT_EMOJI_COUNT,
  frequentEmojiEntries,
  loadEmojiData,
  readEmojiUseCounts,
  recordEmojiUse,
  searchEmoji,
  type EmojiDataset,
} from '../emoji-data';

describe('emoji-data（実物の emojibase）', () => {
  let dataset: EmojiDataset;

  beforeAll(async () => {
    dataset = await loadEmojiData();
  });

  beforeEach(() => {
    localStorage.clear();
  });

  it('既定の「よく使う」は全部データにある', () => {
    expect(frequentEmojiEntries(dataset, {}).map((entry) => entry.emoji)).toEqual([
      ...DEFAULT_FREQUENT_EMOJI,
    ]);
  });

  it('グループはカテゴリの列の順に並び、「コンポーネント」を含まない', () => {
    const order = EMOJI_CATEGORIES.map((category) => category.key).filter(
      (key) => key !== 'frequent',
    );
    const seen = [...new Set(dataset.sections.map((section) => section.category))];
    expect(seen).toEqual(order);
    expect(dataset.sections.map((section) => section.label)).not.toContain('コンポーネント');
  });

  it('shortcode は 1 つ目を使い、検索はすべての shortcode を見る', () => {
    const thumbs = dataset.byEmoji.get('👍');
    expect(thumbs?.shortcode).toBeTruthy();
    expect(searchEmoji(dataset, `:${thumbs?.shortcode}:`)).toContain(thumbs);
  });

  it('全角や大文字の検索語でも引ける', () => {
    expect(searchEmoji(dataset, 'ＯＫ').map((entry) => entry.emoji)).toContain('👍');
    expect(searchEmoji(dataset, 'ok').map((entry) => entry.emoji)).toContain('👍');
  });

  it('空や空白だけの検索語は何も返さない', () => {
    expect(searchEmoji(dataset, '   ')).toEqual([]);
    expect(searchEmoji(dataset, '::')).toEqual([]);
  });

  it('日本語名は異体字セレクタの有無を問わず引ける', () => {
    expect(emojiName('❤️')).toBe('赤いハート');
    expect(emojiName('❤')).toBe('赤いハート');
    expect(emojiName('not-an-emoji')).toBeNull();
  });

  it('使った回数の多い順に並び、同数は既定の順で、16 個に収まる', () => {
    recordEmojiUse('🐱');
    recordEmojiUse('🔥');
    recordEmojiUse('🔥');
    const entries = frequentEmojiEntries(dataset, readEmojiUseCounts());
    expect(entries.map((entry) => entry.emoji).slice(0, 3)).toEqual(['🔥', '🐱', '👍']);
    expect(entries).toHaveLength(FREQUENT_EMOJI_COUNT);
  });

  it('データに無い絵文字の回数は無視する', () => {
    const entries = frequentEmojiEntries(dataset, { nope: 100 });
    expect(entries[0]?.emoji).toBe('👍');
  });

  it('回数以外の値が入っていても読める', () => {
    localStorage.setItem('koyori.reactions.recent', '{"👍":"x","🔥":2}');
    expect(readEmojiUseCounts()).toEqual({ '🔥': 2 });
    localStorage.setItem('koyori.reactions.recent', '[1]');
    expect(readEmojiUseCounts()).toEqual({});
  });
});
