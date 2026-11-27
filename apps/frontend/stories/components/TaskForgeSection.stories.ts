import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import TaskForgeSection from '@/components/tasks/TaskForgeSection.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const pullRequest = (number: number, overrides: Record<string, unknown> = {}) => ({
  id: `00000000-0000-4000-8000-0000000000${number}`,
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  number,
  title: 'feat: OAuth 対応',
  author_handle: 'yupix',
  state: 'open',
  html_url: `https://github.com/acme/backend/pull/${number}`,
  merged_at: null,
  merge_effects_processed_at: null,
  commits_complete: false,
  sources: ['title'],
  closes: false,
  // 題の途中や本文で触れているだけの PR。実装の PR（題の末尾の (KEY-N) など）は題の上のピルに出る
  role: 'mention',
  head_branch: 'feat/oauth',
  unresolved: null,
  ...overrides,
});

const commit = (sha: string, message: string, overrides: Record<string, unknown> = {}) => ({
  id: `commit-${sha}`,
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  sha: sha.padEnd(40, '0'),
  message,
  body: '',
  author_handle: 'yupix',
  author_name: 'Yupix',
  committed_at: '2026-07-15T02:30:00Z',
  html_url: `https://github.com/acme/backend/commit/${sha}`,
  pull_requests: [],
  ...overrides,
});

const branch = {
  id: 'branch-1',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  branch: 'feat/TASK-1-oauth',
  html_url: 'https://github.com/acme/backend/tree/feat/TASK-1-oauth',
  created_at: '2026-07-12T03:00:00Z',
  pull_requests: [
    { number: 231, state: 'open', html_url: 'https://github.com/acme/backend/pull/231' },
  ],
};

const issue = {
  host: 'github',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 12,
  html_url: 'https://github.com/acme/backend/issues/12',
  state: 'open',
  last_synced_at: '2026-07-15T02:55:00Z',
  pending_push: false,
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;
/** 受けた POST の本文（Request の本文はモックが読むと再読できないので控える） */
let postedBodies: unknown[] = [];

/** forge（連携先を含む）をインメモリで返す fetch モック。POST で手動リンクを足し、DELETE で外す */
function mockFetch(
  options: {
    connected?: boolean;
    pullRequests?: ReturnType<typeof pullRequest>[];
    commits?: unknown[];
    branches?: unknown[];
    issue?: unknown;
  } = {},
) {
  return () => {
    const original = globalThis.fetch;
    postedBodies = [];
    let pullRequests = options.pullRequests ?? [
      pullRequest(87, { state: 'merged', sources: ['body', 'title'] }),
    ];
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      if (method === 'POST' && pathname.endsWith('/forge/pull-requests')) {
        const body = await (req as Request).json();
        postedBodies.push(body);
        const created = pullRequest(body.number, {
          state: 'unknown',
          title: '',
          author_handle: '',
          sources: ['manual'],
          closes: body.closes,
          // 手動リンクは「マージしたら完了にする」のチェックで役割が決まる
          role: body.closes ? 'implementation' : 'mention',
          head_branch: null,
        });
        pullRequests = [created, ...pullRequests];
        return jsonResponse(created, 201);
      }
      if (method === 'PUT' && pathname.endsWith('/role')) {
        const body = await (req as Request).json();
        postedBodies.push(body);
        const id = pathname.split('/').at(-2);
        pullRequests = pullRequests.map((pr) => (pr.id === id ? { ...pr, role: body.role } : pr));
        return jsonResponse(pullRequests.find((pr) => pr.id === id));
      }
      if (method === 'DELETE') {
        const id = pathname.split('/').pop();
        pullRequests = pullRequests.filter((pr) => pr.id !== id);
        return new Response(null, { status: 204 });
      }
      if (pathname.endsWith('/forge')) {
        return jsonResponse({
          pull_requests: pullRequests,
          commits: options.commits ?? [],
          branches: options.branches ?? [],
          issue: options.issue ?? null,
          repository:
            options.connected === false
              ? null
              : {
                  host: 'github',
                  host_url: 'https://github.com',
                  repo_owner: 'acme',
                  repo_name: 'backend',
                  html_url: 'https://github.com/acme/backend',
                },
        });
      }
      return jsonResponse({ message: 'not found' }, 404);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
    };
  };
}

const requestsOf = (method: string) =>
  (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .filter((req) => req.method === method);

const meta = {
  title: 'Components/Tasks/TaskForgeSection',
  component: TaskForgeSection,
  tags: ['autodocs'],
  args: {
    tenantId: TENANT_UUID,
    projectId: PROJECT_UUID,
    taskId: 'TASK-1',
    tenantDisplayId: 'acme',
    projectKey: 'TASK',
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細の「リポジトリ」セクション。タスクに触れているだけの PR・コミット・ブランチをタブで並べ、タブの行の右端に取り込み元を置く。実装の PR は題の上のピルに出すので、ここには出さない。PR の手動リンク・「実装 PR にする」・「リンクを外す」を fetch モックで検証。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({
            defaultOptions: {
              queries: { retry: false, gcTime: 0, staleTime: 0 },
              mutations: { retry: false },
            },
          }),
        );
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof TaskForgeSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LinkFlow: Story = {
  name: 'PR をリンク（ダイアログ → POST → 一覧反映）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    // 題は行とホバーカードの両方にあるので、行のリンクで引く
    await canvas.findByRole('link', { name: 'feat: OAuth 対応' });
    await user.click(canvas.getByRole('button', { name: 'PR をリンク' }));
    await expect(
      page.findByText('acme/backend の PR をこのタスクに結びます。'),
    ).resolves.toBeInTheDocument();
    await user.type(page.getByLabelText('PR 番号'), '91');
    await user.click(page.getByRole('button', { name: 'リンクする' }));

    // 「マージしたら完了にする」を入れずにリンクした PR は言及として一覧に出る
    await waitFor(() => expect(canvas.getAllByText('#91').length).toBeGreaterThan(0));
    await expect(postedBodies).toEqual([{ number: 91, closes: false }]);
    await expect(canvas.getByRole('tab', { name: 'Pull Requests 2' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  },
};

export const Tabs: Story = {
  name: 'タブ（Pull Requests / コミット / ブランチ）と取り込み元',
  beforeEach: mockFetch({
    pullRequests: [
      pullRequest(87, { state: 'merged', merge_effects_processed_at: '2026-07-10T00:00:00Z' }),
      pullRequest(92, { title: 'docs: OAuth の設定手順を足す', author_handle: 'sousuke' }),
      // 実装の PR は一覧に出ない
      pullRequest(231, { role: 'implementation', title: 'feat: OAuth ログイン (TASK-1)' }),
    ],
    commits: [
      commit('a3f92c1', 'fix: トークン期限切れを修正', {
        body: '期限の比較が逆だった。\n更新トークンも同じ判定に寄せる。',
        pull_requests: [
          { number: 231, state: 'open', html_url: 'https://github.com/acme/backend/pull/231' },
        ],
      }),
      commit('b17cd90', 'chore: 依存を上げる'),
    ],
    branches: [branch],
    issue,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    // 言及の PR が 2 件。実装の PR（#231）は数えない
    const pullRequests = await canvas.findByRole('tab', { name: 'Pull Requests 2' });
    await expect(pullRequests).toHaveAttribute('aria-selected', 'true');
    await expect(canvas.getByRole('link', { name: 'docs: OAuth の設定手順を足す' })).toBeVisible();
    await expect(canvas.queryByText('feat: OAuth ログイン (TASK-1)')).toBeNull();
    // 状態は色とアイコンで出す。文字は読み上げ専用
    const firstRow = canvasElement.querySelector('[data-forge-pull-request]')!;
    await expect(within(firstRow as HTMLElement).getAllByText('Merged')[0]).toHaveClass('sr-only');
    // 参照元の札は出さない
    await expect(canvas.queryByText('タイトル')).toBeNull();

    // 取り込み元はタブの行の右端
    const source = canvasElement.querySelector<HTMLElement>('[data-forge-issue-source]')!;
    await expect(within(source).getByRole('link', { name: 'acme/backend#12' })).toBeVisible();
    const tabs = canvas.getByRole('tablist').getBoundingClientRect();
    const sourceRect = source.getBoundingClientRect();
    await expect(sourceRect.left).toBeGreaterThan(tabs.right);
    await expect(sourceRect.top).toBeLessThan(tabs.bottom);
    await expect(sourceRect.bottom).toBeGreaterThan(tabs.top);

    await user.click(canvas.getByRole('tab', { name: 'コミット 2' }));
    await waitFor(() =>
      expect(canvas.getAllByText('fix: トークン期限切れを修正')[0]).toBeVisible(),
    );
    await expect(canvas.getByRole('link', { name: 'a3f92c1' })).toBeVisible();

    await user.click(canvas.getByRole('tab', { name: 'ブランチ 1' }));
    await expect(canvas.findByRole('link', { name: 'feat/TASK-1-oauth' })).resolves.toBeVisible();
    await expect(
      canvas.getByRole('button', { name: 'feat/TASK-1-oauth のリンクを外す' }),
    ).toBeVisible();
  },
};

// セクションはそれ自身の幅で並びを変える。狭い幅は物語の箱の幅で作る:
// VRT の撮り手は物語の globals.viewport を効かせず、窓の幅（1280）のまま描く
const NARROW_SECTION_PX = 320;

export const NarrowTabs: Story = {
  name: '狭い幅では取り込み元がタブの下へ回る',
  ...Tabs,
  decorators: [
    () => ({
      template: `<div data-narrow-box style="width: ${NARROW_SECTION_PX}px"><story /></div>`,
    }),
  ],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('tab', { name: 'Pull Requests 2' });
    const box = canvasElement.querySelector<HTMLElement>('[data-narrow-box]')!;
    const section = canvasElement.querySelector<HTMLElement>('[data-forge-tabs]')!;
    // 窓の幅によらず、狭い箱の中で撃っておる
    await expect(box.getBoundingClientRect().width).toBe(NARROW_SECTION_PX);
    const source = canvasElement.querySelector<HTMLElement>('[data-forge-issue-source]')!;
    const tabs = canvas.getByRole('tablist').getBoundingClientRect();
    await waitFor(() =>
      expect(source.getBoundingClientRect().top).toBeGreaterThanOrEqual(tabs.bottom),
    );
    // 箱から横にはみ出さない
    await expect(section.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right + 0.5,
    );
    await expect(source.scrollWidth).toBeLessThanOrEqual(source.clientWidth);
  },
};

export const SourceBesideTabsAtAnyWidth: Story = {
  name: '取り込み元は、セクションがどの幅でもタブに重ならない',
  beforeEach: mockFetch({
    // 件数が 2 桁のタブと、長い参照・反映待ち（縮まない文字）の組み合わせ
    pullRequests: Array.from({ length: 12 }, (_, i) => pullRequest(40 + i)),
    commits: Array.from({ length: 12 }, (_, i) => commit(`a3f92c${i}`, 'fix: 期限の比較')),
    branches: [branch],
    issue: {
      ...issue,
      repo_owner: 'koyori-application-platform',
      repo_name: 'task-tracker-backend-services',
      number: 12345,
      pending_push: true,
    },
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const tablist = await canvas.findByRole('tablist');
    const source = canvasElement.querySelector<HTMLElement>('[data-forge-issue-source]')!;
    const section = canvasElement.querySelector<HTMLElement>('[data-forge-tabs]')!;
    // 右ペインの最小（280）から、横に並ぶ幅の前後、広い幅まで
    for (const width of [280, 400, 520, 575, 576, 640, 960]) {
      canvasElement.style.width = `${width}px`;
      await waitFor(() => expect(section.getBoundingClientRect().width).toBeLessThanOrEqual(width));
      const tabs = tablist.getBoundingClientRect();
      const rect = source.getBoundingClientRect();
      const beside = rect.top < tabs.bottom && rect.bottom > tabs.top;
      // 横に並ぶならタブの右、並ばないならタブの下
      if (beside) await expect(rect.left, `幅 ${width}`).toBeGreaterThanOrEqual(tabs.right);
      else await expect(rect.top, `幅 ${width}`).toBeGreaterThanOrEqual(tabs.bottom);
      // 中身が取り込み元の枠からも、セクションからもはみ出さない
      await expect(source.scrollWidth, `幅 ${width}`).toBeLessThanOrEqual(source.clientWidth);
      await expect(rect.right, `幅 ${width}`).toBeLessThanOrEqual(
        section.getBoundingClientRect().right + 0.5,
      );
      // 行の中の反映待ち（ホバーカードの中のものではない）
      const pending = source.querySelector<HTMLElement>(':scope > .shrink-0')!;
      await expect(pending).toHaveTextContent('GitHub への反映待ち');
      await expect(pending).toBeVisible();
    }
    canvasElement.style.width = '';
  },
};

export const DefaultTabWithoutMentions: Story = {
  name: '言及の PR が無ければ、最初に件数のあるタブ（コミット）を開く',
  beforeEach: mockFetch({
    pullRequests: [pullRequest(231, { role: 'implementation' })],
    commits: [commit('a3f92c1', 'fix: トークン期限切れを修正')],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('tab', { name: 'コミット 1' })).resolves.toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(canvas.getByRole('tab', { name: 'Pull Requests 0' })).toHaveAttribute(
      'aria-selected',
      'false',
    );
  },
};

export const PullRequestHoverCard: Story = {
  name: 'PR のホバーカード（題・作者・ブランチ・未解決のレビュー）',
  beforeEach: mockFetch({
    pullRequests: [pullRequest(87, { state: 'open', unresolved: 2 })],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const link = await canvas.findByRole('link', { name: 'feat: OAuth 対応' });
    await userEvent.hover(link);
    const card = await page.findByRole('dialog', { name: '#87 の PR' });
    await waitFor(() => expect(within(card).getByText('feat/oauth')).toBeVisible());
    await expect(within(card).getByRole('link', { name: 'レビュー 2 件が未解決' })).toHaveAttribute(
      'href',
      '/acme/projects/TASK/reviews/87',
    );
    await expect(card.getBoundingClientRect().width).toBe(320);
    // 行の中には未解決の件数を出さない（カードへ回す）
    const row = canvasElement.querySelector('[data-forge-pull-request]')!;
    await expect(row.querySelectorAll('[data-forge-unresolved]')).toHaveLength(1);
  },
};

export const MakeImplementation: Story = {
  name: '実装 PR にする（PUT → 一覧から消える）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: '#87 の操作' }));
    await user.click(await page.findByRole('menuitem', { name: '実装 PR にする' }));

    await waitFor(() => expect(requestsOf('PUT')).toHaveLength(1));
    await expect(postedBodies).toEqual([{ role: 'implementation' }]);
    // 実装の PR は題の上のピルに出るので、この一覧からは消える
    await expect(canvas.findByText('触れているだけの PR はありません')).resolves.toBeVisible();
  },
};

export const NotConnected: Story = {
  name: '連携なし（ボタン無効 + 理由）',
  beforeEach: mockFetch({ connected: false }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('PR をリンクするには、プロジェクト設定でリポジトリを連携してください'),
    ).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'PR をリンク' })).toBeDisabled();
  },
};

export const UnlinkFlow: Story = {
  name: 'リンクを外す（確認ダイアログ → DELETE）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: '#87 の操作' }));
    await user.click(await page.findByRole('menuitem', { name: 'リンクを外す' }));
    await expect(page.findByText('PR のリンクを外しますか？')).resolves.toBeInTheDocument();
    await user.click(await page.findByRole('button', { name: 'リンクを外す' }));

    await waitFor(() => expect(requestsOf('DELETE')).toHaveLength(1));
    await expect(canvas.findByText('まだ PR もコミットもありません')).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('tab', { name: 'Pull Requests 0' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  },
};
