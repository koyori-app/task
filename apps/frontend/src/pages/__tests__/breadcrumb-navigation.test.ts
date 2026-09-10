import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { h, type Component } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import Layout from '../+Layout.vue';
import TaskPage from '../@tenant/projects/@projectKey/tasks/@taskId/+Page.vue';
import ProjectPage from '../@tenant/projects/@projectKey/tasks/+Page.vue';
import HomePage from '../@tenant/my-tasks/+Page.vue';

vi.mock('@/components/header/AppHeader.vue', () => ({
  default: { template: '<div />' },
  __isKeepAlive: false,
  __isTeleport: false,
}));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

const tenant = {
  id: 'tenant-1',
  display_id: 'acme',
  name: '会社',
  membership: 'Guest',
  description: '',
  icon_url: '',
};
const project = {
  id: 'project-1',
  tenant_id: tenant.id,
  key: 'ENG',
  name: '開発プロジェクト',
  description: '',
  is_personal: false,
};
const parent = {
  id: 'task-2',
  project_id: project.id,
  seq_id: 2,
  title: '親タスクの実画面',
  description: null,
  parent_task_id: null,
  status_id: 'status-1',
  priority: 'Medium',
  progress_pct: 0,
  labels: [],
  assignees: [],
  custom_field_values: [],
  is_archived: false,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const child = {
  ...parent,
  id: 'task-3',
  seq_id: 3,
  title: '子タスクの実画面',
  parent_task_id: parent.id,
};
const status = {
  id: 'status-1',
  project_id: project.id,
  name: '未着手',
  color: '#64748b',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
};
const requests: string[] = [];
const rejected: string[] = [];

beforeEach(() => {
  requests.length = 0;
  rejected.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      const pathname = new URL(request.url, 'http://localhost').pathname.replace(/^\/api/, '');
      requests.push(pathname);
      const projectBase = `/v1/tenants/${tenant.id}/projects/${project.id}`;
      const replies = new Map<string, unknown>([
        [
          '/v1/auth/me',
          { id: 'user-1', email: 'guest@example.com', username: 'guest', email_verified: true },
        ],
        ['/v1/tenants', [tenant]],
        [`/v1/tenants/${tenant.id}/projects`, [project]],
        [`/v1/tenants/${tenant.id}/users/me/tasks`, { tasks: [], total: 0 }],
        [`${projectBase}/tasks`, { tasks: [child], total: 1, next_cursor: null }],
        [`${projectBase}/statuses`, [status]],
        [`${projectBase}/labels`, []],
        [`${projectBase}/assignable-users`, []],
        ...[parent, child].flatMap((task): [string, unknown][] => [
          [`${projectBase}/tasks/ENG-${task.seq_id}`, task],
          [
            `${projectBase}/tasks/ENG-${task.seq_id}/relations`,
            {
              parent: task.parent_task_id ? parent : null,
              subtasks: [],
              blocks: [],
              blocked_by: [],
            },
          ],
          [`${projectBase}/tasks/ENG-${task.seq_id}/comments`, { comments: [] }],
          [
            `${projectBase}/tasks/ENG-${task.seq_id}/activities`,
            { activities: [], next_cursor: null },
          ],
        ]),
      ]);
      // Guest に開く既存の口だけを成功させる。tenant取得・members等は403。
      if (!replies.has(pathname)) rejected.push(pathname);
      return new Response(JSON.stringify(replies.get(pathname) ?? { message: 'Forbidden' }), {
        status: replies.has(pathname) ? 200 : 403,
        headers: { 'Content-Type': 'application/json' },
      });
    }),
  );
});

function mountPage(pathname: string) {
  const segments = pathname.split('/').filter(Boolean);
  const isHome = segments[1] === 'my-tasks';
  const taskId = segments[4];
  const Page: Component = isHome ? HomePage : taskId ? TaskPage : ProjectPage;
  return mount(Layout, {
    slots: { default: () => h(Page) },
    global: {
      plugins: [
        createPinia(),
        [
          VueQueryPlugin,
          {
            queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }),
          },
        ],
      ],
      provide: {
        'vike-vue:usePageContext': {
          urlPathname: pathname,
          urlOriginal: pathname,
          urlParsed: { search: {} },
          routeParams: { tenant: segments[0], projectKey: segments[2], taskId },
        },
        'vike-vue:useData': { descriptionHtml: null, descriptionSource: null },
      },
      stubs: { ClientOnly: true, AppHeader: true, AppSidebar: true, AppSidebarSkeleton: true },
    },
  });
}

describe('パンくずから実際のページへ', () => {
  it.each([
    ['ホーム', '/acme/my-tasks', 'My Tasks'],
    [project.name, '/acme/projects/ENG/tasks', 'project'],
    [parent.title, '/acme/projects/ENG/tasks/ENG-2', parent.title],
  ])('%s のリンク先のページを Guest のAPI応答で表示できる', async (name, href, expected) => {
    const source = mountPage('/acme/projects/ENG/tasks/ENG-3');
    await vi.waitFor(() =>
      expect(source.find('nav[aria-label="パンくず"] [aria-current="page"]').text()).toBe(
        child.title,
      ),
    );
    expect(source.get('h1').text()).toBe(child.title);
    const link = source
      .findAll('nav[aria-label="パンくず"] a')
      .find((item) => item.text() === name)!;
    expect(link.attributes('href')).toBe(href);
    // 実画面が使う task/relations の取得は各1回。ヘッダーが通信を足していない。
    expect(requests.filter((path) => path.endsWith('/tasks/ENG-3'))).toHaveLength(1);
    expect(requests.filter((path) => path.endsWith('/tasks/ENG-3/relations'))).toHaveLength(1);
    source.unmount();
    const destination = mountPage(link.attributes('href')!);
    await flushPromises();
    if (expected === 'project') {
      await vi.waitFor(() =>
        expect(destination.find('input[aria-label="タスクを検索"]').exists()).toBe(true),
      );
      expect(destination.get('nav[aria-label="パンくず"] [aria-current="page"]').text()).toBe(
        project.name,
      );
    } else {
      await vi.waitFor(() => expect(destination.get('h1').text()).toBe(expected));
    }
    expect(rejected).toEqual([]);
    expect(destination.find('script[type="application/ld+json"]').exists()).toBe(false);
  });
});
