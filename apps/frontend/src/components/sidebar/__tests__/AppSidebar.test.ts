import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { computed } from 'vue';
import { enableAutoUnmount, mount } from '@vue/test-utils';

const { navigateSpy, projectsControl, tenantStoreStub } = vi.hoisted(() => ({
  navigateSpy: vi.fn(),
  projectsControl: { data: [] as unknown[], isLoading: false, isError: false },
  tenantStoreStub: {
    tenants: [] as unknown[],
    selectedTenantId: null as string | null,
    isLoading: false,
    error: null as string | null,
    loadTenants: vi.fn(),
    selectTenant: vi.fn(),
  },
}));

vi.mock('vike/client/router', () => ({ navigate: navigateSpy }));

vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({ routeParams: { tenant: 'acme' }, urlPathname: '/acme/my-tasks' }),
}));

vi.mock('@/composables/useRouteAlignedTenantId', () => ({
  useRouteAlignedTenantId: () => computed(() => 'tenant-1'),
}));

vi.mock('@/stores/tenant', () => ({ useTenantStore: () => tenantStoreStub }));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    useProjectsQuery: () => ({
      data: computed(() => projectsControl.data),
      isLoading: computed(() => projectsControl.isLoading),
      isError: computed(() => projectsControl.isError),
      refetch: vi.fn(),
    }),
  };
});

import AppSidebar from '../AppSidebar.vue';

enableAutoUnmount(afterEach);

/**
 * AppSidebar を組み、狭い画面のサイドバー（Drawer）を閉じる指示 `close` を拾う。
 * 画面幅の判定と Drawer の開閉は +Layout が持つので、ここでは指示が出るかだけを見る。
 */
function mountSidebar(onClose: () => void) {
  return mount(AppSidebar, { props: { onClose }, attachTo: document.body });
}

function findLink(href: string) {
  const nav = document.body.querySelector('nav[aria-label="ナビゲーション"]');
  if (!nav) throw new Error('navigation not found');
  const link = nav.querySelector<HTMLAnchorElement>(`a[href="${href}"]`);
  if (!link) throw new Error(`link "${href}" not found`);
  return link;
}

/** `aria-label` で名前を付けたアイコンだけのボタンか、文字で名前が出るボタンを探す */
function findButton(label: string, by: 'aria-label' | 'text') {
  const button = [...document.body.querySelectorAll('button')].find((el) =>
    by === 'aria-label'
      ? el.getAttribute('aria-label') === label
      : el.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  return button as HTMLButtonElement;
}

beforeEach(() => {
  navigateSpy.mockReset();
  projectsControl.data = [];
  projectsControl.isLoading = false;
  projectsControl.isError = false;
  tenantStoreStub.tenants = [{ id: 'tenant-1', display_id: 'acme', name: 'Acme' }];
});

/**
 * ナビのリンクは Sidebar の根に置いたイベント委譲で拾う。効かなくなる現実的な
 * 壊れ方は `@click` の行が消えることではなく、ナビが Sidebar の外へ移ったり
 * 別のコンポーネントで包まれたりして委譲の下から抜けることなので、純粋関数ではなく
 * 実際のマークアップから辿って確認する。
 */
describe('AppSidebar のナビリンク', () => {
  it('ナビのリンクを押したら、サイドバーを閉じる指示を出す', () => {
    const onClose = vi.fn();
    mountSidebar(onClose);

    findLink('/acme/my-tasks').click();

    expect(onClose).toHaveBeenCalledOnce();
  });

  it('現在地のリンクを強調する', () => {
    mountSidebar(vi.fn());

    expect(findLink('/acme/my-tasks').getAttribute('aria-current')).toBe('page');
    expect(findLink('/acme').getAttribute('aria-current')).toBeNull();
  });
});

/**
 * 「プロジェクトを作成」は <button> から navigate を呼ぶ。リンクを踏まないので
 * 本文のイベント委譲（closest('a')）では拾えず、遷移しても作成画面が
 * 狭い画面のサイドバーに覆われたままになっていた。導線は 2 か所ある。
 */
describe('AppSidebar のプロジェクト作成導線', () => {
  it('空状態の作成ボタンで、遷移してサイドバーを閉じる', async () => {
    const onClose = vi.fn();
    mountSidebar(onClose);

    findButton('プロジェクトを作成', 'text').click();
    await Promise.resolve();

    expect(navigateSpy).toHaveBeenCalledWith('/acme/projects/new');
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('プロジェクトがある状態の見出し横の作成ボタンでも、遷移してサイドバーを閉じる', async () => {
    projectsControl.data = [
      {
        id: 'project-1',
        key: 'ALPHA',
        name: 'Team Alpha',
        description: '',
        icon_emoji: null,
        icon_url: null,
        is_personal: false,
        personal_owner_id: null,
        tenant_id: 'tenant-1',
      },
    ];
    const onClose = vi.fn();
    mountSidebar(onClose);

    // 空状態のカードは出ないので、拾えるのは見出し横のボタンだけ
    findButton('プロジェクトを作成', 'aria-label').click();
    await Promise.resolve();

    expect(navigateSpy).toHaveBeenCalledWith('/acme/projects/new');
    expect(onClose).toHaveBeenCalledOnce();
  });
});
