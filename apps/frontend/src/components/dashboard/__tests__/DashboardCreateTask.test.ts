import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { TenantUuid } from '@/lib/api-ids';
import DashboardCreateTask from '../DashboardCreateTask.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
});
const tenantId = '00000000-0000-4000-8000-000000000001' as TenantUuid;
const project = {
  id: '00000000-0000-4000-8000-000000000002',
  tenant_id: tenantId,
  key: 'KOY',
  name: 'Koyori',
  description: '',
  icon_emoji: null,
  icon_url: null,
  is_personal: false,
  personal_owner_id: null,
  inherit_tenant_members: true,
};
const status = {
  id: 'todo',
  project_id: project.id,
  name: 'Todo',
  color: '#777777',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
  created_at: '2026-09-22T00:00:00Z',
};
const otherProject = {
  ...project,
  id: '00000000-0000-4000-8000-000000000003',
  key: 'OTH',
  name: 'Other',
};
function setup(
  response: (request: Request) => Response,
  extra: {
    currentUserId?: string;
    projects?: (typeof project)[];
    initialProjectId?: string;
  } = {},
) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => response(req)),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(DashboardCreateTask, {
    attachTo: document.body,
    props: { open: true, tenantId, projects: [project], ...extra },
    global: {
      plugins: [[VueQueryPlugin, { queryClient: client }]],
      stubs: { CreateTaskDialog: true },
    },
  });
}
const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });
const button = (text: string) =>
  [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === text)!;
describe('DashboardCreateTask', () => {
  it('プロジェクトの取得済みステータスを既存の作成画面へ渡す', async () => {
    const wrapper = setup((req) => json(req.url.endsWith('/statuses') ? [status] : []));
    await flushPromises();
    expect(button('次へ').disabled).toBe(false);
    button('次へ').click();
    await flushPromises();
    const editor = wrapper.getComponent({ name: 'CreateTaskDialog' });
    expect(editor.props()).toMatchObject({
      projectId: project.id,
      projectKey: 'KOY',
      tenantId,
      statuses: [status],
    });
    const task = { id: 't', seq_id: 12 };
    editor.vm.$emit('created', task, { keepOpen: false });
    await flushPromises();
    expect(wrapper.emitted('created')).toEqual([[task, project, { keepOpen: false }]]);
    expect(wrapper.emitted('update:open')?.at(-1)).toEqual([false]);
  });
  it('続けて作成するときは閉じずに created だけを上げる', async () => {
    const wrapper = setup((req) => json(req.url.endsWith('/statuses') ? [status] : []));
    await flushPromises();
    button('次へ').click();
    await flushPromises();
    const editor = wrapper.getComponent({ name: 'CreateTaskDialog' });
    const first = { id: 't1', seq_id: 12 };
    const second = { id: 't2', seq_id: 13 };
    editor.vm.$emit('created', first, { keepOpen: true });
    editor.vm.$emit('created', second, { keepOpen: true });
    await flushPromises();
    expect(wrapper.emitted('created')).toEqual([
      [first, project, { keepOpen: true }],
      [second, project, { keepOpen: true }],
    ]);
    expect(wrapper.emitted('update:open')).toBeUndefined();
    expect(wrapper.findComponent({ name: 'CreateTaskDialog' }).exists()).toBe(true);
  });
  it.each([
    ['一覧にある ID ならそのプロジェクト', otherProject.id, 'OTH'],
    ['一覧に無い ID なら先頭', 'missing', 'KOY'],
    ['未指定なら先頭', undefined, 'KOY'],
  ])('initialProjectId: %s を選ぶ', async (_, initialProjectId, expectedKey) => {
    const wrapper = setup((req) => json(req.url.endsWith('/statuses') ? [status] : []), {
      projects: [project, otherProject],
      initialProjectId,
    });
    await flushPromises();
    button('次へ').click();
    await flushPromises();
    expect(wrapper.getComponent({ name: 'CreateTaskDialog' }).props('projectKey')).toBe(
      expectedKey,
    );
  });
  it('現在のユーザーを担当者の初期値として渡す。未指定なら空', async () => {
    const respond = (req: Request) => json(req.url.endsWith('/statuses') ? [status] : []);
    const withMe = setup(respond, { currentUserId: 'me' });
    await flushPromises();
    button('次へ').click();
    await flushPromises();
    expect(withMe.getComponent({ name: 'CreateTaskDialog' }).props('initialAssigneeIds')).toEqual([
      'me',
    ]);
    withMe.unmount();
    document.body.innerHTML = '';

    const without = setup(respond);
    await flushPromises();
    button('次へ').click();
    await flushPromises();
    expect(without.getComponent({ name: 'CreateTaskDialog' }).props('initialAssigneeIds')).toEqual(
      [],
    );
  });
  it('ステータスの取得失敗を再試行でき、0件の場合は作成を始めない', async () => {
    let failed = true;
    setup((req) => (req.url.endsWith('/statuses') && failed ? json({}, 403) : json([])));
    await flushPromises();
    expect(document.body.textContent).toContain('ステータスを取得できませんでした');
    expect(button('次へ').disabled).toBe(true);
    failed = false;
    button('再試行').click();
    await flushPromises();
    expect(document.body.textContent).toContain('ステータスがありません');
    expect(button('次へ').disabled).toBe(true);
  });
});
