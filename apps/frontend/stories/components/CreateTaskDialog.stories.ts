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
    project_id: 'project-1',
    created_at: '2026-07-15T00:00:00Z',
  },
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
    },
    template: '<story />',
  });
}

function mockCreate(status = 201) {
  const originalFetch = globalThis.fetch;
  const fetchSpy = fn(
    async () =>
      new Response(JSON.stringify(status === 201 ? createdTask : { message: 'server error' }), {
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
    tenantDisplayId: 'tenant-demo',
    projectId: 'project-1',
    projectKey: 'ENG',
    statuses,
    navigateOnSuccess: false,
    onCreated: createdSpy,
    'onUpdate:open': openChangeSpy,
  },
} satisfies Meta<typeof CreateTaskDialog>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('dialog', { name: '新規タスク' })).resolves.toBeInTheDocument();
    await expect(canvas.getByLabelText(/タイトル/)).toBeInTheDocument();
    await expect(canvas.getByLabelText(/ステータス/)).toHaveValue('status-backlog');
  },
};

export const Success201: Story = {
  beforeEach() {
    const mock = mockCreate();
    createdSpy.mockClear();
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.type(await canvas.findByLabelText(/タイトル/), '  新しいタスク  ');
    await user.type(canvas.getByLabelText('説明'), 'Storybook から作成');
    await user.type(canvas.getByLabelText('期限'), '2026-07-15');
    await user.type(canvas.getByLabelText('最終期限'), '2026-07-31');
    await user.selectOptions(canvas.getByLabelText('優先度'), 'High');
    const submit = canvas.getByRole('button', { name: '作成' });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);

    await waitFor(() => expect(invalidateQueriesSpy).toHaveBeenCalled());
    const request = (globalThis.fetch as ReturnType<typeof fn>).mock.calls[0]?.[0] as Request;
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
      hard_deadline: '2026-07-31T00:00:00.000Z',
    });
    await expect(invalidateQueriesSpy).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks'],
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
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const titleInput = await canvas.findByLabelText(/タイトル/);
    await user.click(titleInput);
    await user.keyboard('{Escape}');
    await expect(openChangeSpy).toHaveBeenCalledWith(false);
  },
};

export const ValidationError: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
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
    const canvas = within(canvasElement);
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
