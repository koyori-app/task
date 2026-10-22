import { afterEach, describe, expect, it, vi } from 'vitest';
import { DOMWrapper, enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import CreateDocumentDialog from '../CreateDocumentDialog.vue';

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('CreateDocumentDialog', () => {
  it('作成で通信が切れたら、入力を残したままエラーを出す', async () => {
    vi.stubGlobal('fetch', async () => {
      throw new TypeError('Failed to fetch');
    });
    const wrapper = mount(CreateDocumentDialog, {
      props: { open: true, tenantId: 'tenant-1', folderId: 'root-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
      attachTo: document.body,
    });
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('input[name="title"]')!;
    await new DOMWrapper(input).setValue('設計メモ');
    await new DOMWrapper(document.body.querySelector('form')!).trigger('submit');
    await flushPromises();

    expect(document.body.querySelector('[role="alert"]')?.textContent?.trim()).toBe(
      'ドキュメントを作成できませんでした',
    );
    expect(input.value).toBe('設計メモ');
    expect(wrapper.emitted('created')).toBeUndefined();
  });
});
