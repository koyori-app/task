import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import UserSelector from '@/components/users/UserSelector.vue';
import {
  accessibleText,
  pickerOption,
  pickerOptions,
  pickerTrigger,
} from '@/components/__tests__/picker';

enableAutoUnmount(afterEach);

const users = [
  { id: 'u1', username: 'sousuke', avatar_url: null },
  { id: 'u2', username: 'yupix', avatar_url: null },
  { id: 'u3', username: 'kousuke', avatar_url: null },
];

function mountSelector(props: Record<string, unknown> = {}) {
  const wrapper = mount(UserSelector, {
    props: { label: '担当者', users, modelValue: [], ...props },
    attachTo: document.body,
  });
  const trigger = () => pickerTrigger(wrapper.element, '担当者');
  const names = () => pickerOptions(trigger()).map(accessibleText);
  const option = (name: string) => pickerOption(trigger(), name);
  return { wrapper, trigger, names, option };
}

describe('UserSelector の並びと絞り込み', () => {
  it('自分を先頭に「（自分）」と添え、その他は名の順に並べる', () => {
    const { names } = mountSelector({ currentUserId: 'u2' });

    expect(names()).toEqual(['yupix（自分）', 'kousuke', 'sousuke']);
  });

  it('打った字を名に含む者だけに絞り込む', async () => {
    const { wrapper, names } = mountSelector();

    await wrapper.get('input[type="search"]').setValue('suke');

    expect(names()).toEqual(['kousuke', 'sousuke']);
  });

  it('BOT は名を変えずに「BOT アカウント」を添え、人には添えない', () => {
    const { names, option } = mountSelector({
      users: [
        ...users,
        { id: 'b1', username: 'release-bot', account_kind: 'bot', avatar_url: null },
      ],
    });

    expect(names()).toContain('release-bot');
    expect(option('release-bot').textContent).toContain('BOT アカウント');
    expect(option('yupix').textContent).not.toContain('BOT');
  });

  it('当たらぬ時は呼び手の文を出す', async () => {
    const { wrapper, names } = mountSelector({ noMatchText: '一致する利用者がいません' });

    await wrapper.get('input[type="search"]').setValue('zzz');

    expect(names()).toEqual([]);
    expect(wrapper.text()).toContain('一致する利用者がいません');
  });
});

describe('UserSelector の選ぶ・外す', () => {
  it('複数: 押すと付け、もう一度押すと外す（toggle と update:modelValue）', async () => {
    const { wrapper, option } = mountSelector({ modelValue: ['u2'] });

    option('sousuke').click();
    await flushPromises();
    expect(wrapper.emitted('toggle')?.[0]).toEqual(['u1', true]);
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([['u2', 'u1']]);
    expect(option('sousuke').getAttribute('aria-selected')).toBe('true');

    option('yupix').click();
    await flushPromises();
    expect(wrapper.emitted('toggle')?.[1]).toEqual(['u2', false]);
    expect(wrapper.emitted('update:modelValue')?.[1]).toEqual([['u1']]);
  });

  it('単数: 別の者を押すと置き換える', async () => {
    const { wrapper, option } = mountSelector({ multiple: false, modelValue: ['u1'] });

    option('yupix').click();
    await flushPromises();

    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([['u2']]);
    expect(wrapper.emitted('toggle')).toEqual([
      ['u2', true],
      ['u1', false],
    ]);
  });

  it('単数は選ぶと閉じ、複数は開いたまま続けて選べる', async () => {
    const single = mountSelector({ multiple: false });
    single.trigger().click();
    await flushPromises();
    expect(single.trigger().getAttribute('aria-expanded')).toBe('true');
    single.option('yupix').click();
    await flushPromises();
    expect(single.trigger().getAttribute('aria-expanded')).toBe('false');
    single.wrapper.unmount();

    const multiple = mountSelector();
    multiple.trigger().click();
    await flushPromises();
    multiple.option('yupix').click();
    await flushPromises();
    expect(multiple.trigger().getAttribute('aria-expanded')).toBe('true');
  });

  // Picker は候補に無い id を落として返す。そのまま渡すと、外れた担当者まで外してしまう
  it('複数: 候補に無い選択（プロジェクトを外れた担当者など）は外さずに残す', async () => {
    const { wrapper, option } = mountSelector({ modelValue: ['gone'] });

    option('yupix').click();
    await flushPromises();

    expect(wrapper.emitted('toggle')).toEqual([['u2', true]]);
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([['gone', 'u2']]);
  });

  it('保存中（disabled）: 閉じていればトリガーを止め、開いていれば開いたまま選択を受けない', async () => {
    const closed = mountSelector({ disabled: true });
    expect(closed.trigger().disabled).toBe(true);
    closed.wrapper.unmount();

    const { wrapper, trigger, option } = mountSelector();
    trigger().click();
    await vi.waitFor(() => expect(wrapper.element.contains(document.activeElement)).toBe(true));
    await wrapper.setProps({ disabled: true });

    option('yupix').click();
    await flushPromises();

    expect(trigger().getAttribute('aria-expanded')).toBe('true');
    expect(wrapper.emitted('toggle')).toBeUndefined();
    expect(option('yupix').getAttribute('aria-selected')).toBe('false');
  });
});

describe('UserSelector の部署・チーム', () => {
  const grouped = [
    { id: 'u1', username: 'sousuke', department: '開発', team: '基盤' },
    { id: 'u2', username: 'yupix', department: '開発', team: null },
    { id: 'u3', username: 'kousuke', department: '営業', team: '基盤' },
    { id: 'u4', username: 'alice' },
  ];
  const DEV = '部署「開発」の全員';
  const CORE = 'チーム「基盤」の全員';

  it('重複を除いた行を候補の後ろに出す（部署、チームの順）', () => {
    const { names } = mountSelector({ users: grouped, currentUserId: 'u2' });

    expect(names()).toEqual([
      'yupix（自分）',
      'alice',
      'kousuke',
      'sousuke',
      '部署「営業」の全員',
      DEV,
      CORE,
    ]);
  });

  it('行を選ぶと属する全員が選択に加わる', async () => {
    const { wrapper, option } = mountSelector({ users: grouped });

    option(DEV).click();
    await flushPromises();

    expect(wrapper.emitted('update:modelValue')).toEqual([[['u1', 'u2']]]);
    expect(wrapper.emitted('toggle')).toEqual([
      ['u1', true],
      ['u2', true],
    ]);
    expect(option('sousuke').getAttribute('aria-selected')).toBe('true');
    expect(option('yupix').getAttribute('aria-selected')).toBe('true');
    expect(option('alice').getAttribute('aria-selected')).toBe('false');
  });

  it('選択済みの人は重ねず、候補に無い選択は残す', async () => {
    const { wrapper, option } = mountSelector({ users: grouped, modelValue: ['gone', 'u1'] });

    option(CORE).click();
    await flushPromises();

    expect(wrapper.emitted('update:modelValue')).toEqual([[['gone', 'u1', 'u3']]]);
    expect(wrapper.emitted('toggle')).toEqual([['u3', true]]);
  });

  it('行そのものは選択に残らず、続けて別の行も一人ずつの解除もできる', async () => {
    const { wrapper, option } = mountSelector({ users: grouped });

    option(DEV).click();
    await flushPromises();
    expect(option(DEV).getAttribute('aria-selected')).toBe('false');

    option(CORE).click();
    await flushPromises();
    expect(wrapper.emitted('update:modelValue')?.[1]).toEqual([['u1', 'u2', 'u3']]);

    option('sousuke').click();
    await flushPromises();
    expect(wrapper.emitted('update:modelValue')?.[2]).toEqual([['u3', 'u2']]);
    expect(wrapper.emitted('toggle')?.at(-1)).toEqual(['u1', false]);
  });

  it('部署・チームの名で行を探せる', async () => {
    const { wrapper, names } = mountSelector({ users: grouped });

    await wrapper.get('input[type="search"]').setValue('開発');

    expect(names()).toEqual([DEV]);
  });

  it('部署・チームが無い候補だけなら行を出さない', () => {
    const { names } = mountSelector({
      users: [
        { id: 'u1', username: 'sousuke', department: null, team: null },
        { id: 'u2', username: 'yupix' },
      ],
    });

    expect(names()).toEqual(['sousuke', 'yupix']);
  });

  it('単数では行を出さない', () => {
    const { names } = mountSelector({ users: grouped, multiple: false });

    expect(names()).toEqual(['alice', 'kousuke', 'sousuke', 'yupix']);
  });
});

describe('UserSelector の状態', () => {
  it('取得中は読み込み中を出し、「いません」と混ぜない', () => {
    const { wrapper } = mountSelector({
      users: [],
      state: { loading: true },
      emptyText: '選べる利用者がいません',
    });

    expect(wrapper.text()).toContain('読み込み中…');
    expect(wrapper.text()).not.toContain('選べる利用者がいません');
  });

  it('失敗は失敗として出し、再試行できる', async () => {
    const onRetry = vi.fn();
    const { wrapper } = mountSelector({ users: [], state: { error: true, onRetry } });

    expect(wrapper.text()).toContain('候補を読み込めませんでした');
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '再試行')!
      .trigger('click');
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('取得できて 0 人のときだけ「いません」を出す（文は呼び手が決められる）', () => {
    const { wrapper } = mountSelector({
      users: [],
      emptyText: '担当者に指定できる利用者がいません',
    });

    expect(wrapper.text()).toContain('担当者に指定できる利用者がいません');
  });
});
