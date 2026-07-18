import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import CustomFieldsSection from '@/components/projects/CustomFieldsSection.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const sampleFields = [
  {
    id: '00000000-0000-4000-8000-000000000101',
    project_id: PROJECT_UUID,
    name: '見積もり',
    field_type: 'number',
    is_required: false,
    position: 0,
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: '00000000-0000-4000-8000-000000000102',
    project_id: PROJECT_UUID,
    name: '優先度',
    field_type: 'select',
    options: [
      { label: '高', value: '高' },
      { label: '低', value: '低' },
    ],
    is_required: false,
    position: 1,
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: '00000000-0000-4000-8000-000000000103',
    project_id: PROJECT_UUID,
    name: '仕様リンク',
    field_type: 'url',
    is_required: false,
    position: 2,
    created_at: '2026-01-01T00:00:00Z',
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

type MockOptions = { fields?: unknown[]; rejectWrite?: number };

let fetchSpy: ReturnType<typeof fn> | null = null;

/**
 * reka-ui Select のポップアップ（floating-ui）が発する無害な
 * 「ResizeObserver loop completed with undelivered notifications」を
 * vitest browser mode が未処理エラー扱いして story を落とすため、
 * コールバックを rAF に遅延させて発生自体を抑止する（定番ワークアラウンド）。
 */
function stabilizeResizeObserver() {
  const OriginalResizeObserver = window.ResizeObserver;
  window.ResizeObserver = class extends OriginalResizeObserver {
    constructor(callback: ResizeObserverCallback) {
      super((entries, observer) => {
        window.requestAnimationFrame(() => callback(entries, observer));
      });
    }
  };
  return () => {
    window.ResizeObserver = OriginalResizeObserver;
  };
}

function mockFetch(overrides: MockOptions = {}) {
  return () => {
    const restoreResizeObserver = stabilizeResizeObserver();
    const original = globalThis.fetch;
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      if (url.includes('/custom-fields')) {
        if (method === 'GET') {
          return jsonResponse({ fields: overrides.fields ?? sampleFields });
        }
        if (overrides.rejectWrite) {
          return jsonResponse({ message: 'error' }, overrides.rejectWrite);
        }
        if (method === 'DELETE') return new Response(null, { status: 204 });
        const body = await (req as Request).clone().json();
        if (method === 'POST') {
          return jsonResponse(
            {
              id: '00000000-0000-4000-8000-000000000199',
              project_id: PROJECT_UUID,
              is_required: false,
              position: 99,
              created_at: '2026-01-01T00:00:00Z',
              ...body,
            },
            201,
          );
        }
        return jsonResponse({ ...sampleFields[0], ...body });
      }
      return jsonResponse([]);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
      restoreResizeObserver();
    };
  };
}

function requestByMethod(method: string) {
  return (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .find((req) => req.method === method);
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
  title: 'Components/CustomFieldsSection',
  component: CustomFieldsSection,
  tags: ['autodocs'],
  args: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト設定のカスタムフィールドセクション。一覧・追加・編集・削除を fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof CustomFieldsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '一覧表示（型アイコン＋日本語型バッジ）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'カスタムフィールド' }),
    ).resolves.toBeInTheDocument();
    const list = await canvas.findByRole('list', { name: 'カスタムフィールド一覧' });
    await expect(within(list).getByText('見積もり')).toBeInTheDocument();
    await expect(within(list).getByText('数値')).toBeInTheDocument();
    await expect(within(list).getByText('優先度')).toBeInTheDocument();
    await expect(within(list).getByText('選択')).toBeInTheDocument();
    await expect(within(list).getByText('仕様リンク')).toBeInTheDocument();
    await expect(within(list).getByText('URL')).toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: '空状態',
  beforeEach: mockFetch({ fields: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('カスタムフィールドはまだありません'),
    ).resolves.toBeInTheDocument();
  },
};

export const AddFlow: Story = {
  name: '追加（日付型を選択 → POST）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();

    await canvas.findByRole('list', { name: 'カスタムフィールド一覧' });
    await user.click(canvas.getByRole('button', { name: 'フィールドを追加' }));
    await expect(page.findByLabelText('名前')).resolves.toBeInTheDocument();

    await user.type(page.getByLabelText('名前'), '完了予定日');
    await user.click(page.getByRole('combobox', { name: '型' }));
    await user.click(await page.findByRole('option', { name: '日付' }));
    // ポップアップの閉じアニメーション中は他要素が a11y ツリーから隠れるため findByRole で待つ
    await user.click(await page.findByRole('button', { name: '追加する' }));

    const post = requestByMethod('POST');
    await expect(post).toBeTruthy();
    await expect(post!.url).toContain(`/projects/${PROJECT_UUID}/custom-fields`);
    const body = await post!.clone().json();
    await expect(body).toEqual({ name: '完了予定日', field_type: 'date' });
  },
};

export const AddSelectFlow: Story = {
  name: '追加（選択型は選択肢入力必須 → options 付き POST）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();

    await canvas.findByRole('list', { name: 'カスタムフィールド一覧' });
    await user.click(canvas.getByRole('button', { name: 'フィールドを追加' }));
    await user.type(await page.findByLabelText('名前'), '重要度');
    await user.click(page.getByRole('combobox', { name: '型' }));
    await user.click(await page.findByRole('option', { name: '選択' }));

    // 選択肢が空のまま送信するとバリデーションエラーになる
    // （ポップアップの閉じアニメーション中は他要素が a11y ツリーから隠れるため findByRole で待つ）
    await user.click(await page.findByRole('button', { name: '追加する' }));
    await expect(
      page.findByText('選択肢を1行に1つ以上入力してください'),
    ).resolves.toBeInTheDocument();
    await expect(requestByMethod('POST')).toBeFalsy();

    await user.type(page.getByLabelText('選択肢（1行に1つ）'), '高{enter}中{enter}低');
    await user.click(page.getByRole('button', { name: '追加する' }));

    const post = requestByMethod('POST');
    await expect(post).toBeTruthy();
    const body = await post!.clone().json();
    await expect(body).toEqual({
      name: '重要度',
      field_type: 'select',
      options: [
        { label: '高', value: '高' },
        { label: '中', value: '中' },
        { label: '低', value: '低' },
      ],
    });
  },
};

export const EditFlow: Story = {
  name: '編集（名前のみ PATCH・型は変更不可）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();

    await canvas.findByRole('list', { name: 'カスタムフィールド一覧' });
    await user.click(canvas.getByRole('button', { name: '見積もり を編集' }));

    const name = await page.findByLabelText('名前');
    await expect(name).toHaveValue('見積もり');
    await expect(page.queryByRole('combobox', { name: '型' })).toBeNull();

    await user.clear(name);
    await user.type(name, 'ポイント');
    await user.click(page.getByRole('button', { name: '保存する' }));

    const patch = requestByMethod('PATCH');
    await expect(patch).toBeTruthy();
    await expect(patch!.url).toContain(`/custom-fields/${sampleFields[0].id}`);
    const body = await patch!.clone().json();
    await expect(body).toEqual({ name: 'ポイント' });
  },
};

export const DeleteFlow: Story = {
  name: '削除（確認ダイアログ → DELETE）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();

    await canvas.findByRole('list', { name: 'カスタムフィールド一覧' });
    await user.click(canvas.getByRole('button', { name: '優先度 を削除' }));
    await expect(
      page.findByText('カスタムフィールドを削除しますか？'),
    ).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '削除する' }));

    const del = requestByMethod('DELETE');
    await expect(del).toBeTruthy();
    await expect(del!.url).toContain(`/custom-fields/${sampleFields[1].id}`);
  },
};
