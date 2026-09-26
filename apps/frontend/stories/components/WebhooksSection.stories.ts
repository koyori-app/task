import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import WebhooksSection from '@/components/projects/WebhooksSection.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';
const CREATED_SECRET = 'whsec-0123456789abcdef';

const activeHook = {
  id: '00000000-0000-4000-8000-000000000031',
  project_id: PROJECT_UUID,
  url: 'https://example.com/hooks/koyori',
  events: ['task.created', 'comment.created'],
  format: 'json',
  is_active: true,
  failure_streak: 0,
  created_by: '00000000-0000-4000-8000-000000000001',
  created_at: '2026-09-01T00:00:00Z',
};

const stoppedHook = {
  ...activeHook,
  id: '00000000-0000-4000-8000-000000000032',
  url: 'https://discord.com/api/webhooks/1234567890/token',
  events: ['review.round_created', 'review.finding_changed'],
  format: 'discord',
  is_active: false,
  failure_streak: 5,
};

const sampleDeliveries = [
  {
    id: '00000000-0000-4000-8000-000000000041',
    webhook_id: activeHook.id,
    event: 'comment.created',
    payload: { event: 'comment.created', comment: { body: '確認しました' } },
    status_code: 500,
    attempt: 5,
    next_attempt_at: null,
    last_error: 'HTTP 500 Internal Server Error',
    delivered_at: null,
    created_at: '2026-09-02T03:00:00Z',
  },
  {
    id: '00000000-0000-4000-8000-000000000042',
    webhook_id: activeHook.id,
    event: 'task.created',
    payload: { event: 'task.created', task: { seq_id: 42, title: 'OAuth 対応を実装する' } },
    status_code: 200,
    attempt: 1,
    next_attempt_at: null,
    last_error: null,
    delivered_at: '2026-09-02T02:00:01Z',
    created_at: '2026-09-02T02:00:00Z',
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;

/** Webhook 一覧・作成・更新・配信履歴・再送をインメモリで応答する fetch モック */
function mockFetch(
  overrides: { empty?: boolean; listStatus?: number; updateStatus?: number } = {},
) {
  return () => {
    const original = globalThis.fetch;
    let webhooks = overrides.empty ? [] : [{ ...activeHook }, { ...stoppedHook }];
    let deliveries: Record<string, unknown>[] = sampleDeliveries.map((d) => ({ ...d }));
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;

      const redeliver = pathname.match(/\/deliveries\/([^/]+)\/redeliver$/);
      if (method === 'POST' && redeliver) {
        const source = deliveries.find((d) => d.id === redeliver[1])!;
        const copy = {
          ...source,
          id: `00000000-0000-4000-8000-0000000000${50 + deliveries.length}`,
          status_code: null,
          attempt: 0,
          next_attempt_at: '2026-09-02T04:00:00Z',
          last_error: null,
          delivered_at: null,
          created_at: '2026-09-02T04:00:00Z',
        };
        deliveries = [copy, ...deliveries];
        return jsonResponse(copy, 201);
      }
      if (pathname.endsWith('/deliveries')) return jsonResponse(deliveries);

      if (method === 'POST') {
        const body = await (req as Request).json();
        const created = {
          ...activeHook,
          id: '00000000-0000-4000-8000-000000000039',
          url: body.url,
          events: body.events,
          format: body.format,
        };
        webhooks = [...webhooks, created];
        return jsonResponse({ ...created, secret: body.secret }, 201);
      }
      if (method === 'PUT') {
        if (overrides.updateStatus) {
          return jsonResponse({ message: 'forbidden' }, overrides.updateStatus);
        }
        const id = pathname.split('/').pop();
        const body = await (req as Request).json();
        webhooks = webhooks.map((w) => (w.id === id ? { ...w, ...body } : w));
        return jsonResponse(webhooks.find((w) => w.id === id));
      }
      if (overrides.listStatus) return jsonResponse({ message: 'error' }, overrides.listStatus);
      return jsonResponse(webhooks);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
    };
  };
}

function storyDecorator() {
  return () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, gcTime: 0, staleTime: 0 },
          mutations: { retry: false },
        },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Components/Projects/WebhooksSection',
  component: WebhooksSection,
  tags: ['autodocs'],
  args: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
  parameters: {
    layout: 'padded',
    docs: {
      // fetch モックが他の story に漏れないよう、このファイルの Docs だけ iframe を分ける。
      story: { inline: false, iframeHeight: 480 },
      description: {
        component:
          'プロジェクト設定の Webhook セクション。一覧＋作成（secret を一度だけ表示）・編集・有効/無効・削除＋配信履歴と再送。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof WebhooksSection>;

export default meta;
type Story = StoryObj<typeof meta>;

const requestsOf = (method: string) =>
  (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .filter((req) => req.method === method);

export const Default: Story = {
  name: '一覧表示（有効 1 件＋連続失敗で停止 1 件）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('heading', { name: 'Webhook' })).resolves.toBeInTheDocument();
    await expect(canvas.findByText(activeHook.url)).resolves.toBeInTheDocument();
    await expect(canvas.getByText(stoppedHook.url)).toBeInTheDocument();
    await expect(canvas.getByText('連続失敗で停止')).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '無効にする' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '有効にする' })).toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: '空状態',
  beforeEach: mockFetch({ empty: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('Webhook はまだありません')).resolves.toBeInTheDocument();
  },
};

export const CreateFlow: Story = {
  name: '作成（POST → secret を一度だけ表示）',
  beforeEach: mockFetch({ empty: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('Webhook はまだありません');
    await user.click(canvas.getByRole('button', { name: '追加' }));

    await user.type(await page.findByLabelText('URL'), 'https://example.com/hooks/new');
    await user.type(page.getByLabelText('シークレット'), CREATED_SECRET);
    await user.click(page.getByRole('checkbox', { name: 'タスクの作成' }));
    await user.click(page.getByRole('button', { name: 'Webhook を追加' }));

    await expect(page.findByTestId('created-secret')).resolves.toHaveTextContent(CREATED_SECRET);
    await expect(canvas.findByText('https://example.com/hooks/new')).resolves.toBeInTheDocument();
    const [post] = requestsOf('POST');
    await expect(post.url).toContain(`/projects/${PROJECT_UUID}/webhooks`);

    // 閉じたら二度と表示しない
    await user.click(page.getByRole('button', { name: '閉じる' }));
    await waitFor(() => expect(page.queryByText(CREATED_SECRET)).not.toBeInTheDocument());
  },
};

export const DeliveriesAndRedeliver: Story = {
  name: '配信履歴の展開と再送',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByText(activeHook.url);
    await user.click(canvas.getAllByRole('button', { name: '配信履歴' })[0]);

    await expect(canvas.findByText('失敗 (5 回)')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('成功 (HTTP 200)')).toBeInTheDocument();
    await user.click(canvas.getAllByRole('button', { name: '再送' })[0]);

    await expect(canvas.findByText('送信待ち')).resolves.toBeInTheDocument();
    await waitFor(() =>
      expect(requestsOf('POST').some((req) => req.url.endsWith('/redeliver'))).toBe(true),
    );
  },
};

export const Forbidden: Story = {
  name: '403（Member が無効化を押す）',
  beforeEach: mockFetch({ updateStatus: 403 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByText(activeHook.url);
    await user.click(canvas.getByRole('button', { name: '無効にする' }));

    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      'この操作にはプロジェクトの管理者権限が必要です',
    );
  },
};

export const LoadError: Story = {
  name: '読み込みエラー',
  beforeEach: mockFetch({ listStatus: 500 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      'Webhook を読み込めませんでした',
    );
  },
};
