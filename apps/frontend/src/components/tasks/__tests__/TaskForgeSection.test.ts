import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import TaskForgeSection from '../TaskForgeSection.vue';

afterEach(() => {
  vi.unstubAllGlobals();
});

const pullRequest = {
  id: 'pr-1',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 87,
  title: 'feat: OAuth 対応 TASK-1',
  author_handle: 'yupix',
  state: 'merged',
  html_url: 'https://github.com/acme/backend/pull/87',
  merged_at: '2026-10-01T00:00:00Z',
  merge_effects_processed_at: null,
  commits_complete: false,
  sources: ['body', 'title'],
  closes: false,
  role: 'mention',
  head_branch: 'feat/oauth',
  unresolved: null,
};
// 題の末尾の (KEY-N) やクローズキーワードで、このタスクの実装と判定された PR
const implementation = { ...pullRequest, id: 'pr-impl', number: 231, role: 'implementation' };

const commit = {
  id: 'commit-1',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  sha: 'a3f92c1e7b81d4000000000000000000000000aa',
  message: 'fix: トークン期限切れを修正',
  author_handle: '',
  author_name: 'Yupix',
  committed_at: '2026-10-01T00:00:00Z',
  html_url: 'https://github.com/acme/backend/commit/a3f92c1',
  body: '',
  pull_requests: [],
};

const repository = {
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  html_url: 'https://github.com/acme/backend',
};

const branch = {
  id: 'branch-1',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  branch: 'feat/TASK-1-oauth',
  html_url: 'https://github.com/acme/backend/tree/feat/TASK-1-oauth',
  created_at: '2026-10-01T00:00:00Z',
  pull_requests: [],
};

const issue = {
  host: 'github',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 12,
  html_url: 'https://github.com/acme/backend/issues/12',
  state: 'open',
  last_synced_at: null,
  pending_push: false,
};

type Wrapper = Awaited<ReturnType<typeof mountWith>>;
const tabs = (wrapper: Wrapper) => wrapper.findAll('[role="tab"]');
const selectedTab = (wrapper: Wrapper) =>
  tabs(wrapper)
    .find((tab) => tab.attributes('aria-selected') === 'true')!
    .text();
/** 選んでいるタブの中身（ほかのタブの中身は hidden） */
const panel = (wrapper: Wrapper) =>
  wrapper.findAll('[role="tabpanel"]').find((el) => el.attributes('hidden') === undefined)!;
const menuItem = (root: { findAll: Wrapper['findAll'] }, text: string) =>
  root.findAll('[role="menuitem"]').find((item) => item.text() === text)!;

async function mountWith(forge: Record<string, unknown>) {
  vi.stubGlobal('fetch', async (request: Request | string) => {
    const url = typeof request === 'string' ? request : request.url;
    if (url.endsWith('/tasks/TASK-1/forge')) {
      return Response.json({ pull_requests: [], commits: [], branches: [], ...forge });
    }
    return Response.json({ message: 'not found' }, { status: 404 });
  });
  const wrapper = mount(TaskForgeSection, {
    props: {
      tenantId: 'tenant-1',
      projectId: 'project-1',
      taskId: 'TASK-1',
      tenantDisplayId: 'acme',
      projectKey: 'TASK',
    },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
  });
  await flushPromises();
  return wrapper;
}

describe('TaskForgeSection', () => {
  it('PR もコミットも無ければその旨を出し、Pull Requests のタブを開く', async () => {
    const wrapper = await mountWith({});
    expect(selectedTab(wrapper)).toBe('Pull Requests 0');
    expect(panel(wrapper).text()).toContain('まだ PR もコミットもありません');
    expect(wrapper.find('[data-forge-pull-request]').exists()).toBe(false);
  });

  it('タブに件数を出す（Pull Requests は言及だけの PR の数）', async () => {
    const wrapper = await mountWith({
      pull_requests: [pullRequest, implementation],
      commits: [commit, { ...commit, id: 'commit-2' }],
      branches: [branch],
    });
    expect(tabs(wrapper).map((tab) => tab.text())).toEqual([
      'Pull Requests 1',
      'コミット 2',
      'ブランチ 1',
    ]);
  });

  it('実装の PR は一覧に出さず、言及だけの PR を並べる', async () => {
    const wrapper = await mountWith({ pull_requests: [implementation, pullRequest] });

    const rows = wrapper.findAll('[data-forge-pull-request]');
    expect(rows).toHaveLength(1);
    expect(rows[0].text()).toContain('#87');
    expect(wrapper.text()).not.toContain('#231');
  });

  it('実装の PR しか無ければ、一覧は空で「触れているだけの PR はありません」を出す', async () => {
    const wrapper = await mountWith({ pull_requests: [implementation] });
    expect(panel(wrapper).get('[data-forge-empty]').text()).toBe(
      '触れているだけの PR はありません',
    );
  });

  it('PR の行は題・番号・作者を出し、参照元の札と状態の文字は出さない', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest] });
    const row = wrapper.get('[data-forge-pull-request]');

    const link = row.get('a');
    expect(link.text()).toBe('feat: OAuth 対応 TASK-1');
    expect(link.classes()).toContain('font-medium');
    expect(link.attributes('href')).toBe(pullRequest.html_url);
    expect(link.attributes('rel')).toBe('noopener noreferrer');
    expect(row.text()).toContain('#87');
    expect(row.text()).toContain('yupix');

    // 状態は色とアイコンで出す。名前は読み上げ専用
    expect(row.get('.sr-only').text()).toBe('Merged');
    const visible = row.element.cloneNode(true) as HTMLElement;
    for (const el of visible.querySelectorAll('.sr-only, [role="menu"], [popover]')) el.remove();
    for (const text of ['Merged', '本文', 'タイトル', '手動', 'コミット一覧は一部のみ']) {
      expect(visible.textContent).not.toContain(text);
    }
  });

  it('GitLab の PR は Merge Requests とタブに出す', async () => {
    const wrapper = await mountWith({ pull_requests: [{ ...pullRequest, host: 'gitlab' }] });
    expect(tabs(wrapper)[0].text()).toBe('Merge Requests 1');
  });

  it('状態が分からない PR も出し、http(s) 以外の URL はリンクにしない', async () => {
    const wrapper = await mountWith({
      pull_requests: [{ ...pullRequest, state: 'unknown', html_url: 'javascript:alert(1)' }],
    });
    const row = wrapper.get('[data-forge-pull-request]');
    expect(row.get('.sr-only').text()).toBe('状態不明');
    expect(row.find('a').exists()).toBe(false);
    expect(row.text()).toContain('feat: OAuth 対応 TASK-1');
  });

  describe('既定のタブ', () => {
    it.each([
      [
        '言及の PR があれば Pull Requests',
        { pull_requests: [pullRequest], commits: [commit] },
        'Pull Requests 1',
      ],
      [
        'PR が実装だけならコミット',
        { pull_requests: [implementation], commits: [commit] },
        'コミット 1',
      ],
      ['コミットも無ければブランチ', { branches: [branch] }, 'ブランチ 1'],
      ['全部 0 件なら Pull Requests', {}, 'Pull Requests 0'],
    ])('%s', async (_name, forge, expected) => {
      expect(selectedTab(await mountWith(forge))).toBe(expected);
    });

    it('利用者が選んだ後は、件数が変わっても動かさない', async () => {
      let forge: Record<string, unknown> = { commits: [commit] };
      vi.stubGlobal('fetch', async () =>
        Response.json({ pull_requests: [], commits: [], branches: [], ...forge }),
      );
      const queryClient = new QueryClient();
      const wrapper = mount(TaskForgeSection, {
        props: {
          tenantId: 'tenant-1',
          projectId: 'project-1',
          taskId: 'TASK-1',
          tenantDisplayId: 'acme',
          projectKey: 'TASK',
        },
        global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      });
      await flushPromises();
      expect(selectedTab(wrapper)).toBe('コミット 1');

      await tabs(wrapper)[2].trigger('click');
      expect(selectedTab(wrapper)).toBe('ブランチ 0');

      // 言及の PR が増えても、選んだタブのまま
      forge = { commits: [commit], pull_requests: [pullRequest] };
      await queryClient.invalidateQueries();
      await flushPromises();
      expect(selectedTab(wrapper)).toBe('ブランチ 0');
    });
  });

  it('コミットのタブは先頭行・短縮 SHA・作者（無ければ git の author 名）を出す', async () => {
    const wrapper = await mountWith({ commits: [commit] });
    expect(selectedTab(wrapper)).toBe('コミット 1');
    const row = wrapper.get('[data-forge-commit]');
    expect(row.get('a').text()).toBe('a3f92c1');
    expect(row.get('a').attributes('href')).toBe(commit.html_url);
    expect(row.get('.font-medium').text()).toBe('fix: トークン期限切れを修正');
    expect(row.text()).toContain('Yupix');
  });

  it('連携が無ければ「PR をリンク」を止めて理由を出す', async () => {
    const wrapper = await mountWith({ repository: null });
    const button = wrapper.get('button[aria-describedby="forge-link-disabled-reason"]');
    expect(button.text()).toContain('PR をリンク');
    expect(button.attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain('プロジェクト設定でリポジトリを連携してください');
  });

  it('連携していればボタンを止めない', async () => {
    const wrapper = await mountWith({ repository });
    const button = wrapper.findAll('button').find((b) => b.text().includes('PR をリンク'))!;
    expect(button.attributes('disabled')).toBeUndefined();
    expect(wrapper.text()).not.toContain('リポジトリを連携してください');
  });

  it('PR の行の操作メニューに「実装 PR にする」と「リンクを外す」を出す', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest], repository });
    const row = wrapper.get('[data-forge-pull-request]');
    expect(row.find('button[aria-label="#87 の操作"]').exists()).toBe(true);
    // Dropdown は空のときの文言も menuitem として持つ（項目があれば隠れる）
    expect(
      row
        .findAll('[role="menuitem"]')
        .map((item) => item.text())
        .filter((text) => text !== '項目がありません'),
    ).toEqual(['実装 PR にする', 'リンクを外す']);
  });

  it('PR の操作メニューはタスク詳細のスクロール領域で切れないようトップレイヤーに出す', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest], repository });
    const trigger = wrapper.get('button[aria-label="#87 の操作"]');
    expect(trigger.element.closest('[data-menu-top-layer]')).not.toBeNull();
  });

  describe('「実装 PR にする」', () => {
    /** PUT の応答を差し替えられる fetch。成功したら次の取得から役割が変わる */
    function stubRole(respond: () => Response) {
      const calls: { method: string; path: string; body: unknown }[] = [];
      let role = 'mention';
      vi.stubGlobal('fetch', async (request: Request) => {
        const path = new URL(request.url).pathname;
        calls.push({
          method: request.method,
          path,
          body: request.method === 'PUT' ? await request.clone().json() : null,
        });
        if (request.method === 'PUT') {
          const response = respond();
          if (response.ok) role = 'implementation';
          return response;
        }
        if (path.endsWith('/forge')) {
          return Response.json({
            pull_requests: [{ ...pullRequest, role }],
            commits: [],
            branches: [],
            repository,
          });
        }
        return Response.json({ items: [] });
      });
      return calls;
    }

    function mountSection(queryClient: QueryClient) {
      return mount(TaskForgeSection, {
        props: {
          tenantId: 'tenant-1',
          projectId: 'project-1',
          taskId: 'TASK-1',
          tenantDisplayId: 'acme',
          projectKey: 'TASK',
        },
        global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      });
    }

    it('PUT …/role を送り、forge とアクティビティを取り直す', async () => {
      const calls = stubRole(() => Response.json({ ...pullRequest, role: 'implementation' }));
      const queryClient = new QueryClient();
      const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = mountSection(queryClient);
      await flushPromises();

      await menuItem(wrapper.get('[data-forge-pull-request]'), '実装 PR にする').trigger('click');
      await flushPromises();

      expect(calls.filter((call) => call.method === 'PUT')).toEqual([
        {
          method: 'PUT',
          path: '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/forge/pull-requests/pr-1/role',
          body: { role: 'implementation' },
        },
      ]);
      expect(
        invalidate.mock.calls.map(([filters]) => (filters as { queryKey?: unknown }).queryKey),
      ).toEqual([
        ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge'],
        ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/activities'],
      ]);
      // 取り直した後は実装の PR なので、一覧から消える
      expect(wrapper.find('[data-forge-pull-request]').exists()).toBe(false);
      expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    });

    it('応答が返るまで表示は変えず、失敗したら一覧の下に出す', async () => {
      const calls = stubRole(() => Response.json({ message: 'not-found' }, { status: 404 }));
      const queryClient = new QueryClient();
      const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = mountSection(queryClient);
      await flushPromises();

      await menuItem(wrapper.get('[data-forge-pull-request]'), '実装 PR にする').trigger('click');
      // 楽観更新しない
      expect(wrapper.findAll('[data-forge-pull-request]')).toHaveLength(1);
      await flushPromises();

      expect(calls.some((call) => call.method === 'PUT')).toBe(true);
      expect(wrapper.get('[role="alert"]').text()).toBe('PR の役割を変えられませんでした');
      expect(wrapper.findAll('[data-forge-pull-request]')).toHaveLength(1);
      expect(invalidate).not.toHaveBeenCalled();
    });
  });

  it('ブランチだけでも空表示にせず、名前とホストのリンクを出す', async () => {
    const wrapper = await mountWith({ branches: [branch] });
    expect(wrapper.text()).not.toContain('まだ PR もコミットもありません');
    expect(selectedTab(wrapper)).toBe('ブランチ 1');
    const link = wrapper.get('[data-forge-branch] a');
    expect(link.text()).toBe('feat/TASK-1-oauth');
    expect(link.attributes('href')).toBe(branch.html_url);
  });

  it('解除するとリンクを外す API を呼び、一覧を取り直す', async () => {
    let branches = [branch];
    const calls: string[] = [];
    vi.stubGlobal('fetch', async (request: Request) => {
      calls.push(`${request.method} ${new URL(request.url).pathname}`);
      if (request.method === 'DELETE') {
        branches = [];
        return new Response(null, { status: 204 });
      }
      return Response.json({ pull_requests: [], commits: [], branches });
    });
    const wrapper = mount(TaskForgeSection, {
      props: {
        tenantId: 'tenant-1',
        projectId: 'project-1',
        taskId: 'TASK-1',
        tenantDisplayId: 'acme',
        projectKey: 'TASK',
      },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();

    await wrapper.get('button[aria-label="feat/TASK-1-oauth のリンクを外す"]').trigger('click');
    await flushPromises();

    expect(calls).toContain(
      'DELETE /api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/forge/branches/branch-1',
    );
    expect(wrapper.find('[data-forge-branch]').exists()).toBe(false);
  });

  it('取り込み元はタブの行（タブの器の中）に置く', async () => {
    const wrapper = await mountWith({ commits: [commit], issue });

    const source = wrapper.get('[data-forge-issue-source]');
    expect(source.get('a').text()).toBe('acme/backend#12');
    // タブと同じ器（relative）の中で、sm 以上は右端へ絶対配置する
    const container = wrapper.get('[role="tablist"]').element.closest('.relative')!;
    expect(container.contains(source.element)).toBe(true);
    expect(source.classes()).toEqual(expect.arrayContaining(['sm:absolute', 'sm:right-0']));
  });

  it('取り込み元が無ければ行を出さない', async () => {
    const wrapper = await mountWith({ commits: [commit] });
    expect(wrapper.find('[data-forge-issue-source]').exists()).toBe(false);
  });

  it('未解決のレビュー指摘の件数は行に出さず、ホバーカードへ回す', async () => {
    const wrapper = await mountWith({
      pull_requests: [
        { ...pullRequest, id: 'pr-1', number: 87, unresolved: 2 },
        { ...pullRequest, id: 'pr-2', number: 88, unresolved: 0 },
        { ...pullRequest, id: 'pr-3', number: 89, unresolved: null },
      ],
    });
    // カードの中身は HoverCard の panel に描かれる。レビューのある PR だけリンクを持つ
    const badges = wrapper.findAll('[data-forge-pull-request-card] [data-forge-unresolved]');
    expect(badges.map((b) => b.text())).toEqual(['レビュー 2 件が未解決', '未解決なし']);
    expect(badges[0]!.attributes('href')).toBe('/acme/projects/TASK/reviews/87');
    expect(badges[1]!.attributes('href')).toBe('/acme/projects/TASK/reviews/88');
    // 行そのもの（カードの外）には出さない
    for (const row of wrapper.findAll('[data-forge-pull-request]')) {
      const outside = row.element.cloneNode(true) as HTMLElement;
      for (const card of outside.querySelectorAll('[data-forge-pull-request-card]')) card.remove();
      expect(outside.textContent).not.toContain('未解決');
    }
  });
});
