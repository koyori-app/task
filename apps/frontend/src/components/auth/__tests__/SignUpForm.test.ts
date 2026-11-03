import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import SignUpForm from '../SignUpForm.vue';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

describe('SignUpForm', () => {
  it('中身のない画像の面を出さない', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Response.json({ providers: [] })),
    );
    const wrapper = mount(SignUpForm, {
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();

    expect(wrapper.find('form').exists()).toBe(true);
    expect(wrapper.find('img').exists()).toBe(false);
  });
});
