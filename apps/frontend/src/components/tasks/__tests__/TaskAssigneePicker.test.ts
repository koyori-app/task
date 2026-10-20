import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import { useListLayout } from '@/components/users/__tests__/list-layout';

enableAutoUnmount(afterEach);
useListLayout();

const members = [
  { id: 'user-1', username: 'yupix', avatar_url: null },
  { id: 'user-2', username: 'sousuke', avatar_url: null },
];

/**
 * 候補の窓はトリガーを押さないと描画されないので、
 * Popover の部品を素の div に差し替えて中を読む（中身の UserSelectorList は本物）。
 */
function mountPicker(props: Record<string, unknown>) {
  return mount(TaskAssigneePicker, {
    props: { members: [], selected: [], ...props },
    global: {
      stubs: {
        PopoverRoot: { template: '<div><slot /></div>' },
        PopoverTrigger: { template: '<div><slot /></div>' },
        PopoverPortal: { template: '<div><slot /></div>' },
        PopoverContent: { template: '<div><slot /></div>' },
      },
    },
  });
}

// 取得中・失敗を「候補が 0 人」と混ぜると、権限や通信で候補が取れていないのに
// 「いません」と出て、担当者を触れない理由が分からなくなる
describe('TaskAssigneePicker の候補の取得状態', () => {
  it('取得中は読み込み中を出す', () => {
    const wrapper = mountPicker({ membersState: { loading: true } });

    expect(wrapper.text()).toContain('読み込み中…');
    expect(wrapper.text()).not.toContain('担当者に指定できる利用者がいません');
  });

  it('失敗は失敗として出し、再試行できる', async () => {
    const onRetry = vi.fn();
    const wrapper = mountPicker({ membersState: { error: true, onRetry } });

    expect(wrapper.text()).toContain('候補を読み込めませんでした');
    expect(wrapper.text()).not.toContain('担当者に指定できる利用者がいません');

    const button = wrapper.findAll('button').find((b) => b.text() === '再試行');
    expect(button).toBeDefined();
    await button!.trigger('click');
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('取得できて 0 人のときだけ「いません」を出す', () => {
    const wrapper = mountPicker({ membersState: { loading: false, error: false } });

    expect(wrapper.text()).toContain('担当者に指定できる利用者がいません');
  });

  it('候補があれば絞り込める', async () => {
    const wrapper = mountPicker({ members });
    // 候補の一覧は仮想化しており、描くのは一覧の丈を測った次の tick
    await flushPromises();

    expect(wrapper.text()).toContain('yupix');
    expect(wrapper.text()).toContain('sousuke');

    await wrapper.get('input[aria-label="メンバーを検索"]').setValue('sou');

    expect(wrapper.text()).not.toContain('yupix');
    expect(wrapper.text()).toContain('sousuke');
  });
});

// 担当者は一人ずつ保存してから再取得するので、selected は選んでから遅れて変わる。一覧は選んだ時に
// 先に反映し、遅れて届いた同じ中身では差し替えない（差し替えると reka が一覧を選んだ者へ送り直す）
describe('TaskAssigneePicker の保存が遅れて反映される時', () => {
  const optionOf = (wrapper: ReturnType<typeof mountPicker>, id: string) =>
    wrapper.get(`[role="option"][data-user-id="${id}"]`);

  it('選んだ時に一覧へ先に反映し、遅れて届いた同じ中身でも選んだままにする', async () => {
    const wrapper = mountPicker({ members, selected: [members[0]] });
    await flushPromises();

    await optionOf(wrapper, 'user-2').trigger('click');
    expect(wrapper.emitted('toggle')).toEqual([['user-2', true]]);
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');

    await wrapper.setProps({ selected: [members[0], members[1]] });
    await flushPromises();
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');
    expect(optionOf(wrapper, 'user-1').attributes('aria-selected')).toBe('true');
  });

  it('保存中に届いた値では突き合わせず、保存が終わってから突き合わせる', async () => {
    const wrapper = mountPicker({ members, selected: [members[0]] });
    await flushPromises();

    await optionOf(wrapper, 'user-2').trigger('click');
    // 保存中（disabled）に、途中の値（まだ足しておらぬ）が届く
    await wrapper.setProps({ disabled: true, selected: [{ ...members[0] }] });
    await flushPromises();
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');

    // 保存が終わり、足した値が届く
    await wrapper.setProps({ selected: [members[0], members[1]] });
    await wrapper.setProps({ disabled: false });
    await flushPromises();
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');
  });

  it('保存が失敗すれば、保存が終わった時に一覧も呼び手の選択へ戻る', async () => {
    const wrapper = mountPicker({ members, selected: [members[0]] });
    await flushPromises();

    await optionOf(wrapper, 'user-2').trigger('click');
    await wrapper.setProps({ disabled: true });
    await flushPromises();
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');

    // 失敗では再取得せず、呼び手の選択は変わらぬまま保存が終わる
    await wrapper.setProps({ disabled: false });
    await flushPromises();
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('false');
    expect(optionOf(wrapper, 'user-1').attributes('aria-selected')).toBe('true');
  });

  it('保存中でない時に呼び手の選択が違う中身で届けば、一覧も呼び手に従う', async () => {
    const wrapper = mountPicker({ members, selected: [members[0]] });
    await flushPromises();

    await optionOf(wrapper, 'user-2').trigger('click');
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('true');

    // 再取得で、保存の前と同じ中身の配列が届く
    await wrapper.setProps({ selected: [{ ...members[0] }] });
    await flushPromises();
    expect(optionOf(wrapper, 'user-1').attributes('aria-selected')).toBe('true');
    expect(optionOf(wrapper, 'user-2').attributes('aria-selected')).toBe('false');
  });
});
