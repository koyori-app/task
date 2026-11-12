import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';

import { installImeKeydownGuard } from '../ime-keydown-guard';

function press(target: Element, init: KeyboardEventInit & { keyCode?: number }) {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

let uninstall: () => void;

beforeEach(() => {
  uninstall = installImeKeydownGuard();
});

afterEach(() => {
  uninstall();
  document.body.innerHTML = '';
});

describe('installImeKeydownGuard', () => {
  it.each([
    ['isComposing', { isComposing: true }],
    ['Safari の keyCode 229', { keyCode: 229 }],
  ])('変換確定の Enter（%s）でサブタスクを作らず、通常の Enter では作る', async (_, ime) => {
    const onCreate = vi.fn().mockResolvedValue(true);
    const wrapper = mount(TaskSubtaskComposer, {
      attachTo: document.body,
      props: { onCreate },
    });
    const input = wrapper.get('input');
    await input.setValue('にほんご');

    press(input.element, { key: 'Enter', ...ime });
    await nextTick();
    expect(onCreate).not.toHaveBeenCalled();

    press(input.element, { key: 'Enter' });
    await nextTick();
    expect(onCreate).toHaveBeenCalledWith('にほんご');
    wrapper.unmount();
  });

  it('変換中の Esc はアプリへ届かず、dialog の cancel も起こさせない', () => {
    document.body.innerHTML = '<input id="field" />';
    const field = document.getElementById('field')!;
    const reached = vi.fn();
    field.addEventListener('keydown', reached);

    const event = press(field, { key: 'Escape', keyCode: 229 });

    expect(reached).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it('変換確定の Enter は既定動作を止めない', () => {
    document.body.innerHTML = '<input id="field" />';

    const event = press(document.getElementById('field')!, { key: 'Enter', keyCode: 229 });

    expect(event.defaultPrevented).toBe(false);
  });

  it('CodeMirror の中では変換中の keydown も止めない', () => {
    document.body.innerHTML = '<div class="cm-editor"><div id="content"></div></div>';
    const content = document.getElementById('content')!;
    const reached = vi.fn();
    content.addEventListener('keydown', reached);

    press(content, { key: 'Enter', keyCode: 229 });

    expect(reached).toHaveBeenCalledOnce();
  });
});
