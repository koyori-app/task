import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import { defineComponent, h } from 'vue';
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
const task = {
  id: 'task-1',
  seq_id: 1,
  title: 'OAuth 対応を実装する',
  priority: 'High',
  status_id: status.id,
  project_id: project.id,
  soft_deadline: null,
  hard_deadline: null,
  is_archived: false,
  labels: [],
  progress_pct: 0,
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-15T00:00:00Z',
  created_by: { id: 'user-1', username: 'a', avatar_url: null },
  assignees: [],
};
const projectBase = `/v1/tenants/${tenant.id}/projects/${project.id}`;

function stubFetch() {
  const replies = new Map<string, unknown>([
    ['/v1/auth/me', { id: 'user-1', email: 'a@example.com', username: 'a', email_verified: true }],
    ['/v1/tenants', [tenant]],
    [`/v1/tenants/${tenant.id}/projects`, [project]],
    [`${projectBase}/statuses`, [status]],
    [`${projectBase}/tasks`, { tasks: [task], total: 1, next_cursor: null }],
    [`${projectBase}/labels`, []],
    [`${projectBase}/assignable-users`, []],
  ]);
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      const pathname = new URL(request.url, 'http://localhost').pathname.replace(/^\/api/, '');
      return Response.json(replies.get(pathname) ?? { message: 'Forbidden' }, {
        status: replies.has(pathname) ? 200 : 403,
      });
    }),
  );
}

// 詳細ペインの中身は見ない。閉じるボタンだけを持たせる
const TaskDetailPaneStub = defineComponent({
  name: 'TaskDetailPane',
  emits: ['close'],
  setup:
    (_, { emit }) =>
    () =>
      h('button', { type: 'button', onClick: () => emit('close') }, '詳細を閉じる'),
});

function mountPage() {
  const pathname = '/acme/projects/ENG/tasks';
  return mount(ProjectPage, {
    attachTo: document.body,
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
          urlParsed: { search: { view: 'table' } },
          routeParams: { tenant: 'acme', projectKey: 'ENG' },
        },
      },
      stubs: {
        CreateTaskDialog: true,
        ClientOnly: true,
        TaskDetailOverlay: true,
        TaskDetailPane: TaskDetailPaneStub,
      },
    },
  });
}

describe('タスク一覧の分割ペインの開閉とフォーカス', () => {
  it('一覧のリンクで開いても閉じても、フォーカスはそのタスクのリンクに戻る', async () => {
    stubFetch();
    const wrapper = mountPage();
    const link = () => wrapper.find<HTMLAnchorElement>('a[data-task-seq-key="ENG-1"]');
    await vi.waitFor(() => expect(link().exists()).toBe(true));

    // リンクで Enter を押すと、ブラウザはそのリンクの click を発火する
    const before = link().element;
    before.focus();
    await link().trigger('click');
    await vi.waitFor(() => expect(wrapper.find('[role="separator"]').exists()).toBe(true));
    // 一覧は作り直されている（同じ要素のままなら、この試験は戻す処理を確かめていない）
    expect(link().element).not.toBe(before);
    expect(document.activeElement).toBe(link().element);

    // 詳細ペインの閉じるボタンで閉じると、閉じたタスクのリンクへ戻る
    const close = wrapper.getComponent(TaskDetailPaneStub).get('button');
    close.element.focus();
    await close.trigger('click');
    await vi.waitFor(() => expect(wrapper.find('[role="separator"]').exists()).toBe(false));
    expect(document.activeElement).toBe(link().element);
  });
});
