import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import ReviewPullRequestDetail from '../ReviewPullRequestDetail.vue';
import {
  DEFAULT_REVIEW_DETAIL_URL_STATE,
  parseReviewDetailUrlState,
  type ReviewDetailUrlState,
} from '@/lib/review-url-state';
import {
  OTHER_ID,
  PROJECT_ID,
  TENANT_ID,
  VIEWER_ID,
  bodyButton,
  finding,
  menuItem,
  pullRequest,
  round,
  stubFetch,
  type MockState,
} from './review-fixtures';

const navigate = vi.hoisted(() => vi.fn());
vi.mock('vike/client/router', () => ({ navigate }));

const DETAIL_PATH = '/acme/projects/APP/reviews/618';
const HEAD_SHA = 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678';

function mountDetail(
  urlState: Partial<ReviewDetailUrlState> = {},
  options: { warnings?: string[]; prNumber?: number } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ReviewPullRequestDetail, {
    props: {
      tenantId: TENANT_ID,
      tenantSlug: 'acme',
      projectId: PROJECT_ID,
      projectKey: 'APP',
      prNumber: options.prNumber ?? 618,
      viewerId: VIEWER_ID,
      initialUrlState: { ...DEFAULT_REVIEW_DETAIL_URL_STATE, ...urlState },
      initialUrlWarnings: options.warnings ?? [],
    },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

/** 2 ファイル + ファイルの指定なし。sortFindings の順は a1, b1, a2, n1, a3 */
function sampleFindings() {
  return [
    finding({
      id: 'a1',
      severity: 'high',
      state: 'open',
      file: 'src/lib/a.ts',
      title: 'A の未解決',
    }),
    finding({
      id: 'a2',
      severity: 'medium',
      state: 'verified',
      file: 'src/lib/a.ts',
      title: 'A の確認済み',
      available_actions: [],
    }),
    finding({
      id: 'a3',
      severity: 'low',
      state: 'fixed',
      file: 'src/lib/a.ts',
      title: 'A の承認待ち',
      round: 2,
      available_actions: ['verified', 'open'],
    }),
    finding({
      id: 'b1',
      severity: 'high',
      state: 'verified',
      file: 'src/pages/b.vue',
      title: 'B の確認済み',
      available_actions: [],
    }),
    finding({
      id: 'n1',
      severity: 'medium',
      state: 'fixed',
      file: null,
      line: null,
      title: 'ファイルなしの承認待ち',
      available_actions: ['verified', 'open'],
    }),
  ];
}

const rowIds = (wrapper: ReturnType<typeof mountDetail>) =>
  wrapper.findAll('[data-testid="finding-row"]').map((row) => row.attributes('id'));
const selectedId = (wrapper: ReturnType<typeof mountDetail>) =>
  wrapper.element.querySelector('[data-testid="finding-row"][data-selected]')?.id;
const urlState = () => parseReviewDetailUrlState(new URL(window.location.href).searchParams).state;
const press = (key: string) =>
  document.body.dispatchEvent(
    new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }),
  );

enableAutoUnmount(afterEach);

beforeEach(() => {
  window.history.replaceState(null, '', DETAIL_PATH);
  window.sessionStorage.clear();
  navigate.mockReset();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ReviewPullRequestDetail', () => {
  it('見出しに番号・作成者・状態・リポジトリと題を出し、ホストのロゴつきのリンクを出す', async () => {
    stubFetch({
      pullRequests: [pullRequest({ pr_title: 'タスク一覧の絞り込みを URL に持つ' })],
      findings: sampleFindings(),
    });
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.get('[data-testid="pr-meta"]').text()).toBe('#618 · yupix · Open · acme/app');
    expect(wrapper.get('h1').text()).toBe('タスク一覧の絞り込みを URL に持つ');
    const open = wrapper.get('[data-testid="open-on-host"]');
    expect(open.element.tagName).toBe('A');
    expect(open.text()).toBe('GitHub で開く');
    expect(open.attributes('href')).toBe('https://github.com/acme/app/pull/618');
    expect(open.attributes('target')).toBe('_blank');
    expect(open.attributes('rel')).toBe('noopener noreferrer');
    expect(open.find('svg').exists()).toBe(true);
  });

  it('GitLab の PR 行なら GitLab のロゴと名前で、行の URL をそのまま使う', async () => {
    stubFetch({
      pullRequests: [
        pullRequest({
          host: 'gitlab',
          html_url: 'https://gitlab.example.com/acme/app/-/merge_requests/618',
        }),
      ],
    });
    const wrapper = mountDetail();
    await flushPromises();
    const open = wrapper.get('[data-testid="open-on-host"]');
    expect(open.text()).toBe('GitLab で開く');
    expect(open.attributes('href')).toBe(
      'https://gitlab.example.com/acme/app/-/merge_requests/618',
    );
  });

  it('指摘をファイルごとにまとめ、見出しと木の節に同じ「解決 / 全体」を出す', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();

    const groups = wrapper.findAll('[data-testid="file-group"]');
    expect(groups.map((g) => g.attributes('aria-label'))).toEqual([
      'src/lib/a.ts',
      'src/pages/b.vue',
      'ファイルの指定なし',
    ]);
    expect(groups.map((g) => g.get('[data-testid="file-progress"]').text())).toEqual([
      '解決 1 / 3',
      '解決 1 / 1',
      '解決 0 / 1',
    ]);
    // 未解決が残れば赤、承認待ちだけなら橙、全部解決なら緑
    expect(groups[0].get('[data-testid="file-progress"]').classes().join(' ')).toContain('danger');
    expect(groups[1].get('[data-testid="file-progress"]').classes().join(' ')).toContain('success');
    expect(groups[2].get('[data-testid="file-progress"]').classes().join(' ')).toContain('warning');
    expect(rowIds(wrapper)).toEqual([
      'finding-a1',
      'finding-a2',
      'finding-a3',
      'finding-b1',
      'finding-n1',
    ]);

    const tree = wrapper.get('[data-testid="file-tree"]');
    expect(tree.text()).toContain('ファイル · 2');
    const items = tree.findAll('[role="treeitem"]');
    expect(items.map((i) => i.text().replace(/\s+/g, ''))).toEqual([
      'src/lib',
      'a.ts1/3',
      'src/pages',
      'b.vue1/1',
      'ファイルの指定なし0/1',
    ]);
    expect(items[1].get('[data-tone]').attributes('data-tone')).toBe('danger');
    expect(items[3].get('[data-tone]').attributes('data-tone')).toBe('success');
    expect(items[4].get('[data-tone]').attributes('data-tone')).toBe('warning');
    expect(wrapper.get('[data-testid="finding-count"]').text()).toBe('5 件 · ↑ ↓ で移る');
  });

  it('木の節を選ぶと、絞り込まずにそのまとまりへ移る', async () => {
    const scrolled: string[] = [];
    vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(function (this: Element) {
      scrolled.push(this.id);
    });
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();

    const items = wrapper.get('[data-testid="file-tree"]').findAll('[role="treeitem"]');
    await items[3].trigger('click');
    await flushPromises();

    expect(scrolled).toEqual(['review-file-group-1']);
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(5);
    expect(items[3].attributes('aria-selected')).toBe('true');
  });

  it('選んだ行だけを開き、もう一度押すと畳む。選択は URL の finding に載る', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.find('[data-testid="finding-detail"]').exists()).toBe(false);

    await wrapper.get('#finding-a1 [data-testid="finding-title"]').trigger('click');
    expect(selectedId(wrapper)).toBe('finding-a1');
    expect(wrapper.findAll('[data-testid="finding-detail"]')).toHaveLength(1);
    expect(urlState().finding).toBe('a1');
    // 選んだ指摘のファイルが、木でも選ばれる
    expect(wrapper.get('[data-testid="file-tree"] [aria-selected="true"]').text()).toContain(
      'a.ts',
    );

    await wrapper.get('#finding-b1 [data-testid="finding-title"]').trigger('click');
    expect(selectedId(wrapper)).toBe('finding-b1');
    expect(wrapper.findAll('[data-testid="finding-detail"]')).toHaveLength(1);

    await wrapper.get('#finding-b1 [data-testid="finding-title"]').trigger('click');
    expect(selectedId(wrapper)).toBeUndefined();
    expect(urlState().finding).toBeNull();
  });

  it('指摘へのリンクは、いまの絞り込みを保った詳細の URL になる', async () => {
    window.history.replaceState(null, '', `${DETAIL_PATH}?round=1&tab=activity`);
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ round: 1 });
    await flushPromises();
    expect(wrapper.get('#finding-a1 [data-testid="finding-title"]').attributes('href')).toBe(
      `${DETAIL_PATH}?tab=activity&round=1&finding=a1`,
    );
  });

  it('URL の finding の行を開き、強制スクロールは一度きりにする', async () => {
    const scrolled: string[] = [];
    vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(function (this: Element) {
      scrolled.push(this.id);
    });
    const state: MockState = { findings: sampleFindings() };
    const { patched } = stubFetch(state);
    const wrapper = mountDetail({ finding: 'a3' });
    await flushPromises();

    expect(selectedId(wrapper)).toBe('finding-a3');
    expect(scrolled).toEqual(['finding-a3']);

    // 状態の変更 → 取り直しで指摘の data が入れ替わっても、画面を勝手に戻さない
    bodyButton('確認した')!.click();
    await flushPromises();
    expect(patched).toHaveLength(1);
    expect(scrolled).toEqual(['finding-a3']);
  });

  it('↑ ↓ で見えている指摘の選択を動かし、端で止まり、Esc で外す', async () => {
    vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();

    press('ArrowUp');
    await flushPromises();
    expect(selectedId(wrapper)).toBeUndefined();

    press('ArrowDown');
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-a1');
    press('ArrowDown');
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-a2');
    press('ArrowUp');
    press('ArrowUp');
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-a1');

    for (let i = 0; i < 9; i += 1) press('ArrowDown');
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-n1');
    expect(urlState().finding).toBe('n1');

    press('Escape');
    await flushPromises();
    expect(selectedId(wrapper)).toBeUndefined();
  });

  it('入力欄にフォーカスがある間と、理由の入力欄を開いている間は ↑ ↓ で動かさない', async () => {
    vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ finding: 'a1' });
    await flushPromises();

    const search = wrapper.get('input[type="search"]').element;
    search.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-a1');

    bodyButton('取り下げる…')!.click();
    await flushPromises();
    press('ArrowDown');
    press('Escape');
    await flushPromises();
    expect(selectedId(wrapper)).toBe('finding-a1');
    expect(wrapper.find('[data-testid="finding-form-reject"]').exists()).toBe(true);
  });

  it('状態と重大度は複数選べ、URL に集合で載る。絞り込んでも選択は外さない', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ finding: 'a1' });
    await flushPromises();
    expect(bodyButton('状態: すべて')).toBeDefined();

    menuItem(wrapper.element, 'Fixed').click();
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['finding-a3', 'finding-n1']);
    expect(bodyButton('状態: Fixed')).toBeDefined();
    expect(urlState()).toMatchObject({ states: ['fixed'], finding: 'a1' });
    // 選択中の指摘が絞り込みで見えなくなったら、そう伝える
    expect(wrapper.get('[data-testid="filtered-finding"]').text()).toContain(
      'URL で指定された指摘は現在の絞り込みでは非表示です。',
    );
    // 件数は絞り込みに関係なく、そのファイルの全指摘で数える
    expect(wrapper.get('[data-testid="file-progress"]').text()).toBe('解決 1 / 3');

    menuItem(wrapper.element, 'Open').click();
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['finding-a1', 'finding-a3', 'finding-n1']);
    expect(bodyButton('状態: 2 件')).toBeDefined();
    expect(wrapper.find('[data-testid="filtered-finding"]').exists()).toBe(false);

    menuItem(wrapper.element, 'Medium').click();
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['finding-n1']);
    expect(urlState()).toMatchObject({ states: ['open', 'fixed'], severities: ['medium'] });
    expect(wrapper.get('[data-testid="finding-count"]').text()).toContain('1 件');
  });

  it('検索は題・本文・ファイルで絞り、q に載る', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();
    await wrapper.get('input[type="search"]').setValue('PAGES/');
    expect(rowIds(wrapper)).toEqual(['finding-b1']);
    expect(urlState().q).toBe('PAGES/');
  });

  it('Round を選ぶと、そのラウンドが見たコミットをリンクで出す', async () => {
    stubFetch({
      findings: sampleFindings(),
      rounds: [
        round(),
        round({
          id: 'r-2',
          round: 2,
          head_sha: HEAD_SHA,
          reviewer: { id: VIEWER_ID, username: 'yupix', avatar_url: null },
        }),
      ],
    });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.find('[data-testid="round-commit"]').exists()).toBe(false);

    // 項目は文字だけ（コミットへのリンクは置かない）
    const item = menuItem(wrapper.element, 'R2 · a1b2c3d · yupix');
    expect(item.querySelector('a')).toBeNull();
    item.click();
    await flushPromises();

    expect(rowIds(wrapper)).toEqual(['finding-a3']);
    expect(urlState().round).toBe(2);
    expect(bodyButton('Round: R2')).toBeDefined();
    const line = wrapper.get('[data-testid="round-commit"]');
    expect(line.text().replace(/\s+/g, ' ')).toContain('R2 は a1b2c3d を見ています · yupix');
    const link = line.get('a');
    expect(link.attributes('href')).toBe(`https://github.com/acme/app/commit/${HEAD_SHA}`);
    expect(link.attributes('title')).toBe(HEAD_SHA);
  });

  it('レビュアーで絞ると、その人のラウンドの指摘だけを出す', async () => {
    stubFetch({
      findings: sampleFindings(),
      rounds: [
        round(),
        round({
          id: 'r-2',
          round: 2,
          reviewer: { id: VIEWER_ID, username: 'yupix', avatar_url: null },
        }),
      ],
    });
    const wrapper = mountDetail();
    await flushPromises();
    menuItem(wrapper.element, 'yupix').click();
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['finding-a3']);
    expect(urlState().reviewer).toBe(VIEWER_ID);
    expect(bodyButton('レビュアー: yupix')).toBeDefined();

    menuItem(wrapper.element, 'reviewer').click();
    await flushPromises();
    expect(urlState().reviewer).toBe(OTHER_ID);
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(4);
  });

  it('マージ判定の「未解決を見る」で、塞いでいる指摘だけに絞る', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ q: 'B の', round: 1 });
    await flushPromises();
    expect(wrapper.get('[data-testid="merge-gate"]').text()).toContain(
      'High・Medium の未解決が 2 件（うち承認待ち 1）',
    );

    bodyButton('未解決を見る')!.click();
    await flushPromises();
    expect(urlState()).toEqual({
      q: '',
      states: ['open', 'fixing', 'fixed'],
      severities: ['high', 'medium'],
      round: null,
      reviewer: null,
      finding: null,
    });
    expect(rowIds(wrapper)).toEqual(['finding-a1', 'finding-n1']);
    expect(bodyButton('状態: 未解決')).toBeDefined();
  });

  it('条件に合う指摘が無ければそう伝え、「条件を外す」で戻す', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ states: ['rejected'] });
    await flushPromises();

    expect(wrapper.get('[data-testid="no-finding-match"]').text()).toContain(
      '条件に合う指摘はありません',
    );
    expect(wrapper.find('[data-testid="file-tree"]').exists()).toBe(false);
    bodyButton('条件を外す')!.click();
    await flushPromises();
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(5);
    expect(window.location.search).toBe('');
  });

  it('状態の遷移を理由つきで送り、一覧と件数に反映する', async () => {
    const { patched } = stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ finding: 'a1' });
    await flushPromises();

    bodyButton('取り下げる…')!.click();
    await flushPromises();
    await wrapper.get('#finding-a1 textarea').setValue('仕様どおりだった');
    await wrapper.get('[data-testid="finding-form-confirm"]').trigger('click');
    await flushPromises();

    expect(patched).toHaveLength(1);
    expect(patched[0].path.endsWith('/review-findings/a1')).toBe(true);
    expect(patched[0].body).toEqual({ state: 'rejected', note: '仕様どおりだった' });
    expect(wrapper.get('#finding-a1').text()).toContain('Rejected');
    expect(wrapper.get('[data-testid="file-progress"]').text()).toBe('解決 2 / 3');
    expect(wrapper.find('[data-testid="finding-form-reject"]').exists()).toBe(false);
  });

  it('遷移に失敗したら、理由をその行の中に出して取り直す', async () => {
    const { calls } = stubFetch({
      findings: sampleFindings(),
      patchStatus: 409,
      patchMessage: 'High / Medium の指摘は繰り延べできません',
    });
    const wrapper = mountDetail({ finding: 'a1' });
    await flushPromises();
    const before = calls.filter((c) => c.endsWith('/review-findings')).length;

    bodyButton('修正した')!.click();
    await flushPromises();

    expect(wrapper.get('#finding-a1 [role="alert"]').text()).toContain(
      'High / Medium の指摘は繰り延べできません',
    );
    expect(calls.filter((c) => c.endsWith('/review-findings')).length).toBeGreaterThan(before);
  });

  it('403 と、スラグだけの 409 は状態コードごとの文に倒す', async () => {
    const state: MockState = {
      findings: sampleFindings(),
      patchStatus: 403,
      patchMessage: 'forbidden',
    };
    stubFetch(state);
    const wrapper = mountDetail({ finding: 'a1' });
    await flushPromises();

    bodyButton('修正した')!.click();
    await flushPromises();
    expect(wrapper.get('#finding-a1 [role="alert"]').text()).toContain(
      'この操作はレビュー側だけが行えます（自分の修正は自分で確認できません）。',
    );

    state.patchStatus = 409;
    state.patchMessage = 'conflict';
    bodyButton('修正した')!.click();
    await flushPromises();
    expect(wrapper.get('#finding-a1 [role="alert"]').text()).toContain(
      'いまの状態からは行えない操作です。画面を再読み込みしてください。',
    );
  });

  it('指摘の取得に失敗したら「指摘はありません」ではなく失敗と再試行を出す', async () => {
    const state: MockState = { findings: sampleFindings(), findingsStatus: 500 };
    stubFetch(state);
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.find('[data-testid="no-findings"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="load-error"]').text()).toContain(
      '指摘の読み込みに失敗しました',
    );

    state.findingsStatus = undefined;
    await wrapper.get('[data-testid="load-error"] button').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(5);
  });

  it('マージ判定の取得に失敗したら、判定を出さずに再試行を出す', async () => {
    const state: MockState = { findings: sampleFindings(), summaryStatus: 500 };
    stubFetch(state);
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.find('[data-testid="merge-gate"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('マージ判定を読み込めませんでした');
    // 判定が読めなくても、指摘は出す
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(5);

    state.summaryStatus = undefined;
    bodyButton('再試行')!.click();
    await flushPromises();
    expect(wrapper.get('[data-testid="merge-gate"]').text()).toContain('マージ不可');
  });

  it('一覧に無い PR は、未レビューの PR として出す', async () => {
    stubFetch({
      pullRequests: [pullRequest({ pr_number: 1 })],
      findings: [],
      rounds: [],
      summary: { gate: 'unreviewed', rounds: 0, latest_head_sha: null },
    });
    const wrapper = mountDetail({}, { prNumber: 999 });
    await flushPromises();

    expect(wrapper.get('h1').text()).toBe('PR #999');
    expect(wrapper.get('[data-testid="pr-meta"]').text()).toBe('#999 · acme/app');
    expect(wrapper.find('[data-testid="open-on-host"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="merge-gate"]').text()).toContain('未レビュー');
    const empty = wrapper.get('[data-testid="no-findings"]');
    expect(empty.text()).toContain('この PR に指摘はありません');
    expect(empty.find('button').text()).toBe('指摘を起票');
  });

  it('PR の一覧を読めなくても、指摘と集計が読めれば出す', async () => {
    stubFetch({ findings: sampleFindings(), prsStatus: 500 });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.get('h1').text()).toBe('PR #618');
    expect(wrapper.findAll('[data-testid="finding-row"]')).toHaveLength(5);
    expect(wrapper.find('[data-testid="merge-gate"]').exists()).toBe(true);
  });

  it('URL の Round・指摘がこの PR に無ければ理由を出す', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail({ round: 9, finding: 'gone' });
    await flushPromises();
    expect(wrapper.get('[data-testid="missing-round"]').text()).toContain(
      'URL で指定された Round 9 はこの PR にありません。',
    );
    expect(wrapper.get('[data-testid="missing-finding"]').text()).toContain(
      'URL で指定された指摘 gone はこの PR にありません。',
    );
  });

  it('不正な query の警告を出し、操作で URL を書き直したら片付ける', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail(
      {},
      { warnings: ['URL の 状態「bogus」は知らない値のため無視しました。'] },
    );
    await flushPromises();
    expect(wrapper.get('[data-testid="url-warning"]').text()).toContain('bogus');

    await wrapper.get('#finding-a1 [data-testid="finding-title"]').trigger('click');
    expect(wrapper.find('[data-testid="url-warning"]').exists()).toBe(false);
  });

  it('vike の遷移（props の更新）でも URL の状態を復元する', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();

    await wrapper.setProps({
      initialUrlState: { ...DEFAULT_REVIEW_DETAIL_URL_STATE, states: ['verified'], finding: 'b1' },
    });
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['finding-a2', 'finding-b1']);
    expect(selectedId(wrapper)).toBe('finding-b1');
  });

  it('絞り込みと選択は履歴を積まず、いまの URL を差し替える', async () => {
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();
    const push = vi.spyOn(window.history, 'pushState');
    const length = window.history.length;

    menuItem(wrapper.element, 'Open').click();
    await wrapper.get('#finding-a1 [data-testid="finding-title"]').trigger('click');
    await flushPromises();
    expect(push).not.toHaveBeenCalled();
    expect(window.history.length).toBe(length);
    expect(urlState()).toMatchObject({ states: ['open'], finding: 'a1' });
  });

  describe('起票', () => {
    async function openComposer() {
      const state: MockState = { findings: sampleFindings() };
      const mocks = stubFetch(state);
      const wrapper = mountDetail();
      await flushPromises();
      await wrapper.get('[data-testid="compose"]').trigger('click');
      return { wrapper, state, ...mocks };
    }

    it('「指摘を起票」で開き、開いている間は見出しのボタンを出さない。キャンセルで閉じる', async () => {
      const { wrapper } = await openComposer();
      const composer = wrapper.get('[data-testid="round-composer"]');
      expect(composer.text()).toContain('新しいラウンド（R2）を起票');
      expect(wrapper.find('[data-testid="compose"]').exists()).toBe(false);

      bodyButton('キャンセル')!.click();
      await flushPromises();
      expect(wrapper.find('[data-testid="round-composer"]').exists()).toBe(false);
      expect(wrapper.find('[data-testid="compose"]').exists()).toBe(true);
    });

    it('確定すると閉じて、指摘と集計を取り直す', async () => {
      const { wrapper, posted, calls } = await openComposer();
      await wrapper.find('#composer-head').setValue(HEAD_SHA);
      const before = calls.filter((c) => c.endsWith('/review-findings')).length;
      bodyButton('確定（0 件）')!.click();
      await flushPromises();

      expect(posted).toHaveLength(1);
      expect(posted[0]).toMatchObject({ pr_number: 618, head_sha: HEAD_SHA });
      expect(wrapper.find('[data-testid="round-composer"]').exists()).toBe(false);
      expect(calls.filter((c) => c.endsWith('/review-findings')).length).toBeGreaterThan(before);
    });

    it('下書きを抱えたまま「PR の一覧」を押すと確認し、やめると下書きが残る', async () => {
      const { wrapper } = await openComposer();
      await wrapper.find('#draft-title').setValue('書きかけ');

      const back = wrapper.get('[data-testid="back-to-list"]');
      const click = new MouseEvent('click', { bubbles: true, cancelable: true });
      back.element.dispatchEvent(click);
      await flushPromises();
      expect(click.defaultPrevented).toBe(true);
      expect(document.body.textContent).toContain('起票中の下書きは、一覧へ戻ると失われます。');

      bodyButton('キャンセル')!.click();
      await flushPromises();
      expect(navigate).not.toHaveBeenCalled();
      expect((wrapper.find('#draft-title').element as HTMLInputElement).value).toBe('書きかけ');

      back.element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      await flushPromises();
      bodyButton('破棄して戻る')!.click();
      await flushPromises();
      expect(navigate).toHaveBeenCalledWith('/acme/projects/APP/reviews');
    });

    it('下書きを抱えている間だけ、タブを閉じる・再読み込みを止める', async () => {
      const { wrapper } = await openComposer();
      const unload = () => {
        const event = new Event('beforeunload', { cancelable: true });
        window.dispatchEvent(event);
        return event.defaultPrevented;
      };
      expect(unload()).toBe(false);
      await wrapper.find('#draft-title').setValue('書きかけ');
      expect(unload()).toBe(true);
    });

    it('下書きが無ければ、「PR の一覧」は確認なしのリンクのまま', async () => {
      const { wrapper } = await openComposer();
      const click = new MouseEvent('click', { bubbles: true, cancelable: true });
      wrapper.get('[data-testid="back-to-list"]').element.dispatchEvent(click);
      expect(click.defaultPrevented).toBe(false);
    });
  });

  it('「PR の一覧」は、一覧が控えた絞り込みを付けて戻る', async () => {
    window.sessionStorage.setItem(`reviews:list:${PROJECT_ID}`, '?q=fix&pr_state=all');
    stubFetch({ findings: sampleFindings() });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.get('[data-testid="back-to-list"]').attributes('href')).toBe(
      '/acme/projects/APP/reviews?q=fix&pr_state=all',
    );
  });
});
