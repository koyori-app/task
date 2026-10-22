import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

// 器の中身は自前で API を叩くので、下書きの入力欄だけの代役にする
vi.mock('@/components/tasks/TaskDetailPane.vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    default: defineComponent({
      name: 'TaskDetailPane',
      setup: () => () => h('textarea', { id: 'draft' }),
    }),
  };
});

import TaskDetailOverlay from '../TaskDetailOverlay.vue';

enableAutoUnmount(afterEach);

function mountOverlay() {
  return mount(TaskDetailOverlay, {
    props: { open: true, tenantDisplayId: 'acme', projectKey: 'ENG', taskId: 'ENG-1' },
    attachTo: document.body,
  });
}

function draft() {
  return document.querySelector<HTMLTextAreaElement>('#draft')!;
}

function pressEscapeOn(target: Element) {
  const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

describe('TaskDetailOverlay', () => {
  it('下書きのある入力欄の Esc は keydown で止め、空なら止めない', () => {
    mountOverlay();

    expect(pressEscapeOn(draft()).defaultPrevented).toBe(false);
    draft().value = '書きかけ';
    expect(pressEscapeOn(draft()).defaultPrevented).toBe(true);
  });

  it('Esc（dialog の cancel）で閉じる', () => {
    const wrapper = mountOverlay();

    wrapper.get('dialog').element.dispatchEvent(new Event('cancel', { cancelable: true }));

    expect(wrapper.emitted('update:open')).toEqual([[false]]);
  });

  it('背景の pointerdown は、下書きにフォーカスがある間は閉じず、無ければ閉じる', () => {
    const wrapper = mountOverlay();
    const backdrop = wrapper.get('dialog').element;
    draft().value = '書きかけ';
    draft().focus();

    backdrop.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    expect(wrapper.emitted('update:open')).toBeUndefined();

    draft().blur();
    backdrop.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    expect(wrapper.emitted('update:open')).toEqual([[false]]);
  });
});
