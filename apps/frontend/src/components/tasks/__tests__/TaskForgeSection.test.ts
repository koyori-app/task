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

async function mountWith(forge: Record<string, unknown>, integration?: Record<string, unknown>) {
  vi.stubGlobal('fetch', async (request: Request | string) => {
    const url = typeof request === 'string' ? request : request.url;
    if (url.endsWith('/tasks/TASK-1/forge')) {
      return Response.json({ pull_requests: [], commits: [], branches: [], ...forge });
    }
    if (integration && url.endsWith('/github/integration')) {
      return Response.json(integration);
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
    const wrapper = await mountWith({}, { connected: false });
    const button = wrapper.get('button[aria-describedby="forge-link-disabled-reason"]');
    expect(button.text()).toContain('PR をリンク');
    expect(button.attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain('プロジェクト設定でリポジトリを連携してください');
  });

  it('連携先を読めない（オーナー以外）ときはボタンを止めない', async () => {
    const wrapper = await mountWith({});
    const button = wrapper.findAll('button').find((b) => b.text().includes('PR をリンク'))!;
    expect(button.attributes('disabled')).toBeUndefined();
    expect(wrapper.text()).not.toContain('リポジトリを連携してください');
  });

  it('PR の行に操作メニューを出す', async () => {
    const wrapper = await mountWith({ pull_requests: [pullRequest] }, { connected: true });
    expect(
      wrapper.get('[data-forge-pull-request]').find('button[aria-label="#87 の操作"]').exists(),
    ).toBe(true);
  });
});
