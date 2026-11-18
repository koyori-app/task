import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TaskFilterBar from '../TaskFilterBar.vue';
function setup(modelValue = '') {
  return mount(TaskFilterBar, {
    props: {
      modelValue,
      statuses: [
        { id: '1', name: 'Todo' },
        { id: '2', name: 'Done' },
      ],
      labels: [{ id: 'bug', name: '保留' }],
      members: [],
    },
  });
}
describe('filter buttons edit the expression', () => {
  it('adds multiple statuses as OR values', async () => {
    const w = setup();
    const item = (text: string) =>
      w.findAll('[role="menuitemcheckbox"]').find((e) => e.text() === text)!;
    await item('Todo').trigger('click');
    await w.setProps({ modelValue: w.emitted('update:modelValue')!.at(-1)![0] as string });
    await item('Done').trigger('click');
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['status:Todo,Done']);
    w.unmount();
  });
  it('shows exclusions and gray active buttons', () => {
    const w = setup('-label:保留');
    expect(w.text()).toContain('ラベル: 保留以外');
    expect(w.findAll('*').some((node) => node.classes().includes('[&>button]:bg-muted'))).toBe(
      true,
    );
    w.unmount();
  });
  it('uses one due value and separate archived/subtask flags', async () => {
    const w = setup('is:archived');
    await w
      .findAll('[role="menuitemradio"]')
      .find((e) => e.text() === '7 日以内')!
      .trigger('click');
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['is:archived due:<7d']);
    await w
      .findAll('[role="menuitemcheckbox"]')
      .find((e) => e.text() === 'サブタスクだけ')!
      .trigger('click');
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['is:archived is:subtask']);
    w.unmount();
  });
});

it('matches names without case and displays priority labels', async () => {
  const w = setup('status:done priority:HIGH');
  expect(w.text()).toContain('ステータス: Done');
  expect(w.text()).toContain('優先度: 高');
  await w
    .findAll('[role="menuitemcheckbox"]')
    .find((e) => e.text() === 'Done')!
    .trigger('click');
  expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['priority:HIGH']);
  expect(
    w
      .findAll('[role="menuitemradio"]')
      .find((e) => e.text() === '指定なし')!
      .attributes('aria-checked'),
  ).toBe('true');
  w.unmount();
});

describe('buttons unselect what they show as selected', () => {
  const click = (w: ReturnType<typeof setup>, text: string) =>
    w
      .findAll('[role="menuitemcheckbox"]')
      .find((e) => e.text() === text)!
      .trigger('click');
  it.each([
    ['author:@ME', '自分が作成', ''],
    ['in:COMMENT', 'コメントだけを探す', ''],
    ['is:ARCHIVED', 'アーカイブ済み', ''],
  ])('removes %s via the %s button', async (expr, text, expected) => {
    const w = setup(expr);
    await click(w, text);
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual([expected]);
    w.unmount();
  });
  it('does not treat author:me (without @) as myself, since the backend reads it as a username', async () => {
    const w = setup('author:me');
    const mine = w.findAll('[role="menuitemcheckbox"]').find((e) => e.text() === '自分が作成')!;
    expect(mine.attributes('aria-checked')).toBe('false');
    w.unmount();
  });
  it('removes the value from the second term of a repeated key', async () => {
    const w = setup('status:Todo status:Done');
    await click(w, 'Done');
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['status:Todo']);
    w.unmount();
  });
  it('keeps the other values of a shared flag term', async () => {
    const w = setup('no:assignee,label');
    await click(w, 'ラベルなし');
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual(['no:assignee']);
    w.unmount();
  });
});
