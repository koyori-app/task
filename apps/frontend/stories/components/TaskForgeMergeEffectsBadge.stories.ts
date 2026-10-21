import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import TaskForgeMergeEffectsBadge from '@/components/tasks/TaskForgeMergeEffectsBadge.vue';
import type { components } from '@/generated/api';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const pullRequest: components['schemas']['TaskForgePullRequest'] = {
  id: '00000000-0000-4000-8000-000000000087',
  project_id: '00000000-0000-4000-8000-000000000010',
  host: 'github',
  host_url: 'https://github.com',
  repo_owner: 'acme',
  repo_name: 'backend',
  number: 87,
  title: 'feat: OAuth 対応',
  author_handle: 'yupix',
  state: 'merged',
  html_url: 'https://github.com/acme/backend/pull/87',
  merged_at: '2026-10-01T00:00:00Z',
  merge_effects_processed_at: null,
  commits_complete: false,
  sources: ['body'],
  closes: true,
  unresolved: null,
};

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch() {
  const original = globalThis.fetch;
  fetchSpy = fn().mockImplementation(async () => new Response(null, { status: 200 }));
  globalThis.fetch = fetchSpy;
  return () => {
    globalThis.fetch = original;
    fetchSpy = null;
  };
}

const meta = {
  title: 'Components/TaskForgeMergeEffectsBadge',
  component: TaskForgeMergeEffectsBadge,
  render: () => ({
    components: { TaskForgeMergeEffectsBadge },
    setup() {
      provide(
        VUE_QUERY_CLIENT,
        new QueryClient({ defaultOptions: { mutations: { retry: false } } }),
      );
      return { pullRequest };
    },
    template: `<TaskForgeMergeEffectsBadge tenant-id="${TENANT_UUID}" :pull-request="pullRequest" />`,
  }),
  beforeEach: mockFetch,
} satisfies Meta<typeof TaskForgeMergeEffectsBadge>;

export default meta;
type Story = StoryObj;

export const Badge: Story = {
  name: '未適用の印',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByRole('button', { name: 'マージ済み（自動完了は未適用）' }),
    ).toBeInTheDocument();
  },
};

export const Acknowledge: Story = {
  name: '説明→確認→確認済みにする',
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(page.getByRole('button', { name: 'マージ済み（自動完了は未適用）' }));
    await expect(await page.findByText('#87 の自動完了は未適用です')).toBeInTheDocument();
    await userEvent.click(page.getByRole('button', { name: '確認済みにする…' }));
    await userEvent.click(page.getByRole('button', { name: '確認済みにする' }));
    const req = (fetchSpy!.mock.calls as [Request][])[0]?.[0];
    await expect(req.method).toBe('POST');
    await expect(req.url).toContain(
      `/projects/${pullRequest.project_id}/forge/pull-requests/${pullRequest.id}/merge-effects/acknowledge`,
    );
  },
};
