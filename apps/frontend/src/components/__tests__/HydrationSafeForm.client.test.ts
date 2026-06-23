import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ref, nextTick, h } from 'vue';
import { mount } from '@vue/test-utils';

const isHydrated = ref(false);

vi.mock('@/composables/useHydrated', () => ({
  useHydrated: () => isHydrated,
}));

import HydrationSafeForm from '../HydrationSafeForm.vue';

describe('HydrationSafeForm client behavior', () => {
  beforeEach(() => {
    isHydrated.value = false;
  });

  it('blocks pre-hydration submit and emits after hydration', async () => {
    const wrapper = mount(HydrationSafeForm, {
      slots: {
        default: () => h('button', { type: 'submit' }, 'Send'),
      },
      attachTo: document.body,
    });

    const form = wrapper.get('form');

    expect(form.element.getAttribute('onsubmit')).toBe('return false;');

    await form.trigger('submit');
    expect(wrapper.emitted('submit')).toBeUndefined();

    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    });
    form.element.dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(true);

    isHydrated.value = true;
    await nextTick();

    expect(form.element.getAttribute('onsubmit')).toBeNull();

    await form.trigger('submit');
    expect(wrapper.emitted('submit')).toHaveLength(1);

    wrapper.unmount();
  });
});
