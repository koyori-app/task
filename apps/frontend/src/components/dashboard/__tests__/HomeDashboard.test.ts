import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import HomeDashboard from '../HomeDashboard.vue';
import type { TenantUuid } from '@/lib/api-ids';
import type { components } from '@/generated/api';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());
const tenant = '00000000-0000-4000-8000-000000000001' as TenantUuid;
const project = {
  id: '00000000-0000-4000-8000-000000000002',
  tenant_id: tenant,
  key: 'KOY',
  name: 'Koyori',
  description: '',
  icon_emoji: null,
  icon_url: null,
  is_personal: false,
  personal_owner_id: null,
};
function fixture(): components['schemas']['DashboardResponse'] {
  return {
    counts: { today: 12, week: 14, overdue: 2, open: 20, completed_week: 8 },
    days: Array.from({ length: 7 }, (_, i) => ({
      date: `2026-09-${21 + i}`,
      count: i === 0 ? 5 : i === 1 ? 3 : 0,
    })),
    total: 12,
    tasks: [
      {
        task: {
          id: 'task-1',
          seq_id: 1,
          seq_key: 'KOY-1',
          title: 'レイアウトを整える',
          project,
          status: { id: 'todo', name: 'Todo', color: '#778899' },
          priority: 'High',
          soft_deadline: '2026-09-22T00:00:00Z',
          hard_deadline: null,
          is_personal: false,
        },
        done_status_id: 'done',
      },
    ],
    projects: [{ project, total: 50, completed: 34 }],
    activities: [],
  };
}
function setup(handler: (request: Request) => Response | Promise<Response>) {
  const fetch = vi.fn(handler);
  vi.stubGlobal('fetch', fetch);
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = mount(HomeDashboard, {
    props: { tenantId: tenant, tenantSlug: 'acme' },
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient: client }]],
      stubs: { DashboardCreateTask: true },
    },
  });
  return { wrapper, fetch, client };
}
const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });
function respond(request: Request, data: components['schemas']['DashboardResponse']) {
  return json(request.url.includes('/auth/me') ? { id: 'user', username: 'yupix' } : data);
}

describe('HomeDashboard', () => {
  it('サーバーの集計件数を表示し、表示中の行だけで数え直さない', async () => {
    const data = fixture();
    const { wrapper, fetch } = setup((req) => respond(req, data));
    await flushPromises();
    expect(wrapper.text()).toContain('yupixさん');
    expect(wrapper.get('[aria-label="自分のタスクの概要"]').text()).toContain('12');
    expect(wrapper.text()).toContain('34 / 50 タスク完了');
    expect(wrapper.get('a[href="/acme/projects/KOY/tasks/KOY-1"]').text()).toBe(
      'レイアウトを整える',
    );
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '次へ')!
      .trigger('click');
    await flushPromises();
    expect(
      fetch.mock.calls.some(([req]) => new URL(req.url).searchParams.get('offset') === '5'),
    ).toBe(true);
    const overdue = wrapper
      .get('[aria-label="タスクの絞り込み"]')
      .findAll('button')
      .find((b) => b.text().startsWith('期限超過'))!;
    await overdue.trigger('click');
    await flushPromises();
    expect(
      fetch.mock.calls.some(
        ([req]) =>
          new URL(req.url).searchParams.get('filter') === 'overdue' &&
          new URL(req.url).searchParams.get('offset') === '0',
      ),
    ).toBe(true);
  });

  it('絞り込み中はタブと概要を保持し、前の条件のタスクを操作させない', async () => {
    let finish: ((response: Response) => void) | undefined;
    const { wrapper } = setup((req) =>
      new URL(req.url).searchParams.get('filter') === 'overdue'
        ? new Promise<Response>((resolve) => {
            finish = resolve;
          })
        : respond(req, fixture()),
    );
    await flushPromises();
    const tab = wrapper.get('[aria-label="タスクの絞り込み"]').findAll('button')[2]!;
    await tab.trigger('click');
    await flushPromises();
    expect(wrapper.find('[aria-label="自分のタスクの概要"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('タスクを読み込み中…');
    expect(wrapper.find('button[aria-label$="を完了にする"]').exists()).toBe(false);
    finish!(json({ ...fixture(), tasks: [], total: 0 }));
    await flushPromises();
    expect(wrapper.text()).toContain('この条件の未完了タスクはありません');
  });

  it('完了成功後に件数と一覧を再取得する', async () => {
    const data = fixture();
    const { wrapper, fetch } = setup((req) => {
      if (req.method === 'PUT') {
        data.tasks = [];
        data.total = 0;
        data.counts.today = 0;
        data.counts.completed_week = 9;
        return json({});
      }
      return respond(req, data);
    });
    await flushPromises();
    await wrapper.get('button[aria-label="レイアウトを整えるを完了にする"]').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('今日が期限の未完了タスクはありません。');
    const req = fetch.mock.calls.map((c) => c[0]).find((req) => req.method === 'PUT')!;
    expect(req.url).toContain(`/tenants/${tenant}/projects/${project.id}/tasks/task-1`);
    expect(await req.json()).toEqual({ status_id: 'done' });
    expect(wrapper.get('[aria-label="自分のタスクの概要"]').text()).toContain('9');
  });

  it('更新の拒否を表示し、元のタスクと件数を保持する', async () => {
    const { wrapper } = setup((req) =>
      req.method === 'PUT' ? json({ message: 'Forbidden' }, 403) : respond(req, fixture()),
    );
    await flushPromises();
    await wrapper.get('button[aria-label="レイアウトを整えるを完了にする"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('完了にできませんでした');
    expect(wrapper.find('a[href="/acme/projects/KOY/tasks/KOY-1"]').exists()).toBe(true);
    expect(
      wrapper.get('button[aria-label="レイアウトを整えるを完了にする"]').attributes('disabled'),
    ).toBeUndefined();
  });

  it('取得失敗を0件として扱わず再試行できる', async () => {
    let failed = true;
    const { wrapper } = setup((req) =>
      req.url.includes('/dashboard') && failed ? json({}, 503) : respond(req, fixture()),
    );
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('取得できませんでした');
    expect(wrapper.find('[aria-label="自分のタスクの概要"]').exists()).toBe(false);
    failed = false;
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '再試行')!
      .trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('レイアウトを整える');
  });

  it('背景の再取得に失敗しても作成中のダイアログを閉じない', async () => {
    let failed = false;
    const { wrapper, client } = setup((req) =>
      req.url.includes('/dashboard') && failed ? json({}, 503) : respond(req, fixture()),
    );
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((b) => b.text() === 'タスクを作成')!
      .trigger('click');
    failed = true;
    await client.invalidateQueries({
      queryKey: ['get', '/v1/tenants/{tenant_id}/users/me/dashboard'],
    });
    await flushPromises();
    expect(wrapper.get('[role=alert]').text()).toContain('取得できませんでした');
    expect(wrapper.getComponent({ name: 'DashboardCreateTask' }).props('open')).toBe(true);
    expect(wrapper.text()).toContain('レイアウトを整える');
  });

  it('0件・完了ステータスなし・HTMLを含む名前を安全に表示する', async () => {
    const data = fixture();
    data.tasks[0]!.done_status_id = null;
    data.tasks[0]!.task.title = '<img src=x onerror=alert(1)>';
    const { wrapper } = setup((req) => respond(req, data));
    await flushPromises();
    expect(wrapper.text()).toContain('<img src=x onerror=alert(1)>');
    expect(wrapper.find('img[src="x"]').exists()).toBe(false);
    expect(wrapper.get('button[aria-label$="を完了にする"]').attributes('disabled')).toBeDefined();
    data.tasks = [];
    data.projects = [];
    data.counts = { today: 0, week: 0, overdue: 0, open: 0, completed_week: 0 };
    data.total = 0;
    await wrapper.get('[aria-label="タスクの絞り込み"]').findAll('button')[1]!.trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('この条件の未完了タスクはありません');
    expect(wrapper.text()).not.toContain('NaN');
    expect(
      wrapper
        .findAll('button')
        .find((b) => b.text() === 'タスクを作成')!
        .attributes('disabled'),
    ).toBeDefined();
  });
});
