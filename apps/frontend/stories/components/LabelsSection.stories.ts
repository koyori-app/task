import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import LabelsSection from '@/components/projects/LabelsSection.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const sampleLabels = [
  {
    id: '00000000-0000-4000-8000-000000000021',
    name: 'bug',
    description: '不具合の報告',
    color: '#ef4444',
    icon_url: null,
    project_id: PROJECT_UUID,
  },
  {
    id: '00000000-0000-4000-8000-000000000022',
    name: 'enhancement',
    description: '機能改善の提案',
    color: '#3b82f6',
    icon_url: null,
    project_id: PROJECT_UUID,
  },
];

const emojiLabel = {
  id: '00000000-0000-4000-8000-000000000023',
  name: 'デザイン',
  description: '見た目の調整',
  color: '#8b5cf6',
  icon_url: null,
  icon_emoji: '🎨',
  project_id: PROJECT_UUID,
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;

/** GET/POST/PUT/DELETE をインメモリのラベル配列で応答する fetch モック */
function mockFetch(overrides: { empty?: boolean; withEmoji?: boolean } = {}) {
  return () => {
    const original = globalThis.fetch;
    const initial: Record<string, unknown>[] = overrides.empty
      ? []
      : [...sampleLabels, ...(overrides.withEmoji ? [emojiLabel] : [])];
    let labels = initial.map((label) => ({ ...label }));
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      const id = pathname.split('/').pop() ?? '';
      if (method === 'POST') {
        const body = await (req as Request).json();
        const created = {
          id: `00000000-0000-4000-8000-0000000000${30 + labels.length}`,
          icon_url: null,
          project_id: PROJECT_UUID,
          ...body,
        };
        labels = [...labels, created];
        return jsonResponse(created, 201);
      }
      if (method === 'PUT') {
        const body = await (req as Request).json();
        labels = labels.map((label) => (label.id === id ? { ...label, ...body } : label));
        return jsonResponse(labels.find((label) => label.id === id));
      }
      if (method === 'DELETE') {
        labels = labels.filter((label) => label.id !== id);
        return new Response(null, { status: 204 });
      }
      return jsonResponse(labels);
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
  title: 'Components/Projects/LabelsSection',
  component: LabelsSection,
  tags: ['autodocs'],
  args: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト設定のラベルセクション。行形式の一覧＋作成・編集・削除（確認ダイアログ）。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof LabelsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

const requestsOf = (method: string) =>
  (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .filter((req) => req.method === method);

export const Default: Story = {
  name: '一覧表示（ピルバッジ＋説明＋編集/削除ボタン）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('heading', { name: 'ラベル' })).resolves.toBeInTheDocument();
    await expect(canvas.findByText('bug')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('不具合の報告')).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '新しいラベル' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'ラベル「bug」を編集' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'ラベル「bug」を削除' })).toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: '空状態',
  beforeEach: mockFetch({ empty: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('ラベルはまだありません。「新しいラベル」から作成できます。'),
    ).resolves.toBeInTheDocument();
  },
};

export const CreateFlow: Story = {
  name: '作成（POST → 一覧反映）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('bug');
    await user.click(canvas.getByRole('button', { name: '新しいラベル' }));
    await user.type(await page.findByLabelText('名前'), 'design');
    await user.type(page.getByLabelText('説明'), 'デザイン関連');
    await user.click(page.getByRole('button', { name: 'ラベルを作成' }));

    await expect(canvas.findByText('design')).resolves.toBeInTheDocument();
    const [post] = requestsOf('POST');
    await expect(post).toBeTruthy();
    await expect(post.url).toContain(`/projects/${PROJECT_UUID}/labels`);
  },
};

export const EditFlow: Story = {
  name: '編集（プリフィル → PUT）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('bug');
    await user.click(canvas.getByRole('button', { name: 'ラベル「bug」を編集' }));

    const name = await page.findByLabelText('名前');
    await expect(name).toHaveValue('bug');
    await user.clear(name);
    await user.type(name, 'defect');
    await user.click(page.getByRole('button', { name: '変更を保存' }));

    await expect(canvas.findByText('defect')).resolves.toBeInTheDocument();
    const [put] = requestsOf('PUT');
    await expect(put).toBeTruthy();
    await expect(put.url).toContain(`/labels/${sampleLabels[0].id}`);
  },
};

export const DeleteFlow: Story = {
  name: '削除（確認ダイアログ → DELETE）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('bug');
    await user.click(canvas.getByRole('button', { name: 'ラベル「bug」を削除' }));
    await expect(
      page.findByText('「bug」を削除します。この操作は取り消せません。'),
    ).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '削除する' }));

    await waitFor(() => expect(canvas.queryByText('bug')).not.toBeInTheDocument());
    const [del] = requestsOf('DELETE');
    await expect(del).toBeTruthy();
    await expect(del.url).toContain(`/labels/${sampleLabels[0].id}`);
  },
};

export const WithEmoji: Story = {
  name: '絵文字のラベル（色の丸の代わりに絵文字）',
  beforeEach: mockFetch({ withEmoji: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const name = await canvas.findByText('デザイン');
    const row = name.closest('li')!;
    await expect(row.textContent).toContain('🎨');
    await expect(row.querySelector('.rounded-full[style*="background-color"]')).toBeNull();
    // 対照: 絵文字を持たないラベルは色の丸
    const bugRow = canvas.getByText('bug').closest('li')!;
    await expect(bugRow.querySelector('.rounded-full[style*="background-color"]')).not.toBeNull();
  },
};

export const CreateWithEmoji: Story = {
  name: '作成（絵文字を選ぶと色の欄が消える → POST）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('bug');
    await user.click(canvas.getByRole('button', { name: '新しいラベル' }));
    await user.type(await page.findByLabelText('名前'), 'design');
    await expect(page.getByLabelText('色')).toBeInTheDocument();

    await user.click(page.getByRole('button', { name: '絵文字を選ぶ' }));
    await user.type(await page.findByRole('searchbox', { name: '絵文字を検索' }), 'art');
    await waitFor(() =>
      expect(page.getAllByRole('gridcell').some((c) => c.textContent?.trim() === '🎨')).toBe(true),
    );
    await user.click(page.getAllByRole('gridcell').find((c) => c.textContent?.trim() === '🎨')!);

    await expect(
      page.findByRole('button', { name: '絵文字 🎨 を変更' }),
    ).resolves.toBeInTheDocument();
    await expect(page.queryByLabelText('色')).not.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: 'ラベルを作成' }));

    // モックは POST の本文をそのまま一覧に足すので、行の絵文字が送った icon_emoji になる
    const row = (await canvas.findByText('design')).closest('li')!;
    await expect(row.textContent).toContain('🎨');
    await expect(row.querySelector('.rounded-full[style*="background-color"]')).toBeNull();
    await expect(requestsOf('POST')).toHaveLength(1);
  },
};
