import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { h } from 'vue';
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

// ラベルから入力欄を引く。ブラウザと同じく、for の id を文書の中で最初に持つ要素が結び付く
const input = (label: string, root: ParentNode = document) => {
  const element = Array.from(root.querySelectorAll('label')).find(
    (candidate) => candidate.textContent?.trim() === label,
  )!;
  return document.getElementById(element.htmlFor) as HTMLInputElement;
};
// 背景のクリックは dialog 要素そのものへの pointerdown として届く
const backdrop = () => document.querySelector<HTMLDialogElement>('dialog')!;

function updateInput(label: string, value: string) {
  const element = input(label);
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

// テナント未所属のホームでは、ヘッダーの閉じたダイアログが本文のダイアログより先に DOM にある。
// useId はアプリごとに数えるので、実際の画面と同じく 1 つのアプリに並べ、本文側の dialog を返す
async function mountBehindHeaderInstance() {
  mount(
    {
      render: () => [h(CreateTenantDialog, { open: false }), h(CreateTenantDialog, { open: true })],
    },
    { attachTo: document.body },
  );
  await flushPromises();
  return document.querySelectorAll('dialog')[1];
}

async function expectFormResetAfterReopen(
  wrapper: VueWrapper,
  reopenName: string,
  expectedSlug: string,
) {
  await wrapper.setProps({ open: true });
  await flushPromises();
  expect(input('名前').value).toBe('');
  expect(input('表示ID').value).toBe('');

  updateInput('名前', reopenName);
  await flushPromises();
  expect(input('表示ID').value).toBe(expectedSlug);
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

    updateInput('名前', '  New Tenant  ');
    updateInput('説明（任意）', '  Description  ');
    updateInput('アイコンURL（任意）', '  https://example.com/icon.png  ');
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
    expect(assign).toHaveBeenCalledWith('/new-tenant');
  });

  it('rejects missing and whitespace-only names before submission', async () => {
    mountDialog();
    await flushPromises();

    submit();
    await flushPromises();
    expect(document.body.textContent).toContain('名前は必須です');

    updateInput('名前', '   ');
    submit();
    await flushPromises();

    expect(document.body.textContent).toContain('名前は必須です');
    expect(apiClient.POST).not.toHaveBeenCalled();
  });

  it.each([
    {
      label: 'cancellation',
      close: async (wrapper: VueWrapper) => {
        const cancel = Array.from(document.querySelectorAll('button')).find(
          (button) => button.textContent === 'キャンセル',
        )!;
        cancel.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        await flushPromises();
        expect(wrapper.emitted('update:open')).toContainEqual([false]);
      },
      initialName: 'First Tenant',
      customDisplayId: 'custom-id',
      ignoredSlugName: 'Ignored Slug',
      reopenName: 'Second Tenant',
      expectedSlug: 'second-tenant',
    },
    {
      label: 'overlay close',
      close: async (wrapper: VueWrapper) => {
        backdrop().dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await flushPromises();
        expect(wrapper.emitted('update:open')).toContainEqual([false]);
      },
      initialName: 'Overlay Tenant',
      customDisplayId: 'overlay-custom',
      ignoredSlugName: 'After Overlay',
      reopenName: 'After Overlay',
      expectedSlug: 'after-overlay',
    },
  ])(
    'resets form values and manual display-id state after $label',
    async ({ close, initialName, customDisplayId, ignoredSlugName, reopenName, expectedSlug }) => {
      const wrapper = mountDialog();
      await flushPromises();

      updateInput('名前', initialName);
      await flushPromises();
      expect(input('表示ID').value).toBe(initialName.toLowerCase().replace(/\s+/g, '-'));
      updateInput('表示ID', customDisplayId);
      await flushPromises();
      updateInput('名前', ignoredSlugName);
      await flushPromises();
      expect(input('表示ID').value).toBe(customDisplayId);

      await close(wrapper);
      await expectFormResetAfterReopen(wrapper, reopenName, expectedSlug);
    },
  );

  it('shows the duplicate display-id message returned for a 409 response', async () => {
    vi.mocked(apiClient.POST).mockResolvedValue({
      error: { message: 'conflict' },
      response: new Response(null, { status: 409 }),
    });
    mountDialog();
    await flushPromises();

    updateInput('名前', 'Existing Tenant');
    submit();
    await flushPromises();

    expect(document.querySelector('[role="alert"]')?.textContent).toContain(
      'この表示IDはすでに使用されています',
    );
  });

  it('submits its own form when another instance precedes it in the document', async () => {
    vi.mocked(apiClient.POST).mockResolvedValue({
      data: createdTenant,
      response: new Response(null, { status: 201 }),
    });
    vi.spyOn(window.location, 'assign').mockImplementation(() => undefined);
    const dialog = await mountBehindHeaderInstance();
    // 最初の入力欄が「名前」
    const name = dialog.querySelector<HTMLInputElement>('input')!;
    name.value = 'New Tenant';
    name.dispatchEvent(new Event('input', { bubbles: true }));
    await flushPromises();
    dialog.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flushPromises();

    expect(apiClient.POST).toHaveBeenCalledWith('/v1/tenants', {
      body: expect.objectContaining({ name: 'New Tenant', display_id: 'new-tenant' }),
    });
  });

  it('links labels and descriptions to its own inputs when another instance precedes it', async () => {
    const dialog = await mountBehindHeaderInstance();

    for (const label of ['名前', '表示ID', '説明（任意）', 'アイコンURL（任意）']) {
      expect(dialog.contains(input(label, dialog)), label).toBe(true);
    }
    const describedBy = input('表示ID', dialog).getAttribute('aria-describedby')!.split(' ');
    for (const id of describedBy) {
      expect(dialog.contains(document.getElementById(id)), id).toBe(true);
    }
  });

  it('auto-generates a slug until the display id is manually edited', async () => {
    mountDialog();
    await flushPromises();

    updateInput('名前', ' Hello, WORLD! ');
    await flushPromises();
    expect(input('表示ID').value).toBe('hello-world');

    updateInput('表示ID', 'MY-CUSTOM-ID');
    await flushPromises();
    expect(input('表示ID').value).toBe('my-custom-id');
    updateInput('名前', 'Changed Name');
    await flushPromises();
    expect(input('表示ID').value).toBe('my-custom-id');
  });
});
