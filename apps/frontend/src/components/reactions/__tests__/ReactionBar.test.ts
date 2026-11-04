import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ReactionSummary, ReactionUser } from '@/lib/reactions';

import ReactionBar from '../ReactionBar.vue';
import ReactionPeopleCard from '../ReactionPeopleCard.vue';
import ReactionPeoplePanel from '../ReactionPeoplePanel.vue';

function users(count: number): ReactionUser[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `user-${index}`,
    name: `利用者 ${index}`,
    avatar_url: null,
  }));
}

const THUMBS: ReactionSummary = { emoji: '👍', count: 8, reacted: true, users: users(5) };
const HEART: ReactionSummary = { emoji: '❤️', count: 1, reacted: false, users: users(1) };

function mountBar(props: Record<string, unknown> = {}) {
  return mount(ReactionBar, {
    attachTo: document.body,
    props: {
      reactions: [THUMBS, HEART],
      canReact: true,
      fetchUsers: async () => users(8),
      ...props,
    },
  });
}

function chip(wrapper: ReturnType<typeof mountBar>, emoji: string) {
  return wrapper.get(`[data-emoji="${emoji}"]`);
}

describe('ReactionBar', () => {
  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('札が 0 件なら行ごと消える', () => {
    const wrapper = mountBar({ reactions: [] });
    expect(wrapper.find('[role="group"]').exists()).toBe(false);
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('alwaysShowAdd なら 0 件でも追加ボタンを出す', () => {
    const wrapper = mountBar({ reactions: [], alwaysShowAdd: true });
    expect(wrapper.find('[aria-label="リアクションを追加"]').exists()).toBe(true);
  });

  it('押せない人には 0 件の行も追加ボタンも出さない', () => {
    expect(
      mountBar({ reactions: [], alwaysShowAdd: true, canReact: false })
        .find('[role="group"]')
        .exists(),
    ).toBe(false);
    expect(mountBar({ canReact: false }).find('[aria-label="リアクションを追加"]').exists()).toBe(
      false,
    );
  });

  it('札が 0 件でも失敗の文言は出す（コメントの ⋯ の隣のボタンから追加して失敗した時）', () => {
    const wrapper = mountBar({ reactions: [], error: '追加できませんでした' });
    expect(wrapper.get('[role="alert"]').text()).toBe('追加できませんでした');
    expect(wrapper.find('[role="group"]').exists()).toBe(false);
  });

  it('札を押すと toggle を出す', async () => {
    const wrapper = mountBar();
    await chip(wrapper, '👍').trigger('click');
    expect(wrapper.emitted('toggle')).toEqual([['👍']]);
  });

  it.each([{ busy: true }, { canReact: false }])('%o の間は toggle を出さない', async (props) => {
    const wrapper = mountBar(props);
    await chip(wrapper, '👍').trigger('click');
    expect(wrapper.emitted('toggle')).toBeUndefined();
  });

  it('ホバーして 400ms 待つとカードが出て、離れると消える', async () => {
    vi.useFakeTimers();
    const wrapper = mountBar();
    await chip(wrapper, '👍').trigger('pointerenter', { pointerType: 'mouse' });
    vi.advanceTimersByTime(399);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(false);
    vi.advanceTimersByTime(1);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(true);

    await chip(wrapper, '👍').trigger('pointerleave');
    vi.advanceTimersByTime(150);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(false);
  });

  it('札からカードへ移る間は閉じない', async () => {
    vi.useFakeTimers();
    const wrapper = mountBar();
    await chip(wrapper, '👍').trigger('pointerenter', { pointerType: 'mouse' });
    vi.advanceTimersByTime(400);
    await wrapper.vm.$nextTick();

    await chip(wrapper, '👍').trigger('pointerleave');
    await wrapper.get('[data-reaction-card]').trigger('pointerenter');
    vi.advanceTimersByTime(1000);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(true);
  });

  it('ホバーで開いたカードは、フォーカスが外にあっても Esc で閉じる', async () => {
    vi.useFakeTimers();
    const wrapper = mountBar();
    await chip(wrapper, '👍').trigger('pointerenter', { pointerType: 'mouse' });
    vi.advanceTimersByTime(400);
    await wrapper.vm.$nextTick();

    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    document.body.dispatchEvent(event);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(false);
    expect(event.defaultPrevented).toBe(true);
  });

  it('タッチのホバーではカードを出さない', async () => {
    vi.useFakeTimers();
    const wrapper = mountBar();
    await chip(wrapper, '👍').trigger('pointerenter', { pointerType: 'touch' });
    vi.advanceTimersByTime(400);
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReactionPeopleCard).exists()).toBe(false);
  });

  it('openPeople で全員の一覧が開き、Esc で閉じて元の札へ戻る', async () => {
    const wrapper = mountBar();
    (wrapper.vm as unknown as { openPeople(emoji?: string): void }).openPeople('👍');
    await flushPromises();
    const panel = wrapper.findComponent(ReactionPeoplePanel);
    expect(panel.exists()).toBe(true);

    await panel.trigger('keydown', { key: 'Escape' });
    expect(wrapper.findComponent(ReactionPeoplePanel).exists()).toBe(false);
    expect(document.activeElement).toBe(chip(wrapper, '👍').element);
  });
});
