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
function setup(response: (request: Request) => Response) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => response(req)),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(DashboardCreateTask, {
    attachTo: document.body,
    props: { open: true, tenantId, projects: [project] },
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
    editor.vm.$emit('created', {});
    await flushPromises();
    expect(wrapper.emitted('created')).toHaveLength(1);
    expect(wrapper.emitted('update:open')?.at(-1)).toEqual([false]);
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
