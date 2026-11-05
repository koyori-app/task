import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import SignUpForm from '../SignUpForm.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  window.history.replaceState({}, '', '/');
});

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

  it('登録後のメール確認の案内からサインインへ戻るリンクは redirect_after を持ち越す', async () => {
    window.history.replaceState(
      {},
      '',
      '/signup?redirect_after=%2Finvitations%2Faccept%3Ftoken%3Dabc',
    );
    vi.stubGlobal(
      'fetch',
      vi.fn(async (req: Request | string) => {
        const url = typeof req === 'string' ? req : req.url;
        if (new URL(url, 'http://localhost').pathname.endsWith('/v1/auth/register')) {
          return new Response('', { status: 201 });
        }
        return Response.json({ providers: [] });
      }),
    );
    const wrapper = mount(SignUpForm, {
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    await wrapper.find('input[name="username"]').setValue('alice');
    await wrapper.find('input[name="email"]').setValue('alice@example.com');
    await wrapper.find('input[name="password"]').setValue('devpass12345');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    const back = wrapper.findAll('a').find((a) => a.text().includes('サインインページへ戻る'));
    expect(back!.attributes('href')).toBe(
      '/signin?redirect_after=%2Finvitations%2Faccept%3Ftoken%3Dabc',
    );
  });
});
