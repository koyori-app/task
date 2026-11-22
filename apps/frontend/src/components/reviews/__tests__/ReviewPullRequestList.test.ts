import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import ReviewPullRequestList from '../ReviewPullRequestList.vue';
import {
  DEFAULT_REVIEW_LIST_URL_STATE,
  parseReviewListUrlState,
  type ReviewListUrlState,
} from '@/lib/review-url-state';
import {
  PROJECT_ID,
  TENANT_ID,
  bodyButton,
  menuItem,
  pullRequest,
  stubFetch,
  type MockState,
} from './review-fixtures';

function newQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

function mountList(
  urlState: Partial<ReviewListUrlState> = {},
  options: { warnings?: string[]; queryClient?: QueryClient } = {},
) {
  return mount(ReviewPullRequestList, {
    props: {
      tenantId: TENANT_ID,
      tenantSlug: 'acme',
      projectId: PROJECT_ID,
      projectKey: 'APP',
      initialUrlState: { ...DEFAULT_REVIEW_LIST_URL_STATE, ...urlState },
      initialUrlWarnings: options.warnings ?? [],
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient: options.queryClient ?? newQueryClient() }]],
    },
    attachTo: document.body,
  });
}

/** ページの境界（10 件）を越える件数。番号の大きい順に、最終レビューが新しい */
function manyPullRequests(count: number, state = 'open') {
  return Array.from({ length: count }, (_, i) =>
    pullRequest({
      pr_number: 300 - i,
      pr_title: `fix: 一覧の修正 ${300 - i}`,
      pr_author: i % 2 === 0 ? 'yupix' : 'hana',
      state,
    }),
  );
}

const rowNumbers = (wrapper: ReturnType<typeof mountList>) =>
  wrapper.findAll('[data-testid="pr-row"]').map((row) => row.text().match(/#(\d+)/)?.[1]);

const urlState = () => parseReviewListUrlState(new URL(window.location.href).searchParams).state;

enableAutoUnmount(afterEach);

beforeEach(() => {
  window.history.replaceState(null, '', '/acme/projects/APP/reviews');
  window.sessionStorage.clear();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ReviewPullRequestList', () => {
  it('行に PR・作成者・件数・Round を出し、題と矢印は詳細へのリンクにする', async () => {
    stubFetch({
      pullRequests: [
        pullRequest({
          pr_number: 231,
          pr_title: 'タスク一覧の絞り込みを URL に持つ',
          rounds: 3,
          blocking: 4,
          blocking_fixed: 1,
          unresolved: 7,
          unresolved_fixed: 2,
        }),
      ],
    });
    const wrapper = mountList();
    await flushPromises();

    const row = wrapper.get('[data-testid="pr-row"]');
    expect(row.text()).toContain('#231');
    expect(row.text()).toContain('Open · acme/app');
    expect(row.text()).toContain('yupix');
    expect(row.text()).toContain('R3');
    const title = row.get('[data-testid="pr-title"]');
    expect(title.text()).toBe('タスク一覧の絞り込みを URL に持つ');
    expect(title.attributes('href')).toBe('/acme/projects/APP/reviews/231');
    expect(row.get('a[aria-label="#231 を開く"]').attributes('href')).toBe(
      '/acme/projects/APP/reviews/231',
    );
    // 行そのものはクリックを持たない
    expect(row.attributes('onclick')).toBeUndefined();

    // 件数は 3 列で数え方を揃え、色を分ける
    const cell = (id: string) => row.get(`[data-testid="pr-${id}"]`);
    expect(cell('high-medium').text()).toBe('3');
    expect(cell('high-medium').get('[data-tone]').attributes('data-tone')).toBe('danger');
    expect(cell('low-nit').text()).toBe('2');
    expect(cell('low-nit').get('[data-tone]').attributes('data-tone')).toBe('neutral');
    expect(cell('awaiting').text()).toBe('2');
    expect(cell('awaiting').get('[data-tone]').attributes('data-tone')).toBe('warning');
  });

  it('0 件の列はチップにせず、題・作成者・Round・最終レビューが無ければ代わりの表示にする', async () => {
    stubFetch({
      pullRequests: [
        pullRequest({
          pr_number: 9,
          pr_title: null,
          pr_author: null,
          rounds: 0,
          last_reviewed_at: null,
          repository: null,
          state: 'unknown',
        }),
      ],
    });
    const wrapper = mountList();
    await flushPromises();

    const row = wrapper.get('[data-testid="pr-row"]');
    expect(row.get('[data-testid="pr-title"]').text()).toBe('PR #9');
    expect(row.get('[data-testid="pr-state"]').text()).toBe('状態不明');
    expect(row.get('[data-testid="pr-high-medium"]').text()).toBe('0');
    expect(row.find('[data-testid="pr-high-medium"] [data-tone]').exists()).toBe(false);
    expect(row.findAll('td').filter((td) => td.text() === '—')).toHaveLength(3);
  });

  it('11 件目から次のページに分かれ、ページを URL に保つ', async () => {
    stubFetch({ pullRequests: manyPullRequests(11) });
    const wrapper = mountList();
    await flushPromises();

    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(10);
    expect(wrapper.text()).toContain('1–10 / 11 件');

    const nav = wrapper.get('nav[aria-label="PR のページ"]');
    nav
      .findAll('button')
      .find((b) => b.text().includes('次へ'))!
      .element.click();
    await flushPromises();

    expect(rowNumbers(wrapper)).toEqual(['290']);
    expect(urlState().page).toBe(2);

    nav
      .findAll('button')
      .find((b) => b.text().includes('前へ'))!
      .element.click();
    await flushPromises();
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(10);
    expect(new URL(window.location.href).searchParams.has('page')).toBe(false);
  });

  it('範囲外のページは最後のページに寄せ、理由を出す', async () => {
    stubFetch({ pullRequests: manyPullRequests(11) });
    const wrapper = mountList({ page: 9 });
    await flushPromises();

    expect(rowNumbers(wrapper)).toEqual(['290']);
    expect(wrapper.get('[data-testid="url-warning"]').text()).toContain(
      'URL のページ 9 はありません（全 2 ページ）。最後のページを表示しています。',
    );
    expect(urlState().page).toBe(2);
  });

  it('開いた直後は Open の PR だけを出し、「すべて」を選ぶと pr_state に載る', async () => {
    stubFetch({
      pullRequests: [
        pullRequest({ pr_number: 3 }),
        pullRequest({ pr_number: 2, state: 'merged' }),
        pullRequest({ pr_number: 1, state: 'closed' }),
      ],
    });
    const wrapper = mountList();
    await flushPromises();

    expect(rowNumbers(wrapper)).toEqual(['3']);
    expect(bodyButton('状態: Open')).toBeDefined();
    // 選んでいない既定は URL に載せない
    expect(window.location.search).toBe('');

    menuItem(wrapper.element, 'すべて').click();
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['3', '2', '1']);
    expect(urlState().prState).toBe('all');
  });

  it('Open の PR が 1 件も無ければ「すべて」で開き、一覧を空にしない', async () => {
    stubFetch({ pullRequests: manyPullRequests(2, 'merged') });
    const wrapper = mountList();
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['300', '299']);
    expect(bodyButton('状態: すべて')).toBeDefined();
  });

  it('Open が無い一覧でも、URL で明示した Open は「すべて」へ倒さない', async () => {
    stubFetch({ pullRequests: manyPullRequests(2, 'merged') });
    const wrapper = mountList({ prState: 'open' });
    await flushPromises();
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(0);
    expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain('Open の PR はありません');
  });

  it('URL のページが Open の一覧には無く「すべて」なら在るときは「すべて」で開く', async () => {
    stubFetch({
      pullRequests: [pullRequest({ pr_number: 500 }), ...manyPullRequests(11, 'merged')],
    });
    const wrapper = mountList({ page: 2 });
    await flushPromises();
    expect(bodyButton('状態: すべて')).toBeDefined();
    expect(wrapper.find('[data-testid="url-warning"]').exists()).toBe(false);
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(2);
  });

  it('一覧がキャッシュで揃った再マウントでも、Open が無ければ「すべて」で開く', async () => {
    stubFetch({ pullRequests: manyPullRequests(2, 'merged') });
    const queryClient = newQueryClient();
    const first = mountList({}, { queryClient });
    await flushPromises();
    first.unmount();

    const second = mountList({}, { queryClient });
    await flushPromises();
    expect(rowNumbers(second)).toEqual(['300', '299']);
  });

  it('検索・作成者・未解決・最終レビュー・並びが URL に載り、絞り込むと 1 ページ目へ戻る', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: new Date('2026-08-27T00:00:00Z') });
    stubFetch({
      pullRequests: [
        ...manyPullRequests(11),
        pullRequest({
          pr_number: 12,
          pr_title: '共有リンクの期限',
          pr_author: 'sota',
          unresolved: 2,
          last_reviewed_at: '2026-06-01T00:00:00Z',
        }),
      ],
    });
    const wrapper = mountList({ page: 2 });
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['290', '12']);

    await wrapper.get('input[type="search"]').setValue('共有');
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['12']);
    expect(urlState()).toMatchObject({ q: '共有', page: null });

    await wrapper.get('input[type="search"]').setValue('');
    menuItem(wrapper.element, 'sota').click();
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['12']);
    expect(urlState().author).toBe('sota');
    expect(bodyButton('作成者: sota')).toBeDefined();

    menuItem(wrapper.element, 'あり').click();
    await flushPromises();
    expect(urlState().unresolved).toBe('yes');
    expect(rowNumbers(wrapper)).toEqual(['12']);

    menuItem(wrapper.element, '30 日以内').click();
    await flushPromises();
    expect(urlState().reviewed).toBe('30d');
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(0);
    // 条件で外れただけなので、外すボタンを出す
    expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain(
      '条件に合う、sota が作成した Open の PR はありません',
    );
    bodyButton('条件を外す')!.click();
    await flushPromises();
    expect(urlState()).toMatchObject({ author: null, unresolved: 'all', reviewed: 'all' });
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(10);

    menuItem(wrapper.element, '番号が大きい順').click();
    await flushPromises();
    expect(urlState().sort).toBe('number');
    expect(rowNumbers(wrapper)[0]).toBe('300');
    vi.useRealTimers();
  });

  it('検索欄は 200 文字までしか入らない（URL から読む上限と揃う）', async () => {
    stubFetch({});
    const wrapper = mountList();
    await flushPromises();
    expect(wrapper.get('input[type="search"]').attributes('maxlength')).toBe('200');
  });

  it('絞り込みのたびに、いまのクエリを詳細の「PR の一覧」のために控える', async () => {
    stubFetch({ pullRequests: manyPullRequests(3) });
    const wrapper = mountList();
    await flushPromises();
    expect(window.sessionStorage.getItem(`reviews:list:${PROJECT_ID}`)).toBe('');

    await wrapper.get('input[type="search"]').setValue('fix');
    expect(window.sessionStorage.getItem(`reviews:list:${PROJECT_ID}`)).toBe('?q=fix');
  });

  it('未解決が多い順は、High・Medium の残りが多い PR を先に並べる', async () => {
    stubFetch({
      pullRequests: [
        pullRequest({ pr_number: 1, blocking: 1, unresolved: 1 }),
        pullRequest({ pr_number: 2, blocking: 3, unresolved: 3 }),
        pullRequest({ pr_number: 3 }),
      ],
    });
    const wrapper = mountList({ sort: 'unresolved' });
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['2', '1', '3']);
  });

  describe('自分の PR', () => {
    it('連携済みなら自分が作成した PR だけに絞り、URL に mine=1 を載せる', async () => {
      stubFetch({ pullRequests: manyPullRequests(11), githubLogin: 'HANA' });
      const wrapper = mountList({ page: 2 });
      await flushPromises();

      const toggle = wrapper.get('[role="switch"]');
      expect(toggle.attributes('disabled')).toBeUndefined();
      // 説明は見えない形で残し、スイッチと結び付ける
      expect(toggle.attributes('aria-describedby')).toBe('mine-only-help');
      expect(wrapper.get('#mine-only-help').classes()).toContain('sr-only');
      expect(wrapper.get('input[type="search"]').attributes('aria-label')).toBe(
        '番号・タイトルで探す',
      );
      expect(wrapper.get('[data-testid="mine-only-help"]').text()).toBe(
        'GitHub @HANA で判定 · 5 件',
      );

      await toggle.trigger('click');
      await flushPromises();
      expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(5);
      expect(
        wrapper.findAll('[data-testid="pr-row"]').every((r) => r.text().includes('hana')),
      ).toBe(true);
      expect(urlState()).toMatchObject({ mine: true, page: null });
    });

    it('GitHub と未連携ならスイッチを無効にし、URL の mine=1 は理由を出して外す', async () => {
      window.history.replaceState(null, '', '/acme/projects/APP/reviews?mine=1');
      stubFetch({ pullRequests: manyPullRequests(3) });
      const wrapper = mountList({ mine: true });
      await flushPromises();

      expect(wrapper.get('[role="switch"]').attributes('disabled')).toBeDefined();
      expect(wrapper.get('[data-testid="mine-only-help"]').text()).toBe(
        'GitHub と連携すると使えます。PR の作成者を GitHub のユーザー名で判定するためです。',
      );
      expect(wrapper.get('[data-testid="url-warning"]').text()).toContain(
        'GitHub と連携していないため、URL の「自分の PR だけ」は無視しました。',
      );
      expect(urlState().mine).toBe(false);
      expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(3);
    });

    it('連携はあるのにユーザー名が分からなければ、未連携と言わず再連携を案内する', async () => {
      stubFetch({ pullRequests: manyPullRequests(3), githubLinkedWithoutLogin: true });
      const wrapper = mountList({ mine: true });
      await flushPromises();

      expect(wrapper.get('[role="switch"]').attributes('disabled')).toBeDefined();
      expect(wrapper.get('[data-testid="mine-only-help"]').text()).toContain(
        'アカウント設定で GitHub を連携し直すと使えます。',
      );
      expect(wrapper.get('[data-testid="url-warning"]').text()).toContain(
        'GitHub のユーザー名を確認できないため、URL の「自分の PR だけ」は無視しました。',
      );
    });

    it('連携一覧が届かなくても PR の一覧は先に出し、スイッチだけ待たせる', async () => {
      stubFetch({ pullRequests: manyPullRequests(3), hangConnections: true });
      const wrapper = mountList();
      await flushPromises();

      expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(3);
      expect(wrapper.get('[role="switch"]').attributes('disabled')).toBeDefined();
      expect(wrapper.get('[data-testid="mine-only-help"]').text()).toBe(
        '連携状態を確認しています…',
      );
    });

    it('連携一覧の取得に失敗しても未連携とは言わず、URL の mine=1 も消さない', async () => {
      window.history.replaceState(null, '', '/acme/projects/APP/reviews?mine=1');
      stubFetch({ pullRequests: manyPullRequests(3), connectionsStatus: 500 });
      const wrapper = mountList({ mine: true });
      await flushPromises();

      expect(wrapper.get('[data-testid="mine-only-help"]').text()).toContain(
        '連携状態を取得できなかったため使えません。',
      );
      expect(wrapper.get('[data-testid="url-warning"]').text()).toContain(
        '連携状態を取得できなかったため、URL の「自分の PR だけ」を適用できませんでした。',
      );
      expect(new URL(window.location.href).searchParams.get('mine')).toBe('1');
      // 作成者で絞れていないので、全件が出る
      expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(3);
    });

    it('一致なしのとき、作成者で絞れている間だけ「すべての PR から探す」を出す', async () => {
      stubFetch({ pullRequests: manyPullRequests(3), githubLogin: 'nobody' });
      const wrapper = mountList({ mine: true, q: '共有' });
      await flushPromises();

      expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain(
        '「共有」に一致する、自分が作成した Open の PR はありません',
      );
      expect(bodyButton('すべての状態を表示')).toBeDefined();
      bodyButton('すべての PR から探す')!.click();
      await flushPromises();
      expect(urlState().mine).toBe(false);
      expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain(
        '「共有」に一致する Open の PR はありません',
      );
      expect(bodyButton('すべての PR から探す')).toBeUndefined();
    });
  });

  it('不正な query の警告を出し、操作で URL を書き直したら片付ける', async () => {
    stubFetch({ pullRequests: manyPullRequests(3) });
    const wrapper = mountList(
      {},
      { warnings: ['URL の 並び「x」は知らない値のため無視しました。'] },
    );
    await flushPromises();
    expect(wrapper.get('[data-testid="url-warning"]').text()).toContain('並び');

    await wrapper.get('input[type="search"]').setValue('fix');
    expect(wrapper.find('[data-testid="url-warning"]').exists()).toBe(false);
  });

  it('vike の遷移（props の更新）でも URL の状態を復元する', async () => {
    stubFetch({
      pullRequests: [...manyPullRequests(2), pullRequest({ pr_number: 7, state: 'merged' })],
    });
    const wrapper = mountList();
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['300', '299']);

    await wrapper.setProps({
      initialUrlState: { ...DEFAULT_REVIEW_LIST_URL_STATE, prState: 'merged', sort: 'number' },
    });
    await flushPromises();
    expect(rowNumbers(wrapper)).toEqual(['7']);
    expect(bodyButton('状態: Merged')).toBeDefined();
  });

  it('読み込み中は骨組みを出す', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise<Response>(() => {})),
    );
    const wrapper = mountList();
    await flushPromises();
    expect(wrapper.find('[data-testid="pr-list-loading"]').exists()).toBe(true);
    expect(wrapper.find('table').exists()).toBe(false);
  });

  it('読み込みに失敗したら理由と再試行を出し、再試行で取り直す', async () => {
    const state: MockState = { pullRequests: manyPullRequests(2), prsStatus: 500 };
    stubFetch(state);
    const wrapper = mountList();
    await flushPromises();

    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain('エラー');
    expect(alert.text()).toContain('レビューを読み込めませんでした');
    expect(wrapper.find('[data-testid="pr-list-empty"]').exists()).toBe(false);

    state.prsStatus = undefined;
    bodyButton('再試行')!.click();
    await flushPromises();
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(2);
  });

  it('レビューが 1 件も無ければ、絞り込みを出さずにそう伝える', async () => {
    stubFetch({ pullRequests: [] });
    const wrapper = mountList();
    await flushPromises();

    const empty = wrapper.get('[data-testid="pr-list-empty"]');
    expect(empty.text()).toContain('レビューはまだありません');
    expect(empty.text()).toContain('PR にレビューを出すと、ここに並びます。');
    expect(wrapper.find('[data-testid="pr-filters"]').exists()).toBe(false);
  });

  it('絞った状態に PR が無ければ、すべての状態へ戻すボタンを出す', async () => {
    stubFetch({ pullRequests: manyPullRequests(2) });
    const wrapper = mountList({ prState: 'closed' });
    await flushPromises();

    expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain(
      'Closed の PR はありません',
    );
    expect(bodyButton('すべての PR から探す')).toBeUndefined();
    expect(bodyButton('条件を外す')).toBeUndefined();
    bodyButton('すべての状態を表示')!.click();
    await flushPromises();
    expect(wrapper.findAll('[data-testid="pr-row"]')).toHaveLength(2);
    expect(urlState().prState).toBe('all');
  });
});
