import { afterEach, it, expect, vi } from 'vitest';
import { defineComponent, ref } from 'vue';
import { mount, enableAutoUnmount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { useTaskRefCard } from '../useTaskRefCard';
import { fetchClient } from '@/lib/api-vue-query';
import { GET_TASK_PATH } from '../useTaskDetail';
import type { TenantUuid } from '@/lib/api-ids';
import type { TaskRefCardTarget } from '@/lib/task-ref-card';
enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
const tenantId = '11111111-1111-4111-8111-111111111111' as TenantUuid;
const target = { projectKey: 'TASK', seqKey: 'TASK-58', href: '/acme/projects/TASK/tasks/TASK-58' };
const task = { title: '確認', status_id: 'status', assignees: [] };
function setup(code = 200, found = true, statusCode = 200, cached = false) {
  const fetch = vi.fn(async (req: Request) => {
    const json = (value: unknown, status = 200) =>
      new Response(JSON.stringify(value), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    if (req.url.endsWith('/projects'))
      return json(found ? [{ id: 'project', key: 'TASK', name: '開発' }] : []);
    if (req.url.endsWith('/statuses'))
      return json(
        statusCode === 200
          ? [{ id: 'status', name: '完了', color: '#123456', is_done_state: true }]
          : { message: 'failure' },
        statusCode,
      );
    return json(code === 200 ? task : { message: 'failure' }, code);
  });
  vi.stubGlobal('fetch', fetch);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (cached)
    client.setQueryData(
      [
        'get',
        GET_TASK_PATH,
        { params: { path: { tenant_id: tenantId, project_id: 'project', id: 'TASK-58' } } },
      ],
      task,
    );
  const current = ref<TaskRefCardTarget | null>(target);
  let card!: ReturnType<typeof useTaskRefCard>;
  mount(
    defineComponent({
      setup() {
        card = useTaskRefCard(tenantId, current);
        return () => null;
      },
    }),
    { global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } },
  );
  return { card, fetch, current };
}
it.each([
  [200, 'ready'],
  [403, 'unavailable'],
  [404, 'unavailable'],
  [500, 'error'],
] as const)('タスク %i は %s', async (code, state) => {
  const { card } = setup(code);
  await vi.waitFor(() => expect(card.state.value).toBe(state));
  if (code === 200) {
    expect(card.projectName.value).toBe('開発');
    await vi.waitFor(() =>
      expect(card.status.value).toEqual({ name: '完了', color: '#123456', done: true }),
    );
  }
});
it('一覧にないキーではタスクを取得しない', async () => {
  const { card, fetch } = setup(200, false);
  await vi.waitFor(() => expect(card.state.value).toBe('unavailable'));
  expect(fetch.mock.calls.some(([r]) => r.url.includes('/tasks/'))).toBe(false);
});
it('ステータス取得失敗でもタスクを表示する', async () => {
  const { card } = setup(200, true, 500);
  await vi.waitFor(() => expect(card.state.value).toBe('ready'));
  expect(card.status.value).toBeNull();
});
it('詳細のキャッシュを再利用する', async () => {
  const { card, fetch } = setup(200, true, 200, true);
  await vi.waitFor(() => expect(card.state.value).toBe('ready'));
  expect(fetch.mock.calls.some(([r]) => r.url.includes('/tasks/'))).toBe(false);
});
it('閉じても取得を中断しない', async () => {
  const getSpy = vi.spyOn(fetchClient, 'GET');
  const { card, fetch, current } = setup();
  await vi.waitFor(() => expect(card.state.value).toBe('ready'));
  const request = fetch.mock.calls.find(([r]) => r.url.includes('/tasks/'))![0];
  current.value = null;
  await vi.waitFor(() => expect(card.state.value).toBe('loading'));
  expect(request.signal.aborted).toBe(false);
  const calls = getSpy.mock.calls as unknown as [string, Record<string, unknown>][];
  const taskOptions = calls.find(([path]) => path === GET_TASK_PATH)![1];
  expect(taskOptions).not.toHaveProperty('signal');
});
