import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import { navigate } from 'vike/client/router';
import ProjectPage from '../+Page.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.mocked(navigate).mockClear();
});

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
    [
      `${projectBase}/tasks/search`,
      {
        tasks: [{ id: task.id, seq_id: task.seq_id, title: task.title, highlight: '', score: 1 }],
        total: 1,
      },
    ],
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

function stubNarrowScreen() {
  vi.stubGlobal('matchMedia', (media: string) => ({
    media,
    matches: false,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

function mountPage(search: Record<string, string>) {
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
          urlParsed: { search },
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

async function findTaskLink(wrapper: ReturnType<typeof mountPage>) {
  const link = () => wrapper.find('a[data-task-seq-key="ENG-1"]');
  await vi.waitFor(() => expect(link().exists()).toBe(true));
  return link().element;
}

/**
 * vike のリンク横取りは document の click で、defaultPrevented を見ない。そこまで届くと
 * 開いたオーバーレイごと詳細ページへ遷移するので、navigate のモックだけでは検出できない
 */
function click(link: Element) {
  const event = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
  const reachedDocument = vi.fn();
  document.addEventListener('click', reachedDocument);
  link.dispatchEvent(event);
  document.removeEventListener('click', reachedDocument);
  return { event, reachedDocument };
}

describe('タスク一覧のリンクからタスクを開く', () => {
  it('広い画面の List 表示で検索結果を開くと、オーバーレイに出してページ遷移しない', async () => {
    stubFetch();
    const wrapper = mountPage({ q: 'OAuth' });

    const { event, reachedDocument } = click(await findTaskLink(wrapper));

    expect(event.defaultPrevented).toBe(true);
    expect(reachedDocument).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
    await vi.waitFor(() => {
      const overlay = wrapper.getComponent({ name: 'TaskDetailOverlay' });
      expect(overlay.props('open')).toBe(true);
      expect(overlay.props('taskId')).toBe('ENG-1');
    });
  });

  it.each([
    ['検索結果', { view: 'table', q: 'OAuth' }],
    ['通常一覧', { view: 'table' }],
  ])('広い画面の Table 表示で%sを開くと、右ペインに出してページ遷移しない', async (_, search) => {
    stubFetch();
    const wrapper = mountPage(search);

    const { event, reachedDocument } = click(await findTaskLink(wrapper));

    expect(event.defaultPrevented).toBe(true);
    expect(reachedDocument).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
    await vi.waitFor(() =>
      expect(wrapper.getComponent({ name: 'TaskDetailPane' }).props('taskId')).toBe('ENG-1'),
    );
  });

  it('狭い画面で検索結果を開くと、詳細ページへ遷移してオーバーレイは出さない', async () => {
    stubNarrowScreen();
    stubFetch();
    const wrapper = mountPage({ q: 'OAuth' });

    click(await findTaskLink(wrapper));

    expect(navigate).toHaveBeenCalledExactlyOnceWith('/acme/projects/ENG/tasks/ENG-1');
    expect(wrapper.findComponent({ name: 'TaskDetailOverlay' }).exists()).toBe(false);
  });
});
