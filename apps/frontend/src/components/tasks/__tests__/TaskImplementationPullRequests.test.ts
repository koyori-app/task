import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { defineComponent, h } from 'vue';

import TaskForgeSection from '../TaskForgeSection.vue';
import TaskImplementationPullRequests from '../TaskImplementationPullRequests.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const pullRequest = (overrides: Record<string, unknown> = {}) => ({
  id: 'pr-231',
  project_id: 'project-1',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 231,
  title: 'feat: タスク詳細のリデザイン (TASK-1)',
  author_handle: 'yupix',
  state: 'open',
  html_url: 'https://github.com/acme/backend/pull/231',
  merged_at: null,
  merge_effects_processed_at: null,
  commits_complete: true,
  sources: ['title'],
  closes: false,
  role: 'implementation',
  head_branch: 'feat/task-detail',
  unresolved: 2,
  ...overrides,
});

const props = {
  tenantId: 'tenant-1',
  projectId: 'project-1',
  taskId: 'TASK-1',
  tenantDisplayId: 'acme',
  projectKey: 'TASK',
};

/** forge の応答を持つ fetch。書き込みは `write` で受ける */
function stubForge(
  pullRequests: () => object[],
  write: (request: Request) => Response = () => new Response(null, { status: 204 }),
) {
  const calls: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal('fetch', async (request: Request) => {
    const path = new URL(request.url).pathname;
    const body = request.method === 'PUT' ? await request.clone().json() : null;
    calls.push({ method: request.method, path, body });
    if (request.method !== 'GET') return write(request);
    if (path.endsWith('/forge')) {
      return Response.json({ pull_requests: pullRequests(), commits: [], branches: [] });
    }
    return Response.json({ items: [] });
  });
  return calls;
}

async function mountPills(
  queryClient = new QueryClient(),
  component = TaskImplementationPullRequests,
) {
  // 複数の根を持つ（メタ行の flex にピルを直に並べる）ので、器で包んで引く
  const Host = defineComponent({
    render: () => h('div', { 'data-host': '' }, [h(component, props)]),
  });
  const wrapper = mount(Host, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

type Wrapper = Awaited<ReturnType<typeof mountPills>>;
const pills = (wrapper: Wrapper) => wrapper.findAll('[data-implementation-pull-request]');
const menuItems = (root: { findAll: Wrapper['findAll'] }) =>
  root
    .findAll('[role="menuitem"]')
    .map((item) => item.text())
    .filter((text) => text !== '項目がありません');
const menuItem = (root: { findAll: Wrapper['findAll'] }, text: string) =>
  root.findAll('[role="menuitem"]').find((item) => item.text() === text)!;

describe('TaskImplementationPullRequests', () => {
  it('implementation の PR だけがピルになる（状態のアイコンと番号。状態の文字は読み上げ専用）', async () => {
    stubForge(() => [
      pullRequest(),
      pullRequest({ id: 'pr-87', number: 87, role: 'mention' }),
      pullRequest({ id: 'pr-240', number: 240, state: 'merged', merge_effects_processed_at: 'x' }),
    ]);
    const wrapper = await mountPills();

    expect(pills(wrapper)).toHaveLength(2);
    const triggers = pills(wrapper).map((pill) => pill.get('button[aria-label$="の操作"]'));
    expect(triggers.map((trigger) => trigger.text().replace(/\s+/g, ''))).toEqual([
      'Open#231',
      'Merged#240',
    ]);
    expect(triggers[0].get('.sr-only').text()).toBe('Open');
    expect(wrapper.text()).not.toContain('#87');
  });

  it('0 件は何も描かない', async () => {
    stubForge(() => [pullRequest({ role: 'mention' })]);
    const wrapper = await mountPills();

    expect(pills(wrapper)).toHaveLength(0);
    const host = wrapper.get('[data-host]').element.cloneNode(true) as HTMLElement;
    for (const dialog of host.querySelectorAll('dialog')) dialog.remove();
    expect(host.textContent!.trim()).toBe('');
  });

  it('ホバーカードに PR の題・作者・ブランチ・未解決のレビューを出す', async () => {
    stubForge(() => [pullRequest()]);
    const card = (await mountPills()).get('[data-forge-pull-request-card]');

    expect(card.text()).toContain('feat: タスク詳細のリデザイン (TASK-1)');
    expect(card.text()).toContain('yupix');
    expect(card.text()).toContain('feat/task-detail');
    const review = card.get('[data-forge-unresolved]');
    expect(review.text()).toBe('レビュー 2 件が未解決');
    expect(review.attributes('href')).toBe('/acme/projects/TASK/reviews/231');
  });

  it('レビューの無い PR のカードは区切りから下を出さず、ブランチ名の無い PR はブランチを出さない', async () => {
    stubForge(() => [pullRequest({ unresolved: null, head_branch: null })]);
    const card = (await mountPills()).get('[data-forge-pull-request-card]');

    expect(card.find('[data-forge-unresolved]').exists()).toBe(false);
    expect(card.find('.border-t').exists()).toBe(false);
    expect(card.text()).not.toContain('feat/task-detail');
  });

  it('メニューは ホストで開く・言及に戻す・リンクを外す', async () => {
    stubForge(() => [pullRequest()]);
    const wrapper = await mountPills();

    expect(menuItems(pills(wrapper)[0])).toEqual(['GitHub で開く', '言及に戻す', 'リンクを外す']);

    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    await menuItem(wrapper, 'GitHub で開く').trigger('click');
    expect(open).toHaveBeenCalledWith(
      'https://github.com/acme/backend/pull/231',
      '_blank',
      'noopener,noreferrer',
    );
  });

  it('http(s) でない URL の PR には「開く」を出さない', async () => {
    stubForge(() => [pullRequest({ html_url: 'javascript:alert(1)' })]);
    expect(menuItems(await mountPills())).toEqual(['言及に戻す', 'リンクを外す']);
  });

  it('「言及に戻す」は PUT …/role を送り、取り直すとピルが消える', async () => {
    let role = 'implementation';
    const calls = stubForge(
      () => [pullRequest({ role })],
      () => {
        role = 'mention';
        return Response.json(pullRequest({ role }));
      },
    );
    const queryClient = new QueryClient();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    const wrapper = await mountPills(queryClient);

    await menuItem(wrapper, '言及に戻す').trigger('click');
    // 応答まで表示は変えない
    expect(pills(wrapper)).toHaveLength(1);
    await flushPromises();

    expect(calls.filter((call) => call.method === 'PUT')).toEqual([
      {
        method: 'PUT',
        path: '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/forge/pull-requests/pr-231/role',
        body: { role: 'mention' },
      },
    ]);
    expect(
      invalidate.mock.calls.map(([filters]) => (filters as { queryKey?: unknown }).queryKey),
    ).toEqual([
      ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge'],
      ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/activities'],
    ]);
    expect(pills(wrapper)).toHaveLength(0);
  });

  it('役割を変えられなければピルを残して知らせる', async () => {
    stubForge(
      () => [pullRequest()],
      () => Response.json({ message: 'forbidden' }, { status: 403 }),
    );
    const wrapper = await mountPills();

    await menuItem(wrapper, '言及に戻す').trigger('click');
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe('PR の役割を変えられませんでした');
    expect(pills(wrapper)).toHaveLength(1);
  });

  it('「リンクを外す」は確認してから DELETE を送る', async () => {
    let linked = true;
    const calls = stubForge(
      () => (linked ? [pullRequest()] : []),
      () => {
        linked = false;
        return new Response(null, { status: 204 });
      },
    );
    const wrapper = await mountPills();

    await menuItem(wrapper, 'リンクを外す').trigger('click');
    expect(calls.some((call) => call.method === 'DELETE')).toBe(false);
    expect(wrapper.text()).toContain('#231 とこのタスクのリンクをすべて外します');

    await wrapper
      .findAll('button')
      // メニューの項目ではなく、確認のダイアログのボタン
      .find((button) => button.text() === 'リンクを外す' && !button.attributes('role'))!
      .trigger('click');
    await flushPromises();

    expect(calls.filter((call) => call.method === 'DELETE').map((call) => call.path)).toEqual([
      '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/forge/pull-requests/pr-231',
    ]);
    expect(pills(wrapper)).toHaveLength(0);
  });

  it('自動完了が未適用の PR はピルに点を付け、メニューから確認を開ける', async () => {
    stubForge(() => [
      pullRequest({ state: 'merged', merged_at: '2026-10-01T00:00:00Z' }),
      pullRequest({ id: 'pr-240', number: 240 }),
    ]);
    const wrapper = await mountPills();

    const [pending, plain] = pills(wrapper);
    expect(pending.find('[data-merge-effects-dot]').exists()).toBe(true);
    expect(plain.find('[data-merge-effects-dot]').exists()).toBe(false);
    expect(menuItems(pending)).toContain('自動完了の未適用を確認');
    expect(menuItems(plain)).not.toContain('自動完了の未適用を確認');
    // 札そのものは描かない（ダイアログだけを持つ）
    expect(wrapper.find('[data-merge-effects-badge]').exists()).toBe(false);
    expect(pending.get('[data-forge-card-merge-effects]').text()).toBe(
      'マージ済み（自動完了は未適用）',
    );

    await menuItem(pending, '自動完了の未適用を確認').trigger('click');
    expect(wrapper.text()).toContain('#231 の自動完了は未適用です');
  });

  it('リポジトリ欄と同じクエリキーを使い、forge の取得は 1 回で済む', async () => {
    const calls = stubForge(() => [pullRequest()]);
    const Both = defineComponent({
      props: Object.keys(props),
      setup: (p) => () => [
        h(TaskImplementationPullRequests, p as never),
        h(TaskForgeSection, p as never),
      ],
    });
    const wrapper = await mountPills(new QueryClient(), Both as never);

    expect(calls.filter((call) => call.path.endsWith('/forge'))).toHaveLength(1);
    expect(pills(wrapper)).toHaveLength(1);
    // 実装の PR はリポジトリ欄の一覧には出ない
    expect(wrapper.find('[data-forge-pull-request]').exists()).toBe(false);
  });
});
