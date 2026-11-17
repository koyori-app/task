import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import TaskTypesSection from '@/components/projects/TaskTypesSection.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const sampleTaskTypes = [
  {
    id: '00000000-0000-4000-8000-000000000021',
    name: 'バグ',
    color: '#ef4444',
    position: 0,
    project_id: PROJECT_UUID,
  },
  {
    id: '00000000-0000-4000-8000-000000000022',
    name: '機能',
    color: '#3b82f6',
    position: 1,
    project_id: PROJECT_UUID,
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;

/** GET/POST/PUT/DELETE をインメモリの種別配列で応答する fetch モック。`/reorder` は ids の順に並べ替える */
function mockFetch(overrides: { empty?: boolean } = {}) {
  return () => {
    const original = globalThis.fetch;
    let taskTypes = overrides.empty ? [] : sampleTaskTypes.map((taskType) => ({ ...taskType }));
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      const id = pathname.split('/').pop() ?? '';
      if (method === 'POST') {
        const body = await (req as Request).json();
        const created = {
          id: `00000000-0000-4000-8000-0000000000${30 + taskTypes.length}`,
          position: taskTypes.length,
          project_id: PROJECT_UUID,
          ...body,
        };
        taskTypes = [...taskTypes, created];
        return jsonResponse(created, 201);
      }
      if (method === 'PUT' && pathname.endsWith('/reorder')) {
        const body = (await (req as Request).json()) as { ids: string[] };
        taskTypes = body.ids.map((typeId, position) => ({
          ...taskTypes.find((taskType) => taskType.id === typeId)!,
          position,
        }));
        return jsonResponse(taskTypes);
      }
      if (method === 'PUT') {
        const body = await (req as Request).json();
        taskTypes = taskTypes.map((taskType) =>
          taskType.id === id ? { ...taskType, ...body } : taskType,
        );
        return jsonResponse(taskTypes.find((taskType) => taskType.id === id));
      }
      if (method === 'DELETE') {
        taskTypes = taskTypes.filter((taskType) => taskType.id !== id);
        return new Response(null, { status: 204 });
      }
      return jsonResponse(taskTypes);
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
  title: 'Components/Projects/TaskTypesSection',
  component: TaskTypesSection,
  tags: ['autodocs'],
  args: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト設定の種別セクション。行形式の一覧＋作成・編集・並べ替え・削除（確認ダイアログ）。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof TaskTypesSection>;

export default meta;
type Story = StoryObj<typeof meta>;

const requestsOf = (method: string) =>
  (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .filter((req) => req.method === method);

export const Default: Story = {
  name: '一覧表示（色の点＋名前＋並べ替え/編集/削除ボタン）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('heading', { name: '種別' })).resolves.toBeInTheDocument();
    await expect(canvas.findByText('バグ')).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '新しい種別' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '種別「バグ」を編集' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '種別「バグ」を削除' })).toBeInTheDocument();
    // 先頭は上へ動かせない
    await expect(canvas.getByRole('button', { name: '種別「バグ」を上へ' })).toBeDisabled();
  },
};

export const Empty: Story = {
  name: '空状態',
  beforeEach: mockFetch({ empty: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('種別はまだありません。「新しい種別」から作成できます。'),
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
    await canvas.findByText('バグ');
    await user.click(canvas.getByRole('button', { name: '新しい種別' }));
    await user.type(await page.findByLabelText('名前'), '調査');
    await user.click(page.getByRole('button', { name: '種別を作成' }));

    await expect(canvas.findByText('調査')).resolves.toBeInTheDocument();
    const [post] = requestsOf('POST');
    await expect(post).toBeTruthy();
    await expect(post.url).toContain(`/projects/${PROJECT_UUID}/task-types`);
  },
};

export const EditFlow: Story = {
  name: '編集（プリフィル → PUT）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('バグ');
    await user.click(canvas.getByRole('button', { name: '種別「バグ」を編集' }));

    const name = await page.findByLabelText('名前');
    await expect(name).toHaveValue('バグ');
    await user.clear(name);
    await user.type(name, '不具合');
    await user.click(page.getByRole('button', { name: '変更を保存' }));

    await expect(canvas.findByText('不具合')).resolves.toBeInTheDocument();
    const [put] = requestsOf('PUT');
    await expect(put).toBeTruthy();
    await expect(put.url).toContain(`/task-types/${sampleTaskTypes[0].id}`);
  },
};

export const ReorderFlow: Story = {
  name: '並べ替え（下へ → PUT /reorder → 一覧反映）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByText('バグ');
    await user.click(canvas.getByRole('button', { name: '種別「バグ」を下へ' }));

    await waitFor(() => expect(canvas.getAllByRole('listitem')[0]).toHaveTextContent('機能'));
    const [put] = requestsOf('PUT');
    await expect(put.url).toContain('/task-types/reorder');
  },
};

export const DeleteFlow: Story = {
  name: '削除（確認ダイアログ → DELETE）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('バグ');
    await user.click(canvas.getByRole('button', { name: '種別「バグ」を削除' }));
    await expect(
      page.findByText(
        '「バグ」を削除します。付いていたタスクは残り、種別だけが外れます。この操作は取り消せません。',
      ),
    ).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '削除する' }));

    await waitFor(() => expect(canvas.queryByText('バグ')).not.toBeInTheDocument());
    const [del] = requestsOf('DELETE');
    await expect(del).toBeTruthy();
    await expect(del.url).toContain(`/task-types/${sampleTaskTypes[0].id}`);
  },
};
