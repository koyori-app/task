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
