import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';

import ReviewMergeGate from '@/components/reviews/ReviewMergeGate.vue';
import type { ReviewSummary } from '@/lib/review-findings';

const HEAD_SHA = 'be31c05f9a1244d3b0e6f7a8c9d0e1f2a3b4c5d6';
const NEW_HEAD_SHA = 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678';

function summary(overrides: Partial<ReviewSummary>): ReviewSummary {
  return {
    pr_number: 231,
    rounds: 3,
    counts: [],
    blocking: 0,
    latest_head_sha: HEAD_SHA,
    repository: 'koyori-app/task',
    cached_pr_head_sha: HEAD_SHA,
    pr_head_checked_at: '2026-07-15T01:05:00Z',
    owner_override_rejections: 0,
    mergeable: false,
    gate: 'ready',
    ...overrides,
  };
}

// emit を受けるスパイ。story ごとに数え直す
const onShowUnresolved = fn();
const onCompose = fn();

const meta = {
  beforeEach: () => {
    onShowUnresolved.mockClear();
    onCompose.mockClear();
  },
  title: 'Components/Reviews/ReviewMergeGate',
  component: ReviewMergeGate,
  tags: ['autodocs'],
  args: {
    summary: summary({}),
    target: { host: 'github', html_url: null, repository: 'koyori-app/task' },
    'onShow-unresolved': onShowUnresolved,
    onCompose,
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'PR の詳細のマージ判定。判定は backend の `gate` をそのまま使い、満たしていない条件だけを並べる。',
      },
    },
  },
} satisfies Meta<typeof ReviewMergeGate>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Ready: Story = {
  name: 'ready（マージ可）',
  args: { summary: summary({ gate: 'ready', mergeable: true }) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('heading', { name: 'マージ可' })).toBeInTheDocument();
    await expect(canvas.getByText(/GitHub 確認: .+ 時点/)).toBeInTheDocument();
    await expect(canvas.queryByTestId('merge-gate-condition')).not.toBeInTheDocument();
  },
};

export const Blocked: Story = {
  name: 'blocked（未解決あり）',
  args: {
    summary: summary({
      gate: 'blocked',
      blocking: 4,
      counts: [
        { severity: 'high', state: 'open', count: 2 },
        { severity: 'medium', state: 'fixing', count: 1 },
        { severity: 'high', state: 'fixed', count: 1 },
      ],
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTestId('merge-gate-condition')).toHaveTextContent(
      'High・Medium の未解決が 4 件（うち承認待ち 1）',
    );
    await userEvent.click(canvas.getByRole('button', { name: '未解決を見る' }));
    await expect(onShowUnresolved).toHaveBeenCalledOnce();
  },
};

export const Unreviewed: Story = {
  name: 'unreviewed（未レビュー）',
  args: { summary: summary({ gate: 'unreviewed', rounds: 0, latest_head_sha: null }) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('まだレビューがありません')).toBeInTheDocument();
    await userEvent.click(canvas.getByRole('button', { name: '指摘を起票' }));
    await expect(onCompose).toHaveBeenCalledOnce();
  },
};

export const Outdated: Story = {
  name: 'outdated（レビューが古い）',
  args: { summary: summary({ gate: 'outdated', cached_pr_head_sha: NEW_HEAD_SHA }) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('heading', { name: 'レビューが古い' })).toBeInTheDocument();
    await expect(canvas.getByRole('link', { name: 'a1b2c3d' })).toHaveAttribute(
      'href',
      `https://github.com/koyori-app/task/commit/${NEW_HEAD_SHA}`,
    );
  },
};

export const StaleUnknown: Story = {
  name: 'stale_unknown（鮮度不明）',
  args: {
    summary: summary({ gate: 'stale_unknown', cached_pr_head_sha: null, pr_head_checked_at: null }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByText('今の head を GitHub で確かめられていません'),
    ).toBeInTheDocument();
    await expect(canvas.queryByRole('button')).not.toBeInTheDocument();
  },
};

export const Unlinked: Story = {
  name: 'unlinked（リポジトリ未確定）',
  args: {
    summary: summary({ gate: 'unlinked', repository: null, owner_override_rejections: 2 }),
    target: { host: null, html_url: null, repository: null },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByText('GitHub と連携していないため、どの PR を見た集計か決まりません'),
    ).toBeInTheDocument();
    await expect(canvas.getByTestId('merge-gate-override')).toHaveTextContent(
      'オーナー代行での棄却 2 件',
    );
  },
};
