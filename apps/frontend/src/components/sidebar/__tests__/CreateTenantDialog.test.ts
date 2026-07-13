import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { apiClient } from '@/lib/api';
import CreateTenantDialog from '../CreateTenantDialog.vue';

vi.mock('@/lib/api', () => ({
  apiClient: { GET: vi.fn(), POST: vi.fn() },
}));

describe('CreateTenantDialog', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    setActivePinia(createPinia());
    vi.mocked(apiClient.POST).mockReset();
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('rejects a whitespace-only name with a field error before submission', async () => {
    mount(CreateTenantDialog, {
      props: { open: true },
      attachTo: document.body,
    });
    await flushPromises();

    const nameInput = document.querySelector<HTMLInputElement>('#name');
    const form = document.querySelector<HTMLFormElement>('form');
    expect(nameInput).not.toBeNull();
    expect(form).not.toBeNull();

    nameInput!.value = '   ';
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await flushPromises();

    expect(document.body.textContent).toContain('名前は必須です');
    expect(apiClient.POST).not.toHaveBeenCalled();
  });
});
