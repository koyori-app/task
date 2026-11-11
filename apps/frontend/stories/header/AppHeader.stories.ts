import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide, ref } from 'vue';

import { Button, Drawer } from '@koyori-app/ui-vue';
import { PhGear, PhSidebarSimple } from '@phosphor-icons/vue';

import AppHeader from '@/components/header/AppHeader.vue';
import NavUser from '@/components/header/NavUser.vue';
import TenantSwitcher from '@/components/header/TenantSwitcher.vue';
import AppSidebar from '@/components/sidebar/AppSidebar.vue';
import { useAuthStore } from '@/stores/auth';
import type { Tenant } from '@/stores/tenant';
import { useTenantStore } from '@/stores/tenant';

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

const user = { name: 'yupix', email: 'm@example.com', avatar: '' };

const authUser = {
  id: ownerId,
  email: user.email,
  username: user.name,
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

// アイコン（絵文字）付きと無しを並べ、レールでの見え方（無しは頭文字）を確かめる
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

function provideQueryClient() {
  provide(
    VUE_QUERY_CLIENT,
    new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
    }),
  );
}

/** ドロップダウンの頭に出すメンバー数のぶんだけ応答を用意する。 */
function mockMembers(count = 4) {
  return () => {
    const original = globalThis.fetch;
    globalThis.fetch = (async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const body = url.includes('/v1/auth/me')
        ? { id: ownerId, username: 'yupix', avatar_url: null }
        : url.includes('/members')
          ? Array.from({ length: count }, (_, index) => ({ id: String(index) }))
          : [];
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }) as typeof globalThis.fetch;
    return () => {
      globalThis.fetch = original;
    };
  };
}

/**
 * AppHeader は Pinia と pageContext から値を取るので、ストーリーでは同じ構成を
 * 手で組んで見た目だけを確認する（テナントは左、アカウントは右）。
 */
const renderHeader =
  (options: { tenants?: Tenant[]; selectedTenantId?: string | null } = {}) =>
  () => ({
    components: { NavUser, TenantSwitcher, PhGear },
    setup() {
      setActivePinia(createPinia());
      provideQueryClient();
      const store = useTenantStore();
      store.$patch({
        tenants: options.tenants ?? [acme, globex],
        selectedTenantId: options.selectedTenantId ?? acme.id,
        isLoading: false,
        error: null,
      });
      return { store, user, logout: fn(), selectTenant: (t: Tenant) => store.selectTenant(t) };
    },
    template: `
      <header class="flex h-12 shrink-0 items-center gap-2 border-b bg-sidebar px-3">
        <TenantSwitcher
          :tenants="store.tenants"
          :selected-tenant-id="store.selectedTenantId"
          :loading="store.isLoading"
          :error="store.error"
          @select="selectTenant"
        />
        <div class="ml-auto flex items-center gap-1">
          <a
            href="/acme/settings"
            aria-label="テナント設定"
            class="flex size-7.5 items-center justify-center rounded-md text-muted-foreground hover:bg-sidebar-accent"
          >
            <PhGear class="size-4" />
          </a>
          <span class="mx-1 h-4 w-px bg-border" />
          <NavUser :user="user" :on-logout="logout" />
        </div>
      </header>
    `,
  });

const meta = {
  title: 'Header/AppHeader',
  component: TenantSwitcher,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof TenantSwitcher>;

export default meta;

type Story = StoryObj;

export const Default: Story = { render: renderHeader(), beforeEach: mockMembers() };

/**
 * テナントを押したときの中身。現在地・設定への導線・切り替え・作成が 1 枚に載る。
 */
export const TenantMenuOpen: Story = {
  render: renderHeader(),
  beforeEach: mockMembers(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);

    await userEvent.click(await canvas.findByRole('button', { name: /Acme Inc/ }));

    // 項目は閉じていても DOM にあるので、文字ではなく役割で開くのを待つ
    await expect(page.findByRole('menuitem', { name: 'テナント設定' })).resolves.toBeVisible();
    await expect(page.findByText(/4 人のメンバー/)).resolves.toBeInTheDocument();
    await expect(page.getByRole('menuitem', { name: 'メンバー' })).toBeInTheDocument();
    await expect(page.getByText('テナントを切り替え')).toBeInTheDocument();
    await expect(page.getByRole('menuitem', { name: 'テナントを作成' })).toBeInTheDocument();
  },
};

export const NoTenants: Story = {
  render: renderHeader({ tenants: [], selectedTenantId: null }),
  beforeEach: mockMembers(),
};

/**
 * +Layout と同じ並び（全幅のヘッダーの下に、サイドバーと本文）を組む。
 * +Layout は認証や devtools まで抱えるので、開閉の状態だけを同じ形で持たせる。
 * `narrow` は狭い画面（サイドバーをモーダルの Drawer で出す）。
 */
const renderLayout = (narrow: boolean) => () => ({
  components: { AppHeader, AppSidebar, Button, Drawer, PhSidebarSimple },
  setup() {
    const rail = ref(false);
    const drawerOpen = ref(false);
    function toggle() {
      if (narrow) drawerOpen.value = !drawerOpen.value;
      else rail.value = !rail.value;
    }
    // ストーリーの iframe を遷移させない（閉じる処理は先に委譲で走る）
    function stayOnPage(event: MouseEvent) {
      if ((event.target as Element).closest('a')) event.preventDefault();
    }
    return { narrow, rail, drawerOpen, toggle, stayOnPage };
  },
  template: `
    <div class="flex h-svh flex-col overflow-hidden" @click="stayOnPage">
      <AppHeader />
      <div class="flex min-h-0 flex-1">
        <Drawer
          label="ナビゲーション"
          placement="left"
          :modal="narrow"
          :open="drawerOpen"
          @close="drawerOpen = false"
        >
          <AppSidebar
            :rail="!narrow && rail"
            @update:rail="rail = $event"
            @close="drawerOpen = false"
          />
        </Drawer>
        <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
          <header class="flex h-16 shrink-0 items-center gap-2 px-4">
            <Button
              variant="ghost"
              ariaLabel="サイドバーを開閉"
              :ariaExpanded="narrow ? drawerOpen : !rail"
              ariaControls="app-sidebar"
              @click="toggle"
            >
              <template #icon><PhSidebarSimple /></template>
            </Button>
          </header>
          <div class="flex flex-col gap-4 p-4 pt-0 text-sm text-muted-foreground">
            <p v-for="n in 40" :key="n">本文 {{ n }}</p>
          </div>
        </main>
      </div>
    </div>
  `,
});

/**
 * 広い画面。ヘッダーは全幅のまま、本文だけがスクロールする。
 * 開閉ボタンと端のつまみは、どちらもアイコンだけの表示（rail）を切り替える。
 */
export const WithSidebar: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  render: renderLayout(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 本文の見出し行も header なので、アプリのヘッダーは印で取る
    const banner = canvasElement.querySelector<HTMLElement>('[data-slot="app-header"]')!;
    await canvas.findByRole('navigation', { name: 'ナビゲーション' });
    const main = canvas.getByRole('main');
    // ヘッダーはサイドバー・本文の上に全幅で載り、スクロールするのは本文だけ
    await expect(main.contains(banner)).toBe(false);
    await expect(banner.getBoundingClientRect().width).toBe(
      (main.parentElement as HTMLElement).getBoundingClientRect().width,
    );
    await expect(getComputedStyle(main).overflowY).toBe('auto');

    const toggle = canvas.getByRole('button', { name: 'サイドバーを開閉' });
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');

    await userEvent.click(toggle);
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(canvasElement.querySelector('#app-sidebar')).toHaveAttribute(
      'data-sidebar-rail',
      'true',
    );
    // 畳んでもリンクは名前ごと残る
    await expect(canvas.getByRole('link', { name: 'My Tasks' })).toBeInTheDocument();

    // 端のつまみでも戻せる
    await userEvent.click(canvas.getByRole('button', { name: 'アイコンだけの表示' }));
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(canvasElement.querySelector('#app-sidebar')).toHaveAttribute(
      'data-sidebar-rail',
      'false',
    );
  },
};

/** 広い画面でアイコンだけの表示（rail）にした状態。 */
export const WithSidebarRail: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  render: renderLayout(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: 'サイドバーを開閉' }));
    await expect(await canvas.findByText('📥')).toBeVisible();
  },
};

/**
 * 狭い画面。サイドバーはモーダルの Drawer で、開閉ボタンで開く。
 * ナビのリンクを押したら閉じる。
 */
export const WithSidebarNarrow: Story = {
  decorators: [realLayoutDecorator()],
  beforeEach: mockLayoutFetch,
  globals: { viewport: { value: 'mobile2', isRotated: false } },
  render: renderLayout(true),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = canvas.getByRole('button', { name: 'サイドバーを開閉' });
    // 閉じている Drawer の中身は読み上げにも出ない
    await expect(canvas.queryByRole('navigation', { name: 'ナビゲーション' })).toBeNull();

    await userEvent.click(toggle);
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(await canvas.findByRole('navigation', { name: 'ナビゲーション' })).toBeVisible();

    await userEvent.click(await canvas.findByRole('link', { name: 'My Tasks' }));
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    // 閉じるアニメーションのあいだは残るので、消えるのを待つ
    await waitFor(() =>
      expect(canvas.queryByRole('navigation', { name: 'ナビゲーション' })).toBeNull(),
    );
  },
};
