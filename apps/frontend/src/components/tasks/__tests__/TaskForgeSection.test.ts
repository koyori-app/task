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
  closes: true,
  unresolved: null,
};

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
};

async function mountWith(forge: Record<string, unknown>) {
  vi.stubGlobal('fetch', async (request: Request | string) => {
    const url = typeof request === 'string' ? request : request.url;
    if (url.endsWith('/tasks/TASK-1/forge')) {
      return Response.json({ pull_requests: [], commits: [], branches: [], ...forge });
    }
    return Response.json({ message: 'not found' }, { status: 404 });
  });
  const wrapper = mount(TaskForgeSection, {
    props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
    global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
  });
  await flushPromises();
  return wrapper;
}

describe('TaskForgeSection', () => {
  it('PR もコミットも無ければその旨を出す', async () => {
    const wrapper = await mountWith({});
    expect(wrapper.text()).toContain('まだ PR もコミットもありません');
    expect(wrapper.find('[data-forge-pull-request]').exists()).toBe(false);
  });

  it('PR だけなら状態・番号・タイトル・参照元・作者を出し、PR へリンクする', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest] });
    const row = wrapper.get('[data-forge-pull-request]');
    expect(wrapper.text()).toContain('Pull Requests');
    expect(wrapper.text()).not.toContain('Commits');
    expect(row.text()).toContain('Merged');
    expect(row.text()).toContain('#87');
    expect(row.text()).toContain('feat: OAuth 対応 TASK-1');
    expect(row.text()).toContain('本文');
    expect(row.text()).toContain('タイトル');
    expect(row.text()).toContain('yupix');
    const link = row.get('a');
    expect(link.attributes('href')).toBe(pullRequest.html_url);
    expect(link.attributes('rel')).toBe('noopener noreferrer');
  });

  it('GitLab の PR は Merge Requests と見出しを出す', async () => {
    const wrapper = await mountWith({ pull_requests: [{ ...pullRequest, host: 'gitlab' }] });
    expect(wrapper.text()).toContain('Merge Requests');
  });

  it('状態が分からない PR も出し、http(s) 以外の URL はリンクにしない', async () => {
    const wrapper = await mountWith({
      pull_requests: [{ ...pullRequest, state: 'unknown', html_url: 'javascript:alert(1)' }],
    });
    const row = wrapper.get('[data-forge-pull-request]');
    expect(row.text()).toContain('状態不明');
    expect(row.find('a').exists()).toBe(false);
  });

  it('コミットだけなら短縮 SHA・先頭行・作者（無ければ git の author 名）を出す', async () => {
    const wrapper = await mountWith({ commits: [commit] });
    expect(wrapper.text()).not.toContain('Pull Requests');
    const row = wrapper.get('[data-forge-commit]');
    expect(row.get('a').text()).toBe('a3f92c1');
    expect(row.get('a').attributes('href')).toBe(commit.html_url);
    expect(row.text()).toContain('fix: トークン期限切れを修正');
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

  it('PR の行に操作メニューを出す', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest], repository });
    expect(
      wrapper.get('[data-forge-pull-request]').find('button[aria-label="#87 の操作"]').exists(),
    ).toBe(true);
  });

  it('コミット一覧を確定できていない PR にだけ「一部のみ」を添える', async () => {
    const wrapper = await mountWith({
      pull_requests: [
        pullRequest,
        { ...pullRequest, id: 'pr-2', number: 91, commits_complete: true },
      ],
    });
    const rows = wrapper.findAll('[data-forge-pull-request]');
    expect(rows[0]!.text()).toContain('コミット一覧は一部のみ');
    expect(rows[1]!.text()).not.toContain('コミット一覧は一部のみ');
  });

  it('ブランチだけでも空表示にせず、名前とホストのリンクを出す', async () => {
    const wrapper = await mountWith({ branches: [branch] });
    expect(wrapper.text()).not.toContain('まだ PR もコミットもありません');
    expect(wrapper.text()).toContain('Branches');
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
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
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
});
