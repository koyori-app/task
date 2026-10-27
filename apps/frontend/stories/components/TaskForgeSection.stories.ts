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
  unresolved: null,
  ...overrides,
});

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;
/** 受けた POST の本文（Request の本文はモックが読むと再読できないので控える） */
let postedBodies: unknown[] = [];

/** forge（連携先を含む）をインメモリで返す fetch モック。POST で手動リンクを足し、DELETE で外す */
function mockFetch(options: { connected?: boolean } = {}) {
  return () => {
    const original = globalThis.fetch;
    postedBodies = [];
    let pullRequests = [pullRequest(87, { state: 'merged', sources: ['body', 'title'] })];
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
        });
        pullRequests = [created, ...pullRequests];
        return jsonResponse(created, 201);
      }
      if (method === 'DELETE') {
        const id = pathname.split('/').pop();
        pullRequests = pullRequests.filter((pr) => pr.id !== id);
        return new Response(null, { status: 204 });
      }
      if (pathname.endsWith('/forge')) {
        return jsonResponse({
          pull_requests: pullRequests,
          commits: [],
          branches: [],
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
          'タスク詳細の「リポジトリ」セクション。PR の手動リンク（ダイアログ）と「リンクを外す」（確認ダイアログ）。fetch モックで検証。',
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
    await canvas.findByText('feat: OAuth 対応', { exact: false });
    await user.click(canvas.getByRole('button', { name: 'PR をリンク' }));
    await expect(
      page.findByText('acme/backend の PR をこのタスクに結びます。'),
    ).resolves.toBeInTheDocument();
    await user.type(page.getByLabelText('PR 番号'), '91');
    await user.click(page.getByRole('checkbox'));
    await user.click(page.getByRole('button', { name: 'リンクする' }));

    await expect(canvas.findByText('#91')).resolves.toBeInTheDocument();
    await expect(postedBodies).toEqual([{ number: 91, closes: true }]);
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
  },
};
