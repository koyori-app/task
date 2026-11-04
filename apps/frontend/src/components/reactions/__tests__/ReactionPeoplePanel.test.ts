import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ReactionSummary, ReactionUser } from '@/lib/reactions';

import ReactionPeoplePanel from '../ReactionPeoplePanel.vue';

const ALICE: ReactionUser = { id: 'a', name: 'Alice', avatar_url: null };
const BOB: ReactionUser = { id: 'b', name: 'Bob', avatar_url: null };
const REACTIONS: ReactionSummary[] = [
  { emoji: '👍', count: 1, reacted: true, users: [ALICE] },
  { emoji: '🎉', count: 1, reacted: false, users: [BOB] },
];

function mountPanel(fetchUsers: (emoji: string) => Promise<ReactionUser[]>) {
  return mount(ReactionPeoplePanel, {
    attachTo: document.body,
    props: {
      reactions: REACTIONS,
      initialEmoji: '🎉',
      fetchUsers,
      profileHref: (id: string) => `/users/${id}`,
      currentUserId: 'b',
    },
  });
}

describe('ReactionPeoplePanel', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('読み込み中は Skeleton を出し、終わると人の行にする', async () => {
    let resolve: (users: ReactionUser[]) => void = () => undefined;
    const wrapper = mountPanel(() => new Promise((done) => (resolve = done)));
    expect(wrapper.findAll('[data-testid="people-skeleton"]').length).toBeGreaterThan(0);

    resolve([BOB]);
    await flushPromises();
    expect(wrapper.find('[data-testid="people-skeleton"]').exists()).toBe(false);
    const row = wrapper.get('li');
    expect(row.get('a').attributes('href')).toBe('/users/b');
    expect(row.text()).toContain('自分');
  });

  it('開いた絵文字ごとに取得し、開いた絵文字は取り直さない', async () => {
    const fetchUsers = vi.fn(async (emoji: string) => (emoji === '👍' ? [ALICE] : [BOB]));
    const wrapper = mountPanel(fetchUsers);
    await flushPromises();
    expect(fetchUsers.mock.calls).toEqual([['🎉']]);

    const tabs = wrapper.findAll('[role="tab"]');
    await tabs[0]?.trigger('click');
    await flushPromises();
    expect(fetchUsers.mock.calls).toEqual([['🎉'], ['👍']]);
    expect(wrapper.get('li').text()).toContain('Alice');

    await tabs[1]?.trigger('click');
    await flushPromises();
    expect(fetchUsers).toHaveBeenCalledTimes(2);
    expect(wrapper.get('li').text()).toContain('Bob');
  });

  it('失敗を画面に出し、読み直せる', async () => {
    const fetchUsers = vi
      .fn<(emoji: string) => Promise<ReactionUser[]>>()
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValueOnce([BOB]);
    const wrapper = mountPanel(fetchUsers);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('読み込めませんでした');

    await wrapper.get('[role="alert"] button').trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get('li').text()).toContain('Bob');
  });

  it('Esc で close を出し、外の dialog へは伝えない', async () => {
    const wrapper = mountPanel(async () => [BOB]);
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    const outer = vi.fn();
    document.body.addEventListener('keydown', outer);
    wrapper.element.dispatchEvent(event);
    document.body.removeEventListener('keydown', outer);
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(event.defaultPrevented).toBe(true);
    expect(outer).not.toHaveBeenCalled();
  });

  it('矢印キーでタブを移る', async () => {
    const wrapper = mountPanel(async () => [BOB]);
    await flushPromises();
    await wrapper.get('[role="tablist"]').trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
    const tabs = wrapper.findAll('[role="tab"]');
    expect(tabs[0]?.attributes('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(tabs[0]?.element);
  });
});
