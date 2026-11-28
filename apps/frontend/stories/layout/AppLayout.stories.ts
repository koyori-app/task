import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { computed, provide, ref } from 'vue';

import { Button, Drawer, Toaster } from '@koyori-app/ui-vue';
import { PhSidebarSimple } from '@phosphor-icons/vue';

import AppSidebar from '@/components/sidebar/AppSidebar.vue';
import AppTenantRail from '@/components/tenant-rail/AppTenantRail.vue';
import { tenantStatusLabel } from '@/lib/tenant-status-label';
import { useAuthStore } from '@/stores/auth';
import { useTenantStore, type Tenant } from '@/stores/tenant';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const ownerId = '00000000-0000-4000-8000-000000000001';

const tenant = (id: string, name: string, displayId: string): Tenant => ({
  id,
  name,
  display_id: displayId,
  description: `${name} tenant`,
  icon_url: '',
  owner_id: ownerId,
  require_2fa: false,
  membership: 'Owner' as const,
});

const acme = tenant('00000000-0000-4000-8000-000000000010', 'Acme Inc', 'acme');
const globex = tenant('00000000-0000-4000-8000-000000000020', 'Globex', 'globex');

const authUser = {
  id: ownerId,
  email: 'm@example.com',
  username: 'yupix',
  email_verified: true,
  is_admin: false,
  is_suspended: false,
  totp_enabled: false,
  avatar_url: null,
  bio: null,
};

const layoutPageContext = {
  urlPathname: '/acme/my-tasks',
  routeParams: { tenant: 'acme' },
};

// アイコン（絵文字）付きと無しを並べ、畳んだ状態での見え方（無しは頭文字）を確かめる
const layoutProjects = [
  {
    id: '00000000-0000-4000-8000-000000000101',
    tenant_id: acme.id,
    name: 'Design Engineering',
    description: '',
    key: 'design',
    is_personal: false,
    icon_emoji: null,
    icon_url: null,
    personal_owner_id: null,
  },
  {
    id: '00000000-0000-4000-8000-000000000102',
    tenant_id: acme.id,
    name: 'Inbox',
    description: '',
    key: 'inbox',
    is_personal: true,
    icon_emoji: '📥',
    icon_url: null,
    personal_owner_id: ownerId,
  },
];

const jsonResponse = (data: unknown) =>
  new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

function mockLayoutFetch() {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation(async (request: Request | string) => {
    const url = typeof request === 'string' ? request : request.url;
    const pathname = new URL(url, 'http://localhost').pathname;

    // 実際のパスは baseUrl（/api）付きなので、完全一致ではなく末尾で見る
    if (pathname.endsWith('/v1/auth/me')) return jsonResponse(authUser);
    if (pathname.endsWith('/v1/tenants')) return jsonResponse([acme, globex]);
    if (pathname.endsWith(`/tenants/${acme.id}/projects`)) return jsonResponse(layoutProjects);
    return jsonResponse([]);
  });

  return () => {
    globalThis.fetch = original;
  };
}

function realLayoutDecorator() {
  return () => ({
    setup() {
      setActivePinia(createPinia());
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, gcTime: 0, staleTime: 0 },
          mutations: { retry: false },
        },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      provide(PAGE_CONTEXT_KEY, layoutPageContext);

      useAuthStore().setUser(authUser);
    },
    template: '<story />',
  });
}

/**
 * +Layout と同じ並び（左からテナントのレール、サイドバー、本文。ヘッダーは無い）を組む。
 * +Layout は認証や devtools まで抱えるので、開閉の状態だけを同じ形で持たせる。
 * `narrow` は狭い画面（レールとサイドバーをモーダルの Drawer に並べる）。
 */
const renderLayout = (narrow: boolean) => () => ({
  components: { AppSidebar, AppTenantRail, Button, Drawer, PhSidebarSimple, Toaster },
  setup() {
    const rail = ref(false);
    const drawerOpen = ref(false);
    const tenantStore = useTenantStore();
    const currentTenantLabel = computed(() =>
      tenantStatusLabel({
        loading: tenantStore.isLoading,
        error: tenantStore.error,
        tenants: tenantStore.tenants,
        selected: tenantStore.selectedTenant,
      }),
    );
    function toggle() {
      if (narrow) drawerOpen.value = !drawerOpen.value;
      else rail.value = !rail.value;
    }
    // ストーリーの iframe を遷移させない（閉じる処理は先に委譲で走る）
    function stayOnPage(event: MouseEvent) {
      if ((event.target as Element).closest('a')) event.preventDefault();
    }
    return { narrow, rail, drawerOpen, currentTenantLabel, toggle, stayOnPage };
  },
  template: `
    <div class="flex h-svh overflow-hidden" @click="stayOnPage">
      <div class="flex shrink-0 [--koyori-drawer-width:20rem]">
        <Drawer
          label="ナビゲーション"
          placement="left"
          :modal="narrow"
          :open="drawerOpen"
          @close="drawerOpen = false"
        >
          <div
            class="flex h-full min-h-0 [--koyori-sidebar-border:0] [--koyori-sidebar-radius:0]"
            data-slot="app-navigation"
          >
            <AppTenantRail @close="drawerOpen = false" />
            <div
              class="flex min-h-0"
              :class="narrow ? 'min-w-0 flex-1 border-l' : 'shrink-0 border-x'"
              data-slot="sidebar-frame"
            >
              <AppSidebar
                :rail="!narrow && rail"
                @update:rail="rail = $event"
                @close="drawerOpen = false"
              />
            </div>
          </div>
        </Drawer>
      </div>
      <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
        <header class="flex shrink-0 items-center gap-2 px-4" :class="narrow ? 'h-12' : 'h-16'">
          <Button
            variant="ghost"
            ariaLabel="サイドバーを開閉"
            :ariaExpanded="narrow ? drawerOpen : !rail"
            ariaControls="app-sidebar"
            @click="toggle"
          >
            <template #icon><PhSidebarSimple /></template>
          </Button>
          <button
            v-if="narrow"
            type="button"
            class="flex min-w-0 items-center gap-2 rounded-md px-1 py-1 font-medium text-sm hover:bg-accent"
            aria-haspopup="dialog"
            aria-controls="app-sidebar"
            data-slot="current-tenant"
            @click="drawerOpen = true"
          >
            <span class="truncate">{{ currentTenantLabel }}</span>
          </button>
        </header>
        <div class="flex flex-col gap-4 p-4 pt-0 text-sm text-muted-foreground">
          <p v-for="n in 40" :key="n">本文 {{ n }}</p>
        </div>
      </main>
      <Toaster />
    </div>
  `,
});

const meta = {
  title: 'Layout/AppLayout',
  parameters: { layout: 'fullscreen' },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 広い画面。ヘッダーは無く、レールとサイドバーは本文の外に並び、本文だけがスクロールする。
 * 開閉ボタンと端のつまみは、どちらもサイドバーのアイコンだけの表示を切り替える。
 */
export const Default: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  render: renderLayout(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('navigation', { name: 'ナビゲーション' });
    const main = canvas.getByRole('main');
    const rail = canvasElement.querySelector<HTMLElement>('[data-slot="app-tenant-rail"]')!;
    await expect(canvasElement.querySelector('[data-slot="app-header"]')).toBeNull();
    // レールは本文の外で、画面の上端から下端まで伸びる
    await expect(main.contains(rail)).toBe(false);
    await expect(rail.getBoundingClientRect().top).toBe(0);
    await expect(rail.getBoundingClientRect().width).toBe(64);
    await expect(main.getBoundingClientRect().top).toBe(0);
    await expect(getComputedStyle(main).overflowY).toBe('auto');

    // 見出しにテナント名が出る（押せない）
    await waitFor(() =>
      expect(canvas.getByRole('heading', { level: 2, name: 'Acme Inc' })).toBeInTheDocument(),
    );

    // 境目の線は 1 本だけ。Sidebar 自身は枠を持たず、包む要素が左右の線を引く
    const sidebar = canvasElement.querySelector<HTMLElement>('#app-sidebar')!;
    const frame = canvasElement.querySelector<HTMLElement>('[data-slot="sidebar-frame"]')!;
    await expect(getComputedStyle(sidebar).borderLeftWidth).toBe('0px');
    await expect(getComputedStyle(sidebar).borderRightWidth).toBe('0px');
    await expect(getComputedStyle(frame).borderLeftWidth).toBe('1px');
    await expect(getComputedStyle(rail).borderRightWidth).toBe('0px');

    const toggle = canvas.getByRole('button', { name: 'サイドバーを開閉' });
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');

    await userEvent.click(toggle);
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(sidebar).toHaveAttribute('data-sidebar-rail', 'true');
    // 畳んでもリンクは名前ごと残る
    await expect(canvas.getByRole('link', { name: 'My Tasks' })).toBeInTheDocument();
    // 畳んでもテナントのメニューには届く
    await expect(canvas.getByRole('button', { name: 'テナントのメニュー' })).toBeInTheDocument();

    // 端のつまみでも戻せる
    await userEvent.click(canvas.getByRole('button', { name: 'アイコンだけの表示' }));
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(sidebar).toHaveAttribute('data-sidebar-rail', 'false');
  },
};

/** サイドバーの見出しの「…」から、今のテナントのメニューを開いた状態。 */
export const TenantMenuOpen: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  render: renderLayout(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'テナントのメニュー' }));
    // メニューは top layer に出るので、文書全体から探す
    const page = within(canvasElement.ownerDocument.body);
    await waitFor(() => expect(page.getByRole('menuitem', { name: 'テナント設定' })).toBeVisible());
    await expect(page.getByRole('menuitem', { name: 'メンバー' })).toBeInTheDocument();
    await expect(page.getByRole('menuitem', { name: '表示 ID をコピー' })).toBeInTheDocument();
  },
};

/** 広い画面でアイコンだけの表示（rail）にした状態。 */
export const SidebarRail: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  render: renderLayout(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: 'サイドバーを開閉' }));
    // 絵文字は畳む前から DOM にある。畳んだ後のアイコンだけの表示で見えるのを待つ
    const icon = await canvas.findByText('📥');
    await waitFor(() => expect(icon).toBeVisible());
  },
};

/**
 * 狭い画面。帯には開閉ボタンと今のテナントだけを出し、パンくずは出さない。
 * どちらを押しても、レールとサイドバーを並べた Drawer が開く。ナビのリンクを押したら閉じる。
 */
export const Narrow: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  globals: { viewport: { value: 'mobile2', isRotated: false } },
  render: renderLayout(true),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = canvas.getByRole('button', { name: 'サイドバーを開閉' });
    // 閉じている Drawer の中身は読み上げにも出ない
    await expect(canvas.queryByRole('navigation', { name: 'ナビゲーション' })).toBeNull();

    const current = await canvas.findByRole('button', { name: 'Acme Inc' });
    await userEvent.click(current);
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(await canvas.findByRole('navigation', { name: 'ナビゲーション' })).toBeVisible();
    // Drawer は幅 320px で、その中でレールとサイドバーが横に並ぶ
    const navigation = canvasElement.querySelector<HTMLElement>('[data-slot="app-navigation"]')!;
    const drawer = navigation.closest('dialog')!;
    await expect(Math.round(drawer.getBoundingClientRect().width)).toBe(320);
    const rail = navigation.querySelector<HTMLElement>('[data-slot="app-tenant-rail"]')!;
    const frame = navigation.querySelector<HTMLElement>('[data-slot="sidebar-frame"]')!;
    await expect(rail.getBoundingClientRect().top).toBe(frame.getBoundingClientRect().top);
    await expect(rail.getBoundingClientRect().right).toBe(frame.getBoundingClientRect().left);

    await userEvent.click(await canvas.findByRole('link', { name: 'My Tasks' }));
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    // 閉じるアニメーションのあいだは残るので、消えるのを待つ
    await waitFor(() =>
      expect(canvas.queryByRole('navigation', { name: 'ナビゲーション' })).toBeNull(),
    );

    // 開閉ボタンでも開く
    await userEvent.click(toggle);
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  },
};
