import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import ProjectPage from '../+Page.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

const tenant = {
  id: 'tenant-1',
  display_id: 'acme',
  name: '会社',
  membership: 'Owner',
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
const projectBase = `/v1/tenants/${tenant.id}/projects/${project.id}`;

/** statuses の応答だけを差し替えられる fetch。他は一覧の表示に要る最小の応答を返す */
function stubFetch(statuses: () => Response | Promise<Response>) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      const pathname = new URL(request.url, 'http://localhost').pathname.replace(/^\/api/, '');
      if (pathname === `${projectBase}/statuses`) return statuses();
      const replies = new Map<string, unknown>([
        [
          '/v1/auth/me',
          { id: 'user-1', email: 'a@example.com', username: 'a', email_verified: true },
        ],
        ['/v1/tenants', [tenant]],
        [`/v1/tenants/${tenant.id}/projects`, [project]],
        [`${projectBase}/tasks`, { tasks: [], total: 0, next_cursor: null }],
        [`${projectBase}/labels`, []],
        [`${projectBase}/assignable-users`, []],
      ]);
      return Response.json(replies.get(pathname) ?? { message: 'Forbidden' }, {
        status: replies.has(pathname) ? 200 : 403,
      });
    }),
  );
}

function mountPage() {
  const pathname = '/acme/projects/ENG/tasks';
  return mount(ProjectPage, {
    global: {
      plugins: [
        createPinia(),
        [
          VueQueryPlugin,
          { queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        ],
      ],
      provide: {
        'vike-vue:usePageContext': {
          urlPathname: pathname,
          urlOriginal: pathname,
          urlParsed: { search: {} },
          routeParams: { tenant: 'acme', projectKey: 'ENG' },
        },
      },
      stubs: { CreateTaskDialog: true, ClientOnly: true },
    },
  });
}

const pressT = () =>
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', cancelable: true }));

describe('タスク一覧の T ショートカット', () => {
  it('一覧を表示していれば T で作成ダイアログを開く', async () => {
    stubFetch(() => Response.json([status]));
    const wrapper = mountPage();
    await vi.waitFor(() =>
      expect(wrapper.find('input[aria-label="タスクを検索"]').exists()).toBe(true),
    );
    pressT();
    await flushPromises();
    expect(wrapper.getComponent({ name: 'CreateTaskDialog' }).props('open')).toBe(true);
  });

  it('読み込み中の T は無視し、取得が終わっても開かない', async () => {
    let finish!: () => void;
    const gate = new Promise<void>((resolve) => {
      finish = resolve;
    });
    stubFetch(async () => {
      await gate;
      return Response.json([status]);
    });
    const wrapper = mountPage();
    await flushPromises();
    expect(wrapper.find('input[aria-label="タスクを検索"]').exists()).toBe(false);
    pressT();
    finish();
    await vi.waitFor(() =>
      expect(wrapper.find('input[aria-label="タスクを検索"]').exists()).toBe(true),
    );
    expect(wrapper.getComponent({ name: 'CreateTaskDialog' }).props('open')).toBe(false);
  });

  it('取得に失敗していれば T で開かない', async () => {
    stubFetch(() => Response.json({ message: 'x' }, { status: 500 }));
    const wrapper = mountPage();
    await vi.waitFor(() => expect(wrapper.text()).toContain('タスクの読み込みに失敗しました'));
    pressT();
    await flushPromises();
    expect(wrapper.findComponent({ name: 'CreateTaskDialog' }).exists()).toBe(false);
  });
});
