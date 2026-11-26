import { describe, expect, it } from 'vitest';
import type { ReviewedPullRequest } from '@/lib/review-findings';
import {
  DEFAULT_PR_STATE_FILTER,
  filterPullRequests,
  hasGithubConnection,
  initialPrStateFilter,
  isMyPullRequest,
  noMatchMessage,
  pageCount,
  pullRequestAuthors,
  pullRequestCounts,
  reviewedAgo,
  sortPullRequests,
  viewerGithubLogin,
  type PrListFilters,
} from '@/lib/review-pr-list';

function pr(
  prNumber: number,
  title: string | null,
  author: string | null,
  state = 'open',
  overrides: Partial<ReviewedPullRequest> = {},
): ReviewedPullRequest {
  return {
    state,
    pr_number: prNumber,
    rounds: 1,
    pr_title: title,
    pr_author: author,
    unresolved: 0,
    blocking: 0,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    repository: 'acme/app',
    host: 'github',
    html_url: null,
    last_reviewed_at: '2026-09-01T00:00:00Z',
    ...overrides,
  };
}

const NONE: PrListFilters = {
  q: '',
  state: 'all',
  author: null,
  mineLogin: null,
  unresolved: 'all',
  reviewed: 'all',
};
const NOW = new Date('2026-09-10T00:00:00Z');
const numbers = (list: readonly ReviewedPullRequest[]) => list.map((p) => p.pr_number);

describe('review PR list', () => {
  const list = [
    pr(231, 'drive: 共有リンクに有効期限を追加', 'haru'),
    pr(228, 'tasks: タイマーの多重起動を防ぐ', 'Mio', 'merged'),
    pr(9, null, null, 'unknown'),
  ];
  const filtered = (filters: Partial<PrListFilters>, source = list, now = NOW) =>
    numbers(filterPullRequests(source, { ...NONE, ...filters }, now));

  it('既定の PR の状態は Open', () => {
    expect(DEFAULT_PR_STATE_FILTER).toBe('open');
  });

  it('開いた直後の PR の状態は、Open の PR があれば Open、無ければ「すべて」', () => {
    expect(initialPrStateFilter(list)).toBe('open');
    expect(
      initialPrStateFilter([pr(228, null, null, 'merged'), pr(9, null, null, 'unknown')]),
    ).toBe('all');
    // webhook を受けていない CLI 運用は全件 unknown
    expect(initialPrStateFilter([pr(9, null, null, 'unknown'), pr(8, null, null, 'unknown')])).toBe(
      'all',
    );
    // 空の一覧は既定のまま（画面は「レビューはまだありません」で別扱い）
    expect(initialPrStateFilter([])).toBe('open');
  });

  it('URL のページが Open の一覧には無く「すべて」なら在るときは「すべて」で開く', () => {
    // Open 1 件（1 ページ）と Merged 20 件。「すべて」は 21 件で 3 ページ
    const mixed = [
      pr(300, null, null),
      ...Array.from({ length: 20 }, (_, i) => pr(200 + i, null, null, 'merged')),
    ];
    expect(initialPrStateFilter(mixed, 2)).toBe('all');
    expect(initialPrStateFilter(mixed, 3)).toBe('all');
    // Open の一覧に在るページ、どちらにも無いページ、ページ指定なしは既定のまま
    expect(initialPrStateFilter(mixed, 1)).toBe('open');
    expect(initialPrStateFilter(mixed, 4)).toBe('open');
    expect(initialPrStateFilter(mixed, null)).toBe('open');
  });

  it('連携一覧からクラウド版 GitHub のユーザー名だけを拾う', () => {
    expect(
      viewerGithubLogin([
        { provider: 'gitlab', provider_login: 'gl', connected_at: '', sign_in: true },
        {
          provider: 'github',
          provider_login: 'ghe',
          instance_url: 'https://ghe.example',
          connected_at: '',
          sign_in: true,
        },
        { provider: 'github', provider_login: 'mio', connected_at: '', sign_in: true },
      ]),
    ).toBe('mio');
    expect(viewerGithubLogin([{ provider: 'github', connected_at: '', sign_in: true }])).toBeNull();
  });

  it('ユーザー名が分からない GitHub 連携も、連携ありとして見分ける', () => {
    expect(hasGithubConnection([{ provider: 'github', connected_at: '', sign_in: true }])).toBe(
      true,
    );
    expect(
      hasGithubConnection([
        {
          provider: 'github',
          instance_url: 'https://ghe.example',
          connected_at: '',
          sign_in: true,
        },
        { provider: 'gitlab', provider_login: 'gl', connected_at: '', sign_in: true },
      ]),
    ).toBe(false);
  });

  it('自分の PR は大文字小文字を区別せずに見分ける', () => {
    expect(isMyPullRequest(list[1], 'mio')).toBe(true);
    expect(isMyPullRequest(list[1], null)).toBe(false);
    expect(isMyPullRequest(list[2], 'mio')).toBe(false);
  });

  it('10 件で 1 ページ。11 件目から 2 ページ目になる', () => {
    expect(pageCount(0)).toBe(1);
    expect(pageCount(10)).toBe(1);
    expect(pageCount(11)).toBe(2);
  });

  describe('filterPullRequests', () => {
    it('番号・タイトルの部分一致で探し、先頭の # は無視する', () => {
      expect(filtered({ q: '#23' })).toEqual([231]);
      expect(filtered({ q: '共有リンク' })).toEqual([231]);
      expect(filtered({ q: 'TIMER' })).toEqual([]);
      expect(filtered({ q: 'Tasks' })).toEqual([228]);
    });

    it('PR の状態で絞り、all は絞らない', () => {
      expect(filtered({ state: 'merged' })).toEqual([228]);
      expect(filtered({ state: 'unknown' })).toEqual([9]);
      expect(filtered({ state: 'closed' })).toEqual([]);
      expect(filtered({ state: 'all' })).toHaveLength(3);
      // 検索とは AND
      expect(filtered({ q: '231', state: 'merged' })).toEqual([]);
    });

    it('作成者と「自分の PR」は大文字小文字を区別せず、作成者が無い PR は外す', () => {
      expect(filtered({ author: 'mio' })).toEqual([228]);
      expect(filtered({ mineLogin: 'HARU' })).toEqual([231]);
      expect(filtered({ author: 'mio', mineLogin: 'haru' })).toEqual([]);
    });

    it('未解決のあり / なしで絞る', () => {
      const source = [pr(1, null, null, 'open', { unresolved: 2 }), pr(2, null, null)];
      expect(filtered({ unresolved: 'yes' }, source)).toEqual([1]);
      expect(filtered({ unresolved: 'no' }, source)).toEqual([2]);
    });

    it('最終レビューの期間は境界ちょうどを含み、1 ミリ秒越えと null は外す', () => {
      const day = 24 * 60 * 60 * 1000;
      const at = (ms: number) => new Date(NOW.getTime() - ms).toISOString();
      const source = [
        pr(1, null, null, 'open', { last_reviewed_at: at(7 * day) }),
        pr(2, null, null, 'open', { last_reviewed_at: at(7 * day + 1) }),
        pr(3, null, null, 'open', { last_reviewed_at: at(30 * day) }),
        pr(4, null, null, 'open', { last_reviewed_at: at(90 * day + 1) }),
        pr(5, null, null, 'open', { last_reviewed_at: null }),
      ];
      expect(filtered({ reviewed: '7d' }, source)).toEqual([1]);
      expect(filtered({ reviewed: '30d' }, source)).toEqual([1, 2, 3]);
      expect(filtered({ reviewed: '90d' }, source)).toEqual([1, 2, 3]);
      expect(filtered({ reviewed: 'all' }, source)).toEqual([1, 2, 3, 4, 5]);
    });
  });

  describe('sortPullRequests', () => {
    const source = [
      pr(10, null, null, 'open', {
        blocking: 1,
        unresolved: 1,
        last_reviewed_at: '2026-09-05T00:00:00Z',
      }),
      pr(30, null, null, 'open', {
        blocking: 3,
        blocking_fixed: 1,
        unresolved: 3,
        last_reviewed_at: '2026-09-04T00:00:00Z',
      }),
      pr(20, null, null, 'open', {
        blocking: 1,
        unresolved: 4,
        last_reviewed_at: '2026-09-03T00:00:00Z',
      }),
      pr(40, null, null, 'open', {
        blocking: 1,
        unresolved: 4,
        last_reviewed_at: '2026-09-06T00:00:00Z',
      }),
    ];

    it('recent は API の順のまま', () => {
      expect(numbers(sortPullRequests(source, 'recent'))).toEqual([10, 30, 20, 40]);
    });

    it('unresolved は High・Medium の残り、未解決の数、最終レビューの順で比べる', () => {
      expect(numbers(sortPullRequests(source, 'unresolved'))).toEqual([30, 40, 20, 10]);
    });

    it('number は番号が大きい順', () => {
      expect(numbers(sortPullRequests(source, 'number'))).toEqual([40, 30, 20, 10]);
    });

    it('入力を書き換えない', () => {
      sortPullRequests(source, 'number');
      expect(numbers(source)).toEqual([10, 30, 20, 40]);
    });
  });

  it('作成者の候補は重複なしで、大文字小文字を無視した昇順', () => {
    expect(
      pullRequestAuthors([
        pr(1, null, 'sota'),
        pr(2, null, 'Hana'),
        pr(3, null, null),
        pr(4, null, 'hana'),
        pr(5, null, 'yupix'),
      ]),
    ).toEqual(['Hana', 'sota', 'yupix']);
  });

  it('件数は High・Medium と Low・Nit から承認待ちを除き、承認待ちは全重大度で数える', () => {
    expect(
      pullRequestCounts(
        pr(1, null, null, 'open', {
          blocking: 4,
          blocking_fixed: 1,
          unresolved: 7,
          unresolved_fixed: 2,
        }),
      ),
    ).toEqual({ highMedium: 3, lowNit: 2, awaiting: 2 });
    expect(pullRequestCounts(pr(1, null, null))).toEqual({ highMedium: 0, lowNit: 0, awaiting: 0 });
  });

  it('最終レビューは経過で言い、5 週間からは日付にする', () => {
    const ago = (ms: number) => reviewedAgo(new Date(NOW.getTime() - ms).toISOString(), NOW);
    const sec = 1000;
    const min = 60 * sec;
    const hour = 60 * min;
    const day = 24 * hour;
    expect(ago(59 * sec)).toBe('たった今');
    expect(ago(min)).toBe('1 分前');
    expect(ago(59 * min)).toBe('59 分前');
    expect(ago(hour)).toBe('1 時間前');
    expect(ago(23 * hour)).toBe('23 時間前');
    expect(ago(day)).toBe('昨日');
    expect(ago(2 * day)).toBe('2 日前');
    expect(ago(6 * day)).toBe('6 日前');
    expect(ago(7 * day)).toBe('1 週間前');
    expect(ago(34 * day)).toBe('4 週間前');
    expect(ago(35 * day)).toMatch(/^2026\/8\/\d{1,2}$/);
    expect(reviewedAgo('not-a-date', NOW)).toBe('');
  });

  it('条件に合う PR が無いときの文を、効いている条件から組み立てる', () => {
    expect(noMatchMessage({ ...NONE, q: ' 共有 ' })).toBe('「共有」に一致する PR はありません');
    expect(noMatchMessage({ ...NONE, q: '共有', mineLogin: 'yupix', state: 'open' })).toBe(
      '「共有」に一致する、自分が作成した Open の PR はありません',
    );
    expect(noMatchMessage({ ...NONE, state: 'merged' })).toBe('Merged の PR はありません');
    expect(noMatchMessage({ ...NONE, author: 'hana' })).toBe('hana が作成した PR はありません');
    expect(noMatchMessage({ ...NONE, unresolved: 'yes', reviewed: '7d' })).toBe(
      '条件に合う PR はありません',
    );
  });
});
