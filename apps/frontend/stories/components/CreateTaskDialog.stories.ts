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

type User = ReturnType<typeof userEvent.setup>;
type Canvas = ReturnType<typeof within>;

const DATE_PANEL = '[role="dialog"][aria-label="日付"]';

/** 日付チップのパネルを開き、行を選んで日を押す（カレンダーは preview が固定した今日の月で開く） */
async function pickDate(
  user: User,
  canvas: Canvas,
  field: 'plannedStartOn' | 'softDeadline' | 'hardDeadline',
  date: string,
) {
  const doc = document;
  if (!doc.querySelector(DATE_PANEL))
    await user.click(canvas.getByRole('button', { name: /^日付/ }));
  const row = await waitFor(() => {
    const el = doc.querySelector<HTMLElement>(`[data-date-field="${field}"]`);
    if (!el) throw new Error(`${field} の行がまだ出ていない`);
    return el;
  });
  await user.click(row);
  const day = await waitFor(() => {
    const cell = doc.querySelector<HTMLElement>(`[data-date="${date}"]`);
    if (!cell) throw new Error(`${date} がカレンダーに無い`);
    return cell;
  });
  await user.click(day);
}

/** 似ているタスクの候補（タイトルを入力すると出る） */
const similarTasks = [
  { id: 'task-7', seq_id: 7, title: 'ログイン画面の修正', score: 0.62 },
  { id: 'task-12', seq_id: 12, title: 'ログインフォームの文言を直す', score: 0.5 },
];

/** 開いたあと、全項目を入力する。日付のパネルと一覧は閉じた状態で終える */
async function fillAll(canvas: Canvas, user: User) {
  const titleInput = await canvas.findByLabelText(/タイトル/);
  await user.type(titleInput, 'ログイン画面のエラー表示を直す');
  await user.type(
    canvas.getByLabelText('説明'),
    '概要{Enter}- 入力欄の下に出す{Enter}文言を揃える',
  );

  await user.click(canvas.getByRole('button', { name: /^優先度/ }));
  await user.click(await canvas.findByRole('option', { name: '高' }));

  await user.click(canvas.getByRole('button', { name: /^担当者/ }));
  await user.click(await canvas.findByRole('option', { name: 'yupix' }));
  await user.click(titleInput);

  await user.click(canvas.getByRole('button', { name: /^ラベル/ }));
  await user.click(await canvas.findByRole('menuitemcheckbox', { name: 'bug' }));
  await user.click(await canvas.findByRole('menuitemcheckbox', { name: 'feature' }));
  await user.click(titleInput);

  await pickDate(user, canvas, 'plannedStartOn', '2026-07-13');
  await pickDate(user, canvas, 'softDeadline', '2026-07-15');
  await pickDate(user, canvas, 'hardDeadline', '2026-07-31');
  await user.click(titleInput);
  await waitFor(() => expect(document.querySelector(DATE_PANEL)).toBeNull());

  await user.type(canvas.getByLabelText('見積もり'), '90');
  await user.type(canvas.getByLabelText('進捗率'), '30');
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

function mockCreate(status = 201, similar: typeof similarTasks = []) {
  const originalFetch = globalThis.fetch;
  const fetchSpy = fn(async (request: RequestInfo | URL) =>
    // タイトル入力中に似ているタスク（GET）も問い合わせる。既定は 0 件で返す
    (request as Request).method === 'GET'
      ? new Response(JSON.stringify({ tasks: similar }), {
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
    await expect(canvas.getByRole('button', { name: /^ステータス/ })).toHaveTextContent('Backlog');
    // チップ列は未設定を項目名で出す
    await expect(canvas.getByRole('button', { name: /^担当者/ })).toHaveTextContent('担当者');
    await expect(canvas.getByRole('button', { name: /^ラベル/ })).toHaveTextContent('ラベル');
    await expect(canvas.getByRole('button', { name: /^日付/ })).toHaveTextContent('日付');
    await expect(canvas.getByLabelText('見積もり').closest('label')).toHaveTextContent('見積');
    await expect(canvas.getByLabelText('進捗率').closest('label')).toHaveTextContent('進捗');
  },
};

/** 全項目を入力した状態（似ているタスクの囲みも出す） */
export const Filled: Story = {
  beforeEach() {
    const mock = mockCreate(201, similarTasks);
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await fillAll(canvas, user);
    await expect(canvas.getByRole('button', { name: /^優先度/ })).toHaveTextContent('高');
    await expect(canvas.getByRole('button', { name: /^担当者/ })).toHaveTextContent('yupix');
    await expect(canvas.getByRole('button', { name: /^日付/ })).toHaveTextContent('7/13');
    await expect(canvas.getByRole('button', { name: /^日付/ })).toHaveTextContent('7/31');
    await waitFor(() =>
      expect(canvasElement.ownerDocument.querySelector('[data-similar-tasks]')).not.toBeNull(),
    );
  },
};

/** 日付のパネルを開いたまま（期限の行が選択中。× とカレンダーが見える） */
export const DateMenuOpen: Story = {
  beforeEach() {
    const mock = mockCreate(201, similarTasks);
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await fillAll(canvas, user);
    await user.click(canvas.getByRole('button', { name: /^日付/ }));
    await waitFor(() =>
      expect(canvasElement.ownerDocument.querySelector(DATE_PANEL)).not.toBeNull(),
    );
    await expect(canvas.getByRole('button', { name: '期限を消す' })).toBeInTheDocument();
  },
};

/**
 * 日付を設定して × で消したあとの Esc は、パネルだけ閉じてダイアログを閉じない。
 * × は消すと DOM から外れるので、フォーカスが body に落ちると Esc がパネルを通らず dialog の cancel になる。
 * user-event の Esc は合成で cancel を起こさないため、フォーカスがパネルの中に残り、Esc でパネルが閉じることで見る
 */
export const DateClearThenEscape: Story = {
  beforeEach() {
    openChangeSpy.mockClear();
    const mock = mockCreate();
    return mock.restore;
  },
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    const canvas = within(doc.body);
    const user = userEvent.setup();
    await canvas.findByLabelText(/タイトル/);
    await pickDate(user, canvas, 'softDeadline', '2026-07-15');
    // 日を選んだあとも、フォーカスはパネルの中に残る
    await waitFor(() =>
      expect(doc.querySelector(DATE_PANEL)!.contains(doc.activeElement)).toBe(true),
    );

    await user.click(canvas.getByRole('button', { name: '期限を消す' }));
    await waitFor(() => expect(canvas.queryByRole('button', { name: '期限を消す' })).toBeNull());
    await expect(doc.activeElement).toBe(doc.querySelector('[data-date-field="softDeadline"]'));

    await user.keyboard('{Escape}');
    await waitFor(() => expect(doc.querySelector(DATE_PANEL)).toBeNull());
    await expect(openChangeSpy).not.toHaveBeenCalled();
    // 日付チップへ戻る
    await expect(doc.activeElement).toBe(canvas.getByRole('button', { name: /^日付/ }));
  },
};

/** 優先度のメニューを開いたまま（色の旗と、選択中のチェック印） */
export const PriorityMenuOpen: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: /^優先度/ }));
    await expect(await canvas.findByRole('option', { name: '中' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    // 各行に優先度の色の旗が出る（引き金のチップと同じ旗。行の名前には混ざらない）
    const high = await canvas.findByRole('option', { name: '高' });
    const flag = [...high.querySelectorAll<SVGElement>('svg')].find((svg) => svg.style.color);
    await expect(flag).toBeTruthy();
    await expect(flag!.style.color).not.toBe('');
  },
};

/** 幅 390px。本文だけがスクロールし、チップ列とフッターは下に留まる */
export const Narrow390: Story = {
  beforeEach() {
    const mock = mockCreate();
    return mock.restore;
  },
  parameters: {
    viewport: {
      options: { narrow390: { name: '390', styles: { width: '390px', height: '844px' } } },
    },
  },
  globals: { viewport: { value: 'narrow390', isRotated: false } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.type(await canvas.findByLabelText(/タイトル/), 'ログイン画面のエラー表示を直す');
    await user.click(canvas.getByLabelText('説明'));
    for (let i = 1; i <= 30; i += 1) {
      // 先頭の「- 」は Enter で次の行に引き継がれる
      await user.keyboard(`${i === 1 ? '- ' : ''}手順 ${i}: 入力欄の下にエラーを出す{Enter}`);
    }
    const doc = canvasElement.ownerDocument;
    const chips = doc.querySelector<HTMLElement>('[data-task-chips]')!;
    const submit = canvas.getByRole('button', { name: '作成' });
    // 説明が長くても、チップ列とフッターは画面の中に残る
    await waitFor(() => {
      for (const el of [chips, submit]) {
        if (el.getBoundingClientRect().bottom > window.innerHeight) {
          throw new Error('チップ列かフッターが画面の外に出ている');
        }
      }
    });
    // 説明の冒頭はスクロールの中なので、本文の領域は内容より低い
    const body = chips.parentElement!.previousElementSibling as HTMLElement;
    await expect(body.scrollHeight).toBeGreaterThan(body.clientHeight);
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
    await pickDate(user, canvas, 'softDeadline', '2026-07-15');
    await pickDate(user, canvas, 'hardDeadline', '2026-07-31');
    await user.type(canvas.getByLabelText('見積もり'), '90');
    await user.type(canvas.getByLabelText('進捗率'), '30');
    await user.click(canvas.getByRole('button', { name: /^優先度/ }));
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
      progress_pct: 30,
      hard_deadline: '2026-07-31T00:00:00.000Z',
    });
    await expect(invalidateQueriesSpy).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks'],
    });
    await expect(invalidateQueriesSpy).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search'],
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
    // Esc は dialog の cancel として届く。user-event の keydown は合成なので、ブラウザは cancel を起こさない
    titleInput.closest('dialog')!.dispatchEvent(new Event('cancel', { cancelable: true }));
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
