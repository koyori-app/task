import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import CreateTaskDialog from '@/components/tasks/CreateTaskDialog.vue';

const statuses = [
  {
    id: 'status-backlog',
    name: 'Backlog',
    color: '#94a3b8',
    position: 0,
    is_default: true,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-1',
    created_at: '2026-07-15T00:00:00Z',
  },
  {
    id: 'status-progress',
    name: '進行中',
    color: '#3b82f6',
    position: 1,
    is_default: false,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-1',
    created_at: '2026-07-15T00:00:00Z',
  },
];

const labels = [
  {
    id: 'label-bug',
    name: 'bug',
    description: '',
    color: '#e11d48',
    icon_url: null,
    project_id: 'project-1',
  },
  {
    id: 'label-feature',
    name: 'feature',
    description: '',
    color: '#3b82f6',
    icon_url: null,
    project_id: 'project-1',
  },
];

const members = [
  { id: 'user-1', username: 'yupix', avatar_url: null },
  { id: 'user-2', username: 'sousuke', avatar_url: null },
];

const createdTask = {
  id: 'task-created',
  seq_id: 42,
  title: '新しいタスク',
  description: 'Storybook から作成',
  priority: 'High',
  status_id: 'status-backlog',
  project_id: 'project-1',
  soft_deadline: null,
  hard_deadline: null,
  is_archived: false,
  progress_pct: 0,
  created_at: '2026-07-15T00:00:00Z',
  updated_at: '2026-07-15T00:00:00Z',
  assignees: [],
};

/** 作成の POST（似ているタスクの GET と混ざるので method で拾う） */
function postedRequest() {
  return (globalThis.fetch as ReturnType<typeof fn>).mock.calls
    .map(([request]) => request as Request)
    .find((request) => request.method === 'POST') as Request;
}

let invalidateQueriesSpy: ReturnType<typeof fn>;
const createdSpy = fn();
const openChangeSpy = fn();

function decorator() {
  return () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      invalidateQueriesSpy = fn(queryClient.invalidateQueries.bind(queryClient));
      queryClient.invalidateQueries = invalidateQueriesSpy;
      provide(VUE_QUERY_CLIENT, queryClient);
      // 似ているタスクのリンクが表示用のテナント ID をルートから読む
      provide('vike-vue:usePageContext', { routeParams: { tenant: 'acme' } });
    },
    template: '<story />',
  });
}

function mockCreate(status = 201) {
  const originalFetch = globalThis.fetch;
  const fetchSpy = fn(async (request: RequestInfo | URL) =>
    // タイトル入力中に似ているタスク（GET）も問い合わせる。候補は 0 件で返す
    (request as Request).method === 'GET'
      ? new Response(JSON.stringify({ tasks: [] }), {
          headers: { 'Content-Type': 'application/json' },
        })
      : new Response(JSON.stringify(status === 201 ? createdTask : { message: 'server error' }), {
          status,
          headers: { 'Content-Type': 'application/json' },
        }),
  );
  globalThis.fetch = fetchSpy;
  return {
    fetchSpy,
    restore: () => {
      globalThis.fetch = originalFetch;
    },
  };
}

const meta = {
  title: 'Components/Tasks/CreateTaskDialog',
  component: CreateTaskDialog,
  tags: ['autodocs'],
  decorators: [decorator()],
  args: {
    open: true,
    tenantId: 'tenant-uuid',
    projectId: 'project-1',
    projectKey: 'ENG',
    statuses,
    labels,
    members,
    onCreated: createdSpy,
    'onUpdate:open': openChangeSpy,
  },
} satisfies Meta<typeof CreateTaskDialog>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    await expect(canvas.findByRole('dialog', { name: '新規タスク' })).resolves.toBeInTheDocument();
    await expect(canvas.getByLabelText(/タイトル/)).toBeInTheDocument();
    await expect(canvas.getByLabelText(/ステータス/)).toHaveTextContent('Backlog');
    // 右列の選択欄は未設定を薄い文字で出す
    await expect(canvas.getByRole('button', { name: /^担当者/ })).toHaveTextContent('未割り当て');
    await expect(canvas.getByRole('button', { name: /^ラベル/ })).toHaveTextContent('ラベルなし');
  },
};

export const Success201: Story = {
  beforeEach() {
    const mock = mockCreate();
    createdSpy.mockClear();
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.type(await canvas.findByLabelText(/タイトル/), '  新しいタスク  ');
    await user.type(canvas.getByLabelText('説明'), 'Storybook から作成');
    await user.type(canvas.getByLabelText('期限'), '2026-07-15');
    await user.type(canvas.getByLabelText('最終期限'), '2026-07-31');
    await user.type(canvas.getByLabelText('見積もり'), '90');
    await user.click(canvas.getByLabelText('優先度'));
    await user.click(await canvas.findByRole('option', { name: '高' }));
    const submit = await canvas.findByRole('button', { name: '作成' });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);

    await waitFor(() => expect(invalidateQueriesSpy).toHaveBeenCalled());
    const request = postedRequest();
    await expect(request.method).toBe('POST');
    await expect(new URL(request.url).pathname).toBe(
      '/api/v1/tenants/tenant-uuid/projects/project-1/tasks',
    );
    await expect(request.clone().json()).resolves.toMatchObject({
      title: '新しいタスク',
      status_id: 'status-backlog',
      description: 'Storybook から作成',
      priority: 'High',
      soft_deadline: '2026-07-15T00:00:00.000Z',
      estimated_minutes: 90,
      hard_deadline: '2026-07-31T00:00:00.000Z',
    });
    await expect(invalidateQueriesSpy).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks'],
      refetchType: 'none',
    });
    await expect(createdSpy).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'task-created', seq_id: 42 }),
    );
  },
};

export const EscapeCloses: Story = {
  beforeEach() {
    openChangeSpy.mockClear();
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    const titleInput = await canvas.findByLabelText(/タイトル/);
    await user.click(titleInput);
    await user.keyboard('{Escape}');
    await expect(openChangeSpy).toHaveBeenCalledWith(false);
  },
};

export const ValidationError: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const submit = await canvas.findByRole('button', { name: '作成' });
    await waitFor(() => expect(submit).toBeEnabled());
    await userEvent.click(submit);
    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      'タイトルを入力してください',
    );
  },
};

export const ApiFailure: Story = {
  beforeEach() {
    const mock = mockCreate(500);
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.type(await canvas.findByLabelText(/タイトル/), '失敗するタスク');
    const submit = canvas.getByRole('button', { name: '作成' });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);
    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      'タスクの作成に失敗しました',
    );
    await expect(invalidateQueriesSpy).not.toHaveBeenCalled();
  },
};

const MENTION_ALICE = '11111111-1111-4111-8111-111111111111';
const mentionMembers = [
  { id: MENTION_ALICE, username: 'alice', avatar_url: null },
  { id: '22222222-2222-4222-8222-222222222222', username: 'bob', avatar_url: null },
];

/** 説明欄で `@` を打つと候補が出て、選ぶと札（本文は `<@id>`）になり、そのまま作成の本文に載る */
export const MentionInDescription: Story = {
  args: { members: mentionMembers },
  beforeEach() {
    const mock = mockCreate();
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.type(await canvas.findByLabelText(/タイトル/), 'メンション付き');
    await user.click(canvas.getByLabelText('説明'));
    await user.keyboard('お願い @al');

    const popup = await waitFor(() => {
      const el = canvasElement.ownerDocument.querySelector<HTMLElement>('.cm-tooltip-autocomplete');
      if (!el) throw new Error('候補がまだ出ていない');
      return el;
    });
    await expect(popup.textContent).toContain('@alice');
    await expect(popup.textContent).not.toContain('@bob');
    // 誤って確定せぬよう、候補は表示から少し経つまで確定を受けつけぬ（CodeMirror の既定）
    await new Promise((resolve) => setTimeout(resolve, 150));
    await user.keyboard('{Enter}');

    // 入力欄の中は札で、id の文字は見えない
    const chip = await waitFor(() => {
      const el = canvasElement.ownerDocument.querySelector<HTMLElement>('.cm-mention');
      if (!el) throw new Error('札がまだ出ていない');
      return el;
    });
    await expect(chip.textContent).toBe('@alice');
    await expect(canvas.getByLabelText('説明').textContent).not.toContain(MENTION_ALICE);

    const submit = await canvas.findByRole('button', { name: '作成' });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);
    await waitFor(() => expect(postedRequest()).toBeDefined());
    const request = postedRequest();
    await expect(request.clone().json()).resolves.toMatchObject({
      description: `お願い <@${MENTION_ALICE}>`,
    });
  },
};

/** コードの中の `<@id>` は札にしない（表示側の KFM と同じ扱い） */
export const MentionInsideCodeStaysText: Story = {
  args: { members: mentionMembers },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.click(await canvas.findByLabelText('説明'));
    await user.keyboard(`\`<@${MENTION_ALICE}>\` と <@${MENTION_ALICE}>`);

    // 地の文の印だけが札になり、コードの中の印は文字のまま
    await waitFor(() =>
      expect(canvasElement.ownerDocument.querySelectorAll('.cm-mention')).toHaveLength(1),
    );
    await expect(canvas.getByLabelText('説明').textContent).toContain(`<@${MENTION_ALICE}>`);
  },
};
