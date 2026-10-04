import type { components } from '@/generated/api';
import type { ReviewedPullRequest } from '@/lib/review-findings';

type OAuthConnectionItem = components['schemas']['OAuthConnectionItem'];

/** PR 一覧の 1 ページの件数。 */
export const PR_PAGE_SIZE = 10;

/**
 * 閲覧者が連携している GitHub（クラウド版）のユーザー名。未連携なら null。
 * PR の作成者（`pr_author`）は GitHub のユーザー名なので、これと突き合わせる。
 * アプリのユーザー名とは一致するとは限らない。
 */
export function viewerGithubLogin(connections: readonly OAuthConnectionItem[]): string | null {
  const github = connections.find(
    (c) => c.provider === 'github' && !c.instance_url && !!c.provider_login,
  );
  return github?.provider_login ?? null;
}

/**
 * GitHub（クラウド版）と連携しているか。ユーザー名が分からない連携も含む——
 * 名前を控える前からある連携で、保存済みのトークンでも補完できなかったもの。
 * その場合は未連携と言わず、再連携を案内する。
 */
export function hasGithubConnection(connections: readonly OAuthConnectionItem[]): boolean {
  return connections.some((c) => c.provider === 'github' && !c.instance_url);
}

export function isMyPullRequest(pr: ReviewedPullRequest, login: string | null): boolean {
  return !!login && !!pr.pr_author && pr.pr_author.toLowerCase() === login.toLowerCase();
}

/**
 * 検索と「自分の PR だけ」を当てた一覧（ページ分割の前の全件）。
 * 検索は番号・タイトルの部分一致で、先頭の `#` は無視する。`mineLogin` が null なら作成者で絞らない。
 */
export function filterPullRequests(
  list: readonly ReviewedPullRequest[],
  query: string,
  mineLogin: string | null,
): ReviewedPullRequest[] {
  const q = query.trim().toLowerCase().replace(/^#/, '');
  return list.filter(
    (pr) =>
      (mineLogin === null || isMyPullRequest(pr, mineLogin)) &&
      (q === '' ||
        String(pr.pr_number).includes(q) ||
        (pr.pr_title ?? '').toLowerCase().includes(q)),
  );
}

export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PR_PAGE_SIZE));
}

/** その PR が載っているページ。一覧に無ければ null。 */
export function pageOfPullRequest(
  list: readonly ReviewedPullRequest[],
  prNumber: number | null,
): number | null {
  const index = list.findIndex((pr) => pr.pr_number === prNumber);
  return index < 0 ? null : Math.floor(index / PR_PAGE_SIZE) + 1;
}
