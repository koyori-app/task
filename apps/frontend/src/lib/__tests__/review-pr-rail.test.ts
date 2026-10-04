import { describe, expect, it } from 'vitest';
import type { ReviewedPullRequest } from '@/lib/review-findings';
import {
  filterPullRequests,
  hasGithubConnection,
  pageCount,
  pageOfPullRequest,
  viewerGithubLogin,
} from '@/lib/review-pr-rail';

function pr(prNumber: number, title: string | null, author: string | null): ReviewedPullRequest {
  return {
    pr_number: prNumber,
    rounds: 1,
    pr_title: title,
    pr_author: author,
    unresolved: 0,
    blocking: 0,
    last_reviewed_at: '2026-09-01T00:00:00Z',
  };
}

describe('review PR rail', () => {
  const list = [
    pr(231, 'drive: 共有リンクに有効期限を追加', 'haru'),
    pr(228, 'tasks: タイマーの多重起動を防ぐ', 'Mio'),
    pr(9, null, null),
  ];

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
        { provider: 'gitlab', provider_login: 'gl', connected_at: '' },
        {
          provider: 'github',
          provider_login: 'ghe',
          instance_url: 'https://ghe.example',
          connected_at: '',
        },
        { provider: 'github', provider_login: 'mio', connected_at: '' },
      ]),
    ).toBe('mio');
    expect(viewerGithubLogin([{ provider: 'github', connected_at: '' }])).toBeNull();
  });

  it('ユーザー名が分からない GitHub 連携も、連携ありとして見分ける', () => {
    expect(hasGithubConnection([{ provider: 'github', connected_at: '' }])).toBe(true);
    expect(
      hasGithubConnection([
        { provider: 'github', instance_url: 'https://ghe.example', connected_at: '' },
        { provider: 'gitlab', provider_login: 'gl', connected_at: '' },
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
