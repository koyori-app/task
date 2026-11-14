import { describe, expect, it } from 'vitest';
import type { ReviewedPullRequest } from '@/lib/review-findings';
import {
  DEFAULT_PR_STATE_FILTER,
  filterPullRequests,
  hasGithubConnection,
  initialPrStateFilter,
  pageCount,
  pageOfPullRequest,
  viewerGithubLogin,
} from '@/lib/review-pr-rail';

function pr(
  prNumber: number,
  title: string | null,
  author: string | null,
  state = 'open',
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
    last_reviewed_at: '2026-09-01T00:00:00Z',
  };
}

describe('review PR rail', () => {
  const list = [
    pr(231, 'drive: 共有リンクに有効期限を追加', 'haru'),
    pr(228, 'tasks: タイマーの多重起動を防ぐ', 'Mio', 'merged'),
    pr(9, null, null, 'unknown'),
  ];

  it('PR の状態で絞り、all は絞らない', () => {
    expect(filterPullRequests(list, '', null, 'merged').map((p) => p.pr_number)).toEqual([228]);
    expect(filterPullRequests(list, '', null, 'unknown').map((p) => p.pr_number)).toEqual([9]);
    expect(filterPullRequests(list, '', null, 'closed')).toEqual([]);
    expect(filterPullRequests(list, '', null, 'all')).toHaveLength(3);
    // 検索とは AND
    expect(filterPullRequests(list, '231', null, 'merged')).toEqual([]);
  });

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

  it('番号・タイトルの部分一致で探し、先頭の # は無視する', () => {
    expect(filterPullRequests(list, '#23', null).map((p) => p.pr_number)).toEqual([231]);
    expect(filterPullRequests(list, '共有リンク', null).map((p) => p.pr_number)).toEqual([231]);
    expect(filterPullRequests(list, 'TIMER', null)).toEqual([]);
    expect(filterPullRequests(list, 'Tasks', null).map((p) => p.pr_number)).toEqual([228]);
  });

  it('作成者で絞るときは大文字小文字を区別せず、作成者が無い PR は外す', () => {
    expect(filterPullRequests(list, '', 'mio').map((p) => p.pr_number)).toEqual([228]);
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

  it('10 件で 1 ページ。11 件目から 2 ページ目になる', () => {
    expect(pageCount(0)).toBe(1);
    expect(pageCount(10)).toBe(1);
    expect(pageCount(11)).toBe(2);
    const many = Array.from({ length: 11 }, (_, i) => pr(100 + i, null, null));
    expect(pageOfPullRequest(many, 109)).toBe(1);
    expect(pageOfPullRequest(many, 110)).toBe(2);
    expect(pageOfPullRequest(many, 999)).toBeNull();
  });
});
