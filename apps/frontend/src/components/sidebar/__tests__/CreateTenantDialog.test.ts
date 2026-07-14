import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { apiClient } from '@/lib/api';
import CreateTenantDialog from '../CreateTenantDialog.vue';

vi.mock('@/lib/api', () => ({
  apiClient: { GET: vi.fn(), POST: vi.fn() },
}));

const createdTenant = {
  id: 'tenant-2',
  display_id: 'new-tenant',
  name: 'New Tenant',
  description: '',
  icon_url: '',
  owner_id: 'owner-1',
  require_2fa: false,
};

const input = (id: string) => document.querySelector<HTMLInputElement>(`#${id}`)!;

function updateInput(id: string, value: string) {
  const element = input(id);
  element.value = value;
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

function submit() {
  document
    .querySelector<HTMLFormElement>('form')!
    .dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

function mountDialog() {
  let wrapper: VueWrapper;
  wrapper = mount(CreateTenantDialog, {
    props: {
      open: true,
      'onUpdate:open': (open: boolean) => wrapper.setProps({ open }),
    },
    attachTo: document.body,
  });
  return wrapper;
}

describe('CreateTenantDialog', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    setActivePinia(createPinia());
    vi.mocked(apiClient.POST).mockReset();
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.restoreAllMocks();
  });

  it('creates a tenant from valid input, closes the dialog, and navigates to it', async () => {
    vi.mocked(apiClient.POST).mockResolvedValue({
      data: createdTenant,
      response: new Response(null, { status: 201 }),
    });
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => undefined);
    const wrapper = mountDialog();
    await flushPromises();

    updateInput('name', '  New Tenant  ');
    updateInput('description', '  Description  ');
    updateInput('icon_url', '  https://example.com/icon.png  ');
    submit();
    await flushPromises();

    expect(apiClient.POST).toHaveBeenCalledWith('/v1/tenants', {
      body: {
        name: 'New Tenant',
        display_id: 'new-tenant',
        description: 'Description',
        icon_url: 'https://example.com/icon.png',
      },
    });
    expect(wrapper.emitted('update:open')).toContainEqual([false]);
    expect(assign).toHaveBeenCalledWith('/new-tenant/my-tasks');
  });

  it('rejects missing and whitespace-only names before submission', async () => {
    mountDialog();
    await flushPromises();

    submit();
    await flushPromises();
    expect(document.body.textContent).toContain('名前は必須です');

    updateInput('name', '   ');
    submit();
    await flushPromises();

    expect(document.body.textContent).toContain('名前は必須です');
    expect(apiClient.POST).not.toHaveBeenCalled();
  });

  it('resets form values and manual display-id state after cancellation', async () => {
    const wrapper = mountDialog();
    await flushPromises();

    updateInput('name', 'First Tenant');
    await flushPromises();
    expect(input('display_id').value).toBe('first-tenant');
    updateInput('display_id', 'custom-id');
    await flushPromises();
    updateInput('name', 'Ignored Slug');
    await flushPromises();
    expect(input('display_id').value).toBe('custom-id');

    const cancel = Array.from(document.querySelectorAll('button')).find(
      (button) => button.textContent === 'キャンセル',
    )!;
    cancel.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await flushPromises();
    expect(wrapper.emitted('update:open')).toContainEqual([false]);

    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(input('name').value).toBe('');
    expect(input('display_id').value).toBe('');

    updateInput('name', 'Second Tenant');
    await flushPromises();
    expect(input('display_id').value).toBe('second-tenant');
  });

  it('resets form values and manual display-id state after an overlay close', async () => {
    const wrapper = mountDialog();
    await flushPromises();

    updateInput('name', 'Overlay Tenant');
    await flushPromises();
    updateInput('display_id', 'overlay-custom');
    await flushPromises();

    const overlay = document.querySelector<HTMLElement>('.bg-black\\/50')!;
    overlay.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    overlay.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await flushPromises();
    expect(wrapper.emitted('update:open')).toContainEqual([false]);

    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(input('name').value).toBe('');
    expect(input('display_id').value).toBe('');

    updateInput('name', 'After Overlay');
    await flushPromises();
    expect(input('display_id').value).toBe('after-overlay');
  });

  it('shows the duplicate display-id message returned for a 409 response', async () => {
    vi.mocked(apiClient.POST).mockResolvedValue({
      error: { message: 'conflict' },
      response: new Response(null, { status: 409 }),
    });
    mountDialog();
    await flushPromises();

    updateInput('name', 'Existing Tenant');
    submit();
    await flushPromises();

    expect(document.querySelector('[role="alert"]')?.textContent).toContain(
      'この表示IDはすでに使用されています',
    );
  });

  it('auto-generates a slug until the display id is manually edited', async () => {
    mountDialog();
    await flushPromises();

    updateInput('name', ' Hello, WORLD! ');
    await flushPromises();
    expect(input('display_id').value).toBe('hello-world');

    updateInput('display_id', 'MY-CUSTOM-ID');
    await flushPromises();
    expect(input('display_id').value).toBe('my-custom-id');
    updateInput('name', 'Changed Name');
    await flushPromises();
    expect(input('display_id').value).toBe('my-custom-id');
  });
});
