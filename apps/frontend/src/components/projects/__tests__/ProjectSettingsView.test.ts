import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const { updateMutateAsync, deleteMutateAsync, navigateMock } = vi.hoisted(() => ({
  updateMutateAsync: vi.fn(),
  deleteMutateAsync: vi.fn(),
  navigateMock: vi.fn(),
}));

vi.mock('vike/client/router', () => ({
  navigate: navigateMock,
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string) => ({
        mutateAsync: method === 'put' ? updateMutateAsync : deleteMutateAsync,
        isPending: { value: false },
      })),
    },
  };
});

import ProjectSettingsView from '../ProjectSettingsView.vue';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleProject: ProjectResponse = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: 'Shared project',
  key: 'ALPHA',
  is_personal: false,
  inherit_tenant_members: true,
  icon_emoji: '🎨',
  icon_url: null,
  personal_owner_id: null,
};

function mountView(options: { searchSection?: string; canManage?: boolean } = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ProjectSettingsView, {
    props: {
      tenantId: TENANT_UUID,
      tenantSlug: 'acme',
      project: sampleProject,
      canManage: options.canManage ?? true,
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient }]],
      provide:
        options.searchSection !== undefined
          ? {
              'vike-vue:usePageContext': {
                urlParsed: { search: { section: options.searchSection } },
              },
            }
          : {},
    },
    attachTo: document.body,
  });
}

function input(id: string) {
  const el = document.body.querySelector<HTMLInputElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function formEl() {
  const el = document.body.querySelector('form');
  if (!el) throw new Error('form not found');
  return new DOMWrapper(el);
}

function clickBodyButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

describe('ProjectSettingsView', () => {
  beforeEach(() => {
    updateMutateAsync.mockReset();
    deleteMutateAsync.mockReset();
    navigateMock.mockReset();
    document.body.innerHTML = '';
  });

  it('一般セクション: 既存値をプリフィルし、保存で PUT を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...sampleProject, name: 'Renamed' });
    mountView();
    await flushPromises();

    expect((input('name').element as HTMLInputElement).value).toBe('Team Alpha');

    await input('name').setValue('Renamed');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
      body: { name: 'Renamed', description: 'Shared project' },
    });
    expect(document.body.textContent).toContain('変更を保存しました');
  });

  /**
   * プロジェクトが差し替わったら一般タブの入力値も追従する。
   *
   * vike-vue はサイドバーからのプロジェクト切り替えでこの画面を作り直さない。
   * 一般タブの値だけは props から一度コピーして持つので、追従しないと A の名前と
   * 説明を表示したまま B へ保存できてしまう。親の `:key` でも防いでいるが、
   * それが外れたときにこの一番重い経路が静かに壊れないよう、ここでも固定する。
   */
  it('project が差し替わったら入力値とアイコンが追従する', async () => {
    const other: ProjectResponse = {
      ...sampleProject,
      id: '00000000-0000-4000-8000-000000000020',
      name: 'Team Beta',
      description: 'Another project',
      key: 'BETA',
      icon_emoji: '🚀',
    };
    updateMutateAsync.mockResolvedValue(other);
    const wrapper = mountView();
    await flushPromises();
    expect((input('name').element as HTMLInputElement).value).toBe('Team Alpha');

    await wrapper.setProps({ project: other });
    await flushPromises();

    expect((input('name').element as HTMLInputElement).value).toBe('Team Beta');
    const description = document.body.querySelector<HTMLTextAreaElement>('#description');
    expect(description?.value).toBe('Another project');

    // 保存先も表示中のプロジェクトになる（A の値を B へ書かない）
    await formEl().trigger('submit');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: other.id } },
      body: { name: 'Team Beta', description: 'Another project' },
    });
  });

  it('アイコンを外して保存すると clear_icon_emoji を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...sampleProject, icon_emoji: null });
    mountView();
    await flushPromises();

    // アイコンの選択は開閉の無い格子。「アイコンなし」はその下にある
    const clearButton = [...document.body.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'アイコンなし',
    );
    expect(clearButton).toBeTruthy();
    clearButton!.click();
    await flushPromises();

    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
      body: { name: 'Team Alpha', description: 'Shared project', clear_icon_emoji: true },
    });
  });

  it('キーは読み取り専用で表示される', async () => {
    mountView();
    await flushPromises();

    const keyInput = document.body.querySelector<HTMLInputElement>('#project-key-readonly');
    expect(keyInput).not.toBeNull();
    expect(keyInput!.disabled).toBe(true);
    expect(keyInput!.value).toBe('ALPHA');
  });

  it('ナビで削除セクションに切り替え、確認ダイアログから DELETE できる', async () => {
    deleteMutateAsync.mockResolvedValue(undefined);
    mountView();
    await flushPromises();

    // ナビの「削除」でセクション切替
    const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
    const dangerNav = [...nav.querySelectorAll('button')].find((b) =>
      b.textContent?.includes('削除'),
    );
    expect(dangerNav).toBeTruthy();
    dangerNav!.click();
    await flushPromises();

    expect(document.body.textContent).toContain(
      'このプロジェクトとすべてのタスクを完全に削除します',
    );

    // Danger セクションの削除 → 確認ダイアログ → 削除する
    const sectionDelete = document.body.querySelector<HTMLButtonElement>(
      'button[aria-label="プロジェクトを削除"]',
    );
    expect(sectionDelete).not.toBeNull();
    sectionDelete!.click();
    await flushPromises();
    clickBodyButton('削除する');
    await flushPromises();

    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
    });
    expect(navigateMock).toHaveBeenCalledWith('/acme/my-tasks');
  });

  it('保存失敗時はエラーを表示する', async () => {
    updateMutateAsync.mockRejectedValue(new Error('forbidden'));
    mountView();
    await flushPromises();

    await formEl().trigger('submit');
    await flushPromises();

    expect(document.body.textContent).toContain('プロジェクトを更新できませんでした');
  });

  it('?section=integrations で連携セクションを初期表示する（GitHub callback の戻り先）', async () => {
    mountView({ searchSection: 'integrations' });
    await flushPromises();

    const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
    const activeNav = nav.querySelector('button[aria-current="true"]');
    expect(activeNav?.textContent).toContain('連携');
    // 一般セクションのフォームは表示されない
    expect(document.body.querySelector('#name')).toBeNull();
  });

  it('?section が未知の値なら一般セクションを表示する', async () => {
    mountView({ searchSection: 'unknown-section' });
    await flushPromises();

    const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
    const activeNav = nav.querySelector('button[aria-current="true"]');
    expect(activeNav?.textContent).toContain('一般');
  });

  it('403 は権限不足と明記する', async () => {
    updateMutateAsync.mockRejectedValue({ response: { status: 403 } });
    mountView();
    await flushPromises();

    await formEl().trigger('submit');
    await flushPromises();

    expect(document.body.querySelector('[role="alert"]')?.textContent).toContain(
      'プロジェクトの設定を変更できるのは、テナントのオーナーと Admin だけです。',
    );
    expect(document.body.textContent).not.toContain('プロジェクトを更新できませんでした');
  });

  describe('プロジェクトを管理できない人（canManage: false）', () => {
    function navLabels() {
      const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
      return [...nav.querySelectorAll('button')].map((b) => b.textContent?.trim());
    }

    it('一般は読み取りだけで、保存の入力も保存ボタンも出さず、案内文を出す', async () => {
      mountView({ canManage: false });
      await flushPromises();

      expect(document.body.querySelector('form')).toBeNull();
      expect(document.body.querySelector('#name')).toBeNull();
      expect(
        [...document.body.querySelectorAll('button')].some((b) =>
          b.textContent?.includes('変更を保存'),
        ),
      ).toBe(false);
      const main = document.body.textContent ?? '';
      expect(main).toContain('Team Alpha');
      expect(main).toContain('Shared project');
      expect(main).toContain('ALPHA');
      expect(document.body.querySelector('[role="status"]')?.textContent).toContain(
        '名前・説明・アイコンを変更できるのは、テナントのオーナーと Admin だけです。',
      );
    });

    it('ナビに「削除」が無く、削除の確認も出せない', async () => {
      mountView({ canManage: false });
      await flushPromises();

      expect(navLabels()).not.toContain('削除');
      expect(navLabels()).toContain('ワークフロー');
      expect(document.body.querySelector('button[aria-label="プロジェクトを削除"]')).toBeNull();
    });

    it('他の節（ワークフローなど）へは切り替えられる', async () => {
      mountView({ canManage: false });
      await flushPromises();

      const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
      const workflow = [...nav.querySelectorAll('button')].find((b) =>
        b.textContent?.includes('ワークフロー'),
      );
      workflow!.click();
      await flushPromises();

      expect(nav.querySelector('button[aria-current="true"]')?.textContent).toContain(
        'ワークフロー',
      );
    });

    it('?section=danger で来ても、削除ではなく一般を出す', async () => {
      mountView({ canManage: false, searchSection: 'danger' });
      await flushPromises();

      const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
      expect(nav.querySelector('button[aria-current="true"]')?.textContent).toContain('一般');
      expect(document.body.textContent).not.toContain(
        'このプロジェクトとすべてのタスクを完全に削除します',
      );
    });

    it('削除の節を開いたまま canManage が false になったら、一般へ戻る', async () => {
      const wrapper = mountView({ canManage: true });
      await flushPromises();
      const nav = document.body.querySelector('nav[aria-label="設定セクション"]')!;
      [...nav.querySelectorAll('button')].find((b) => b.textContent?.includes('削除'))!.click();
      await flushPromises();
      expect(document.body.textContent).toContain(
        'このプロジェクトとすべてのタスクを完全に削除します',
      );

      await wrapper.setProps({ canManage: false });
      await flushPromises();

      expect(nav.querySelector('button[aria-current="true"]')?.textContent).toContain('一般');
      expect(document.body.textContent).not.toContain(
        'このプロジェクトとすべてのタスクを完全に削除します',
      );
    });

    it('対照: canManage が true なら、保存の入力と「削除」が出る', async () => {
      mountView({ canManage: true });
      await flushPromises();

      expect(document.body.querySelector('#name')).not.toBeNull();
      expect(navLabels()).toContain('削除');
    });
  });
});
