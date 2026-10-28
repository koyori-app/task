import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import ProjectPage from '../+Page.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

// 表（view=table）の期限の列で、完了の状態にある task を期限切れにしないことを確かめる。
// 頁の組み立ては task-list-split-focus.test.ts と同じ。
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
const todo = {
  id: 'status-todo',
  project_id: project.id,
  name: '未着手',
  color: '#64748b',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
};
const done = {
  ...todo,
  id: 'status-done',
  name: '完了',
  position: 1,
  is_default: false,
  is_done_state: true,
  is_default_done: true,
};

// 期限は遠い過去に置き、今が何日でも未完了の物は超過になるようにする
function makeTask(seq: number, statusId: string, softDeadline: string) {
  return {
    id: `task-${seq}`,
    seq_id: seq,
    title: `タスク ${seq}`,
    priority: 'Medium',
    status_id: statusId,
    project_id: project.id,
    soft_deadline: softDeadline,
    hard_deadline: null,
    is_archived: false,
    labels: [],
    progress_pct: 0,
    created_at: '2020-01-01T00:00:00Z',
    updated_at: '2020-01-01T00:00:00Z',
    created_by: { id: 'user-1', username: 'a', avatar_url: null },
    assignees: [],
  };
}
const projectBase = `/v1/tenants/${tenant.id}/projects/${project.id}`;

function stubFetch() {
  const tasks = [
    makeTask(1, done.id, '2020-01-10T00:00:00Z'),
    makeTask(2, todo.id, '2020-01-10T00:00:00Z'),
  ];
  const replies = new Map<string, unknown>([
    ['/v1/auth/me', { id: 'user-1', email: 'a@example.com', username: 'a', email_verified: true }],
    ['/v1/tenants', [tenant]],
    [`/v1/tenants/${tenant.id}/projects`, [project]],
    [`${projectBase}/statuses`, [todo, done]],
    [`${projectBase}/tasks`, { tasks, total: tasks.length, next_cursor: null }],
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

function mountTable() {
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
        TaskDetailPane: true,
      },
    },
  });
}

/** seq の task の行にある、期限の列の span を返す */
function deadlineCell(wrapper: ReturnType<typeof mountTable>, seqKey: string) {
  const link = wrapper.find(`a[data-task-seq-key="${seqKey}"]`);
  const row = link.element.closest('tr');
  expect(row, `${seqKey} の行が無い`).not.toBeNull();
  const span = [...row!.querySelectorAll('span')].find((el) =>
    /超過|日後|今日|\d+月\d+日/.test(el.textContent ?? ''),
  );
  expect(span, `${seqKey} の期限の列が無い`).toBeDefined();
  return span!;
}

describe('タスク一覧の表の期限の列', () => {
  it('完了の状態にある task は、過ぎた期限を赤くせず日付だけを出し、未完了の task は今までどおり超過と出す', async () => {
    stubFetch();
    const wrapper = mountTable();
    await vi.waitFor(() =>
      expect(wrapper.find('a[data-task-seq-key="ENG-2"]').exists()).toBe(true),
    );

    const doneCell = deadlineCell(wrapper, 'ENG-1');
    expect(doneCell.textContent).toBe('1月10日');
    expect(doneCell.classList.contains('text-red-500')).toBe(false);

    const todoCell = deadlineCell(wrapper, 'ENG-2');
    expect(todoCell.textContent).toMatch(/日超過$/);
    expect(todoCell.classList.contains('text-red-500')).toBe(true);
  });
});
