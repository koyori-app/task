import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { emojiDataset, loadEmojiData } from '@/lib/emoji-data';

import ReactionChip from '../ReactionChip.vue';

function mountChip(props: Partial<InstanceType<typeof ReactionChip>['$props']> = {}) {
  return mount(ReactionChip, { props: { emoji: '👍', count: 3, reacted: false, ...props } });
}

describe('ReactionChip', () => {
  beforeEach(() => {
    emojiDataset.value = null;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('押した状態を aria-pressed で伝える', () => {
    expect(mountChip({ reacted: false }).attributes('aria-pressed')).toBe('false');
    expect(mountChip({ reacted: true }).attributes('aria-pressed')).toBe('true');
  });

  it('データの読み込み前は絵文字そのものを読み上げる', () => {
    expect(mountChip({ reacted: true }).attributes('aria-label')).toBe('👍 3 件、押しています');
    expect(mountChip({ reacted: false }).attributes('aria-label')).toBe('👍 3 件');
  });

  it('データがあれば日本語名で読み上げる', async () => {
    await loadEmojiData();
    expect(mountChip({ reacted: true }).attributes('aria-label')).toBe(
      'サムズアップ 3 件、押しています',
    );
  });

  it.each([
    [1, '1'],
    [999, '999'],
    [1000, '999+'],
    [12345, '999+'],
  ])('件数 %i は %s と書く', (count, text) => {
    expect(mountChip({ count }).find('.reaction-chip__count').text()).toBe(text);
  });

  it('1,000 以上でも読み上げは実数', () => {
    expect(mountChip({ count: 1234 }).attributes('aria-label')).toBe('👍 1234 件');
  });

  it('押すと toggle を出す', async () => {
    const wrapper = mountChip();
    await wrapper.trigger('click');
    expect(wrapper.emitted('toggle')).toHaveLength(1);
  });

  it('disabled の札は押せないがフォーカスは失わない', async () => {
    const wrapper = mountChip({ disabled: true });
    await wrapper.trigger('click');
    expect(wrapper.emitted('toggle')).toBeUndefined();
    expect(wrapper.attributes('aria-disabled')).toBe('true');
    expect(wrapper.attributes('disabled')).toBeUndefined();
  });

  it('タッチの長押しは longPress を出し、離した時の click で切り替えない', async () => {
    vi.useFakeTimers();
    const wrapper = mountChip();
    await wrapper.trigger('pointerdown', { pointerType: 'touch', clientX: 0, clientY: 0 });
    vi.advanceTimersByTime(500);
    expect(wrapper.emitted('longPress')).toHaveLength(1);
    await wrapper.trigger('pointerup');
    await wrapper.trigger('click');
    expect(wrapper.emitted('toggle')).toBeUndefined();
    // 次の普通のタップは切り替わる
    await wrapper.trigger('pointerdown', { pointerType: 'touch', clientX: 0, clientY: 0 });
    await wrapper.trigger('pointerup');
    await wrapper.trigger('click');
    expect(wrapper.emitted('toggle')).toHaveLength(1);
  });

  it('指が動いたらスクロールと見なして長押しをやめる', async () => {
    vi.useFakeTimers();
    const wrapper = mountChip();
    await wrapper.trigger('pointerdown', { pointerType: 'touch', clientX: 0, clientY: 0 });
    await wrapper.trigger('pointermove', { clientX: 0, clientY: 30 });
    vi.advanceTimersByTime(500);
    expect(wrapper.emitted('longPress')).toBeUndefined();
  });

  it('マウスの長押しでは一覧を開かない', async () => {
    vi.useFakeTimers();
    const wrapper = mountChip();
    await wrapper.trigger('pointerdown', { pointerType: 'mouse', clientX: 0, clientY: 0 });
    vi.advanceTimersByTime(500);
    expect(wrapper.emitted('longPress')).toBeUndefined();
  });
});
