import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';

import ReviewPullRequestDetail from '@/components/reviews/ReviewPullRequestDetail.vue';
import { DEFAULT_REVIEW_DETAIL_URL_STATE } from '@/lib/review-url-state';

import {
  PROJECT_UUID,
  TENANT_UUID,
  VIEWER_ID,
  detailFindings,
  mockReviewFetch,
  queryClientDecorator,
} from './review-story-mocks';

const meta = {
  title: 'Pages/Reviews/PullRequestDetail',
  component: ReviewPullRequestDetail,
  args: {
    tenantId: TENANT_UUID,
    tenantSlug: 'acme',
    projectId: PROJECT_UUID,
    projectKey: 'TASK',
    prNumber: 231,
    viewerId: VIEWER_ID,
    initialUrlState: { ...DEFAULT_REVIEW_DETAIL_URL_STATE, finding: 'f-url-state' },
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'PR の詳細（`/reviews/:prNumber`）。マージ判定・絞り込み・ファイルの木・ファイルごとにまとめた指摘と、状態の遷移・ラウンドの起票。fetch モックで検証。',
      },
    },
  },
  decorators: [queryClientDecorator()],
} satisfies Meta<typeof ReviewPullRequestDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

const SELECTED_TITLE = 'URL の絞り込みを読むときに未知の値で落ちる';

/** 選んだ行（開いている行） */
async function selectedRow(canvasElement: HTMLElement) {
  const canvas = within(canvasElement);
  await canvas.findByText(SELECTED_TITLE);
  const row = canvasElement.querySelector<HTMLElement>(
    '[data-testid="finding-row"][data-selected]',
  );
  if (!row) throw new Error('selected row not found');
  return within(row);
}

export const Default: Story = {
  name: 'ファイルごとの指摘（選んだ行を開く）',
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'タスク一覧の絞り込みを URL に持つ' }),
    ).resolves.toBeInTheDocument();
    await expect(canvas.getByTestId('pr-meta')).toHaveTextContent(
      '#231 · yupix · Open · koyori-app/task',
    );
    await expect(canvas.getByRole('link', { name: 'GitHub で開く' })).toHaveAttribute(
      'href',
      'https://github.com/koyori-app/task/pull/231',
    );
    // High・Medium の未解決は 4 件（open 2・fixing 1・fixed 1）
    await expect(canvas.findByTestId('merge-gate')).resolves.toHaveTextContent(
      'High・Medium の未解決が 4 件（うち承認待ち 1）',
    );

    // ファイルごとにまとまり、件数は「解決 / 全体」
    const groups = await canvas.findAllByTestId('file-group');
    await expect(groups).toHaveLength(5);
    await expect(within(groups[0]).getByTestId('file-progress')).toHaveTextContent('解決 2 / 3');
    await expect(canvas.getByRole('tree', { name: '指摘のあるファイル' })).toBeInTheDocument();
    await expect(canvas.getByText('ファイル · 5')).toBeInTheDocument();

    // 選んだ行だけが開き、操作は左に 2 つ、右に取り下げ
    const row = await selectedRow(canvasElement);
    await expect(row.getByRole('button', { name: '修正に着手する' })).toBeInTheDocument();
    await expect(row.getByRole('button', { name: '修正した' })).toBeInTheDocument();
    await expect(row.getByRole('button', { name: '取り下げる…' })).toBeInTheDocument();
    await expect(canvas.getAllByTestId('finding-detail')).toHaveLength(1);
    // 繰り延べ先のタスクは番号のリンクで出る
    await expect(canvas.findByRole('link', { name: /TASK-512/ })).resolves.toHaveAttribute(
      'href',
      '/acme/projects/TASK/tasks/TASK-512',
    );
  },
};

export const AfterFixed: Story = {
  name: '「修正した」のあと（本人は確認できない）',
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const user = userEvent.setup();
    const row = await selectedRow(canvasElement);
    await user.click(row.getByRole('button', { name: '修正した' }));

    await waitFor(() => expect(row.getByRole('button', { name: '確認した' })).toBeDisabled());
    await expect(row.getByText('修正した人は確認できません')).toBeInTheDocument();
    await expect(row.getByRole('button', { name: 'レビューに戻す' })).toBeEnabled();
    await expect(row.getByTestId('finding-last-transition')).toHaveTextContent(
      'yupix が Open → Fixed にしました',
    );
  },
};

export const DeferForm: Story = {
  name: '「繰り延べる…」の入力欄',
  args: {
    initialUrlState: { ...DEFAULT_REVIEW_DETAIL_URL_STATE, finding: 'f-query-order' },
  },
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByText('クエリの並び順が保存のたびに変わる');
    await user.click(await canvas.findByRole('button', { name: '繰り延べる…' }));

    const form = within(await canvas.findByTestId('finding-form-defer'));
    await expect(form.getByText('この指摘を繰り延べる')).toBeInTheDocument();
    await expect(
      form.getByText('繰り延べると、あとで扱うタスクを自動で作ります。'),
    ).toBeInTheDocument();
    // 理由は任意なので、空でも確定できる
    await expect(form.getByRole('button', { name: '繰り延べる' })).toBeEnabled();
  },
};

export const RejectForm: Story = {
  name: '「取り下げる…」の入力欄（理由は必須）',
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const row = await selectedRow(canvasElement);
    await user.click(row.getByRole('button', { name: '取り下げる…' }));

    const form = within(await canvas.findByTestId('finding-form-reject'));
    const confirm = form.getByRole('button', { name: '取り下げる' });
    await expect(confirm).toBeDisabled();
    await user.type(form.getByRole('textbox', { name: /理由/ }), '仕様どおりの挙動だった');
    await expect(confirm).toBeEnabled();
  },
};

export const AllResolved: Story = {
  name: '全部解決（緑の件数・マージ可）',
  args: { initialUrlState: DEFAULT_REVIEW_DETAIL_URL_STATE },
  beforeEach: mockReviewFetch({
    findings: detailFindings()
      .slice(0, 5)
      .map((finding) => ({ ...finding, state: 'verified' as const, available_actions: [] })),
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByTestId('merge-gate')).resolves.toHaveTextContent('マージ可');
    const progress = await canvas.findAllByTestId('file-progress');
    await expect(progress.map((el) => el.textContent?.trim())).toEqual([
      '解決 3 / 3',
      '解決 2 / 2',
    ]);
  },
};

export const NoFindings: Story = {
  name: '指摘がまだ無い（未レビュー）',
  args: { initialUrlState: DEFAULT_REVIEW_DETAIL_URL_STATE },
  beforeEach: mockReviewFetch({
    findings: [],
    summary: { gate: 'unreviewed', rounds: 0, latest_head_sha: null, mergeable: false },
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('この PR に指摘はありません')).resolves.toBeInTheDocument();
    await expect(canvas.findByTestId('merge-gate')).resolves.toHaveTextContent('未レビュー');
  },
};

export const NoMatch: Story = {
  name: '条件に合う指摘が無い',
  args: {
    initialUrlState: { ...DEFAULT_REVIEW_DETAIL_URL_STATE, q: '存在しない語' },
  },
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(canvas.findByText('条件に合う指摘はありません')).resolves.toBeInTheDocument();
    await user.click(canvas.getByRole('button', { name: '条件を外す' }));
    await waitFor(() => expect(canvas.getAllByTestId('finding-row')).toHaveLength(10));
  },
};

export const RoundComposer: Story = {
  name: '指摘の起票（下書き → 確定）',
  args: { initialUrlState: DEFAULT_REVIEW_DETAIL_URL_STATE },
  beforeEach: mockReviewFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.click(await canvas.findByTestId('compose'));

    const composer = within(await canvas.findByTestId('round-composer'));
    await expect(composer.getByText('新しいラウンド（R4）を起票')).toBeInTheDocument();
    await user.type(
      composer.getByRole('textbox', { name: /レビュー対象の head SHA/ }),
      '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e',
    );
    await user.type(composer.getByRole('textbox', { name: /タイトル/ }), 'ページ番号が URL に残る');
    await user.type(
      composer.getByRole('textbox', { name: 'ファイル' }),
      'src/pages/tasks/+Page.vue',
    );
    await user.type(composer.getByRole('textbox', { name: '行' }), '318');
    await user.type(composer.getByRole('textbox', { name: '本文' }), '再現条件と根拠');
    await user.click(composer.getByRole('button', { name: '下書きに追加' }));
    await expect(composer.getByTestId('staged-list')).toHaveTextContent('ページ番号が URL に残る');

    await user.click(composer.getByRole('button', { name: '確定（1 件）' }));
    await waitFor(() => expect(canvas.queryByTestId('round-composer')).not.toBeInTheDocument());
  },
};
