import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';

import ReviewPullRequestList from '@/components/reviews/ReviewPullRequestList.vue';
import { DEFAULT_REVIEW_LIST_URL_STATE } from '@/lib/review-url-state';

import {
  PROJECT_UUID,
  TENANT_UUID,
  mockReviewFetch,
  queryClientDecorator,
} from './review-story-mocks';

const meta = {
  title: 'Pages/Reviews/PullRequestList',
  component: ReviewPullRequestList,
  args: {
    tenantId: TENANT_UUID,
    tenantSlug: 'acme',
    projectId: PROJECT_UUID,
    projectKey: 'TASK',
    initialUrlState: DEFAULT_REVIEW_LIST_URL_STATE,
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'レビューのある PR の一覧（`/reviews`）。状態・作成者・未解決・最終レビュー・自分の PR で絞り込み、行から PR の詳細へ移る。fetch モックで検証。',
      },
    },
  },
  decorators: [queryClientDecorator()],
} satisfies Meta<typeof ReviewPullRequestList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '一覧（2 ページ）',
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(canvas.findByRole('heading', { name: 'レビュー' })).resolves.toBeInTheDocument();

    // 既定は Open の 12 件。10 件ごとに分かれる
    const rows = await canvas.findAllByTestId('pr-row');
    await expect(rows).toHaveLength(10);
    await expect(canvas.getByText('1–10 / 12 件')).toBeInTheDocument();

    // 件数は High・Medium / Low・Nit / 承認待ちに分け、0 はチップにしない
    const first = within(rows[1]);
    await expect(first.getByTestId('pr-high-medium')).toHaveTextContent('3');
    await expect(first.getByTestId('pr-low-nit')).toHaveTextContent('2');
    await expect(first.getByTestId('pr-awaiting')).toHaveTextContent('1');
    await expect(
      first.getByRole('link', { name: 'タスク一覧の絞り込みを URL に持つ' }),
    ).toHaveAttribute('href', '/acme/projects/TASK/reviews/231');

    await user.click(canvas.getByRole('button', { name: /次へ/ }));
    await waitFor(() => expect(canvas.getAllByTestId('pr-row')).toHaveLength(2));
  },
};

export const Loading: Story = {
  name: '読み込み中',
  beforeEach: mockReviewFetch({ hangPullRequests: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByTestId('pr-list-loading')).resolves.toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: 'レビューがまだ無い',
  beforeEach: mockReviewFetch({ pullRequests: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('レビューはまだありません')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('PR にレビューを出すと、ここに並びます。')).toBeInTheDocument();
  },
};

export const NoMatch: Story = {
  name: '条件に合う PR が無い',
  args: {
    initialUrlState: { ...DEFAULT_REVIEW_LIST_URL_STATE, q: '共有', mine: true },
  },
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('「共有」に一致する、自分が作成した Open の PR はありません'),
    ).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'すべての PR から探す' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'すべての状態を表示' })).toBeInTheDocument();
  },
};

export const LoadError: Story = {
  name: '読み込みに失敗',
  beforeEach: mockReviewFetch({ prsStatus: 500 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('レビューを読み込めませんでした')).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '再試行' })).toBeInTheDocument();
  },
};

export const MineUnavailable: Story = {
  name: '「自分の PR」が使えない（GitHub 未連携）',
  beforeEach: mockReviewFetch({ githubLogin: null }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText(
        'GitHub と連携すると使えます。PR の作成者を GitHub のユーザー名で判定するためです。',
      ),
    ).resolves.toBeInTheDocument();
    await expect(canvas.getByRole('switch', { name: '自分の PR' })).toBeDisabled();
  },
};
