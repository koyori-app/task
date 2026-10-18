import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import UserSelectorList from '@/components/users/UserSelectorList.vue';

enableAutoUnmount(afterEach);

const users = [
  { id: 'u1', username: 'sousuke', avatar_url: null },
  { id: 'u2', username: 'yupix', avatar_url: null },
  { id: 'u3', username: 'kousuke', avatar_url: null },
];

function mountList(props: Record<string, unknown> = {}) {
  return mount(UserSelectorList, {
    props: { users, modelValue: [], ...props },
    attachTo: document.body,
  });
}

const nameOf = new Map(users.map((u) => [u.id, u.username]));

/** 候補の名を並びの順に読む（項の字にはアバターの頭文字も混じるので id から引く）。 */
function optionNames(wrapper: ReturnType<typeof mountList>) {
  return wrapper
    .findAll('[role="option"]')
    .map((o) => nameOf.get(o.attributes('data-user-id') ?? ''));
}

describe('UserSelectorList の絞り込み', () => {
  it('打った字で絞り込み、前方一致を先に並べる', async () => {
    const wrapper = mountList();
    expect(optionNames(wrapper)).toEqual(['sousuke', 'yupix', 'kousuke']);

    await wrapper.get('input').setValue('suke');

    expect(optionNames(wrapper)).toEqual(['sousuke', 'kousuke']);
  });

  it('絞り込むと、渡された順で後ろに居る前方一致の者が、部分一致の者より前に来る', async () => {
    // 渡す順は部分一致（kousuke）が先、前方一致（sukeroku）が後
    const wrapper = mountList({
      users: [
        { id: 'u3', username: 'kousuke', avatar_url: null },
        { id: 'u4', username: 'sukeroku', avatar_url: null },
      ],
    });

    await wrapper.get('input').setValue('suke');

    expect(wrapper.findAll('[role="option"]').map((o) => o.attributes('data-user-id'))).toEqual([
      'u4',
      'u3',
    ]);
  });

  it('当たらぬ時は「一致する利用者がいません」を出す', async () => {
    const wrapper = mountList();

    await wrapper.get('input').setValue('zzz');

    expect(wrapper.text()).toContain('一致する利用者がいません');
    expect(wrapper.findAll('[role="option"]').filter((o) => o.isVisible())).toHaveLength(0);
  });
});

describe('UserSelectorList の多くの人数', () => {
  it('描くのは上限まで。残りは数を添え、絞り込めば奥の者も出る', async () => {
    const wrapper = mountList({ maxRendered: 2 });

    expect(optionNames(wrapper)).toEqual(['sousuke', 'yupix']);
    expect(wrapper.text()).toContain('ほか 1 人。名で絞り込めます');

    await wrapper.get('input').setValue('kou');
    expect(optionNames(wrapper)).toEqual(['kousuke']);
    expect(wrapper.text()).not.toContain('ほか');
  });

  it('候補の名には頭文字の字が混じらない（アバターは支援技術から隠す）', () => {
    const wrapper = mountList();
    const avatar = wrapper.get('[data-user-id="u1"] [aria-hidden="true"]');

    expect(avatar.text()).not.toBe('');
  });
});

describe('UserSelectorList の選ぶ・外す', () => {
  it('複数: 押すと付け、もう一度押すと外す（toggle と update:modelValue）', async () => {
    const wrapper = mountList({ modelValue: ['u2'] });

    await wrapper.get('[data-user-id="u1"]').trigger('click');
    expect(wrapper.emitted('toggle')?.[0]).toEqual(['u1', true]);
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([['u2', 'u1']]);

    await wrapper.setProps({ modelValue: ['u2', 'u1'] });
    await wrapper.get('[data-user-id="u2"]').trigger('click');
    expect(wrapper.emitted('toggle')?.[1]).toEqual(['u2', false]);
    expect(wrapper.emitted('update:modelValue')?.[1]).toEqual([['u1']]);
  });

  it('選んだ者には aria-selected が付く', () => {
    const wrapper = mountList({ modelValue: ['u3'] });

    expect(wrapper.get('[data-user-id="u3"]').attributes('aria-selected')).toBe('true');
    expect(wrapper.get('[data-user-id="u1"]').attributes('aria-selected')).toBe('false');
  });

  it('単数: 別の者を押すと置き換える', async () => {
    const wrapper = mountList({ multiple: false, modelValue: ['u1'] });

    await wrapper.get('[data-user-id="u2"]').trigger('click');

    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([['u2']]);
    expect(wrapper.emitted('toggle')).toEqual([
      ['u2', true],
      ['u1', false],
    ]);
  });

  it('自分に「自分」と添える', () => {
    const wrapper = mountList({ currentUserId: 'u3' });

    expect(wrapper.get('[data-user-id="u3"]').text()).toContain('自分');
    expect(wrapper.get('[data-user-id="u1"]').text()).not.toContain('自分');
  });
});

describe('UserSelectorList のキーボード', () => {
  it('上下で候補を移り、Enter で選ぶ', async () => {
    const wrapper = mountList();
    const input = wrapper.get('input');

    await input.trigger('keydown', { key: 'ArrowDown' });
    await input.trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
    expect(wrapper.get('[data-user-id="u2"]').attributes('data-highlighted')).toBeDefined();

    await input.trigger('keydown', { key: 'Enter' });

    expect(wrapper.emitted('toggle')?.[0]).toEqual(['u2', true]);
  });

  it('IME の変換中の Enter では選ばない（変換を確かめた後の Enter では選ぶ）', async () => {
    const wrapper = mountList();
    const input = wrapper.get('input');
    await input.trigger('keydown', { key: 'ArrowDown' });

    await input.trigger('compositionstart');
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    expect(wrapper.emitted('toggle')).toBeUndefined();

    await input.trigger('compositionend');
    await input.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('toggle')).toHaveLength(1);
  });
});

describe('UserSelectorList の状態', () => {
  it('取得中は読み込み中を出し、「いません」と混ぜない', () => {
    const wrapper = mountList({ users: [], state: { loading: true } });

    expect(wrapper.get('[role="status"]').text()).toBe('読み込み中…');
    expect(wrapper.text()).not.toContain('選べる利用者がいません');
  });

  it('失敗は失敗として出し、再試行できる', async () => {
    const onRetry = vi.fn();
    const wrapper = mountList({ users: [], state: { error: true, onRetry } });

    expect(wrapper.get('[role="alert"]').text()).toContain('候補を読み込めませんでした');
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '再試行')!
      .trigger('click');
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('取得できて 0 人のときだけ「いません」を出す（文は呼び手が決められる）', () => {
    const wrapper = mountList({ users: [], emptyText: '担当者に指定できる利用者がいません' });

    expect(wrapper.text()).toContain('担当者に指定できる利用者がいません');
  });
});
