import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import TaskTableTitleCell from '../TaskTableTitleCell.vue';

enableAutoUnmount(afterEach);

function mountCell(narrow: boolean) {
  return mount(TaskTableTitleCell, {
    props: {
      tenantDisplayId: 'acme',
      projectKey: 'ENG',
      seqId: 42,
      title: 'タイトル',
      priority: 'High',
      narrow,
    },
  });
}

describe('TaskTableTitleCell', () => {
  // TASK-524: 狭い形はセル全体を的にし、包含ブロックは <td> ではなくセルの中の div
  it('narrow なら relative の div・広げたリンク・compact のコピーのボタンを出す', () => {
    const wrapper = mountCell(true);
    const link = wrapper.get('a[data-task-seq-key]');

    expect(wrapper.classes()).toContain('relative');
    expect(link.classes()).toEqual(expect.arrayContaining(['after:absolute', 'after:inset-0']));
    const copy = wrapper.get('[data-task-id-copy]');
    expect(copy.classes()).toEqual(expect.arrayContaining(['relative', 'z-10']));
    expect(copy.find('[data-task-id-number]').text()).toBe('-42');
  });

  it('対照: narrow でなければセルを的にせず、ID のボタンも出さない', () => {
    const wrapper = mountCell(false);

    expect(wrapper.classes()).not.toContain('relative');
    expect(wrapper.get('a[data-task-seq-key]').classes()).not.toContain('after:absolute');
    expect(wrapper.find('[data-task-id-copy]').exists()).toBe(false);
  });

  it('リンクを押すと select が出て、ID を押してもコピーだけで出ない', async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const wrapper = mountCell(true);

    await wrapper.get('[data-task-id-copy]').trigger('click');
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith('ENG-42');
    expect(wrapper.emitted('select')).toBeUndefined();

    await wrapper.get('a[data-task-seq-key]').trigger('click', { button: 0 });
    expect(wrapper.emitted('select')).toEqual([[42]]);
    delete (navigator as { clipboard?: unknown }).clipboard;
  });
});
