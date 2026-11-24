import type { components } from '@/generated/api';
import type { ReviewedPullRequest } from '@/lib/review-findings';

type OAuthConnectionItem = components['schemas']['OAuthConnectionItem'];

/** PR 一覧の状態（ホスト上の PR の状態）の絞り込み。`all` は絞らない。既定は `open`。 */
export const PR_STATE_FILTERS = ['all', 'open', 'merged', 'closed', 'unknown'] as const;
export type PrStateFilter = (typeof PR_STATE_FILTERS)[number];

/** 開いた直後の「PR の状態」。 */
export const DEFAULT_PR_STATE_FILTER: PrStateFilter = 'open';

/**
 * 開いた直後の「PR の状態」。既定は Open だけだが、Open の PR が 1 件も無い一覧
 * （全件マージ済み、webhook を受けておらず全件 `unknown` の CLI 運用など）では
 * 空の一覧を見せず「すべて」で開く。
 *
 * URL に `pr_state` が無いとき（選んでいない・状態を載せる前の URL）に使う。URL の `page`
 * （`urlPage`）が Open の一覧には無く「すべて」の一覧なら在るときは、「すべて」でページを
 * 送った URL とみなして「すべて」で開く。
 * 作成者の絞り込み（`mine`）は連携一覧を待つため、ここでは数えない。
 */
export function initialPrStateFilter(
  list: readonly ReviewedPullRequest[],
  urlPage: number | null = null,
): PrStateFilter {
  if (list.length === 0) return DEFAULT_PR_STATE_FILTER;
  const openCount = list.filter((pr) => pr.state === DEFAULT_PR_STATE_FILTER).length;
  if (openCount === 0) return 'all';
  if (urlPage !== null && urlPage > pageCount(openCount) && urlPage <= pageCount(list.length)) {
    return 'all';
  }
  return DEFAULT_PR_STATE_FILTER;
}

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

export const PR_UNRESOLVED_FILTERS = ['all', 'yes', 'no'] as const;
export type PrUnresolvedFilter = (typeof PR_UNRESOLVED_FILTERS)[number];
export const PR_REVIEWED_FILTERS = ['all', '7d', '30d', '90d'] as const;
export type PrReviewedFilter = (typeof PR_REVIEWED_FILTERS)[number];
export const PR_SORTS = ['recent', 'unresolved', 'number'] as const;
export type PrSort = (typeof PR_SORTS)[number];

export type PrListFilters = {
  /** 番号・タイトルの検索語 */
  q: string;
  state: PrStateFilter;
  /** 作成者（GitHub のユーザー名）。null は絞らない */
  author: string | null;
  /** 「自分の PR」で絞るときの、閲覧者の GitHub のユーザー名。null は絞らない */
  mineLogin: string | null;
  unresolved: PrUnresolvedFilter;
  reviewed: PrReviewedFilter;
};

const DAY_MS = 24 * 60 * 60 * 1000;
const REVIEWED_DAYS: Record<Exclude<PrReviewedFilter, 'all'>, number> = {
  '7d': 7,
  '30d': 30,
  '90d': 90,
};

function sameLogin(a: string | null | undefined, b: string): boolean {
  return !!a && a.toLowerCase() === b.toLowerCase();
}

/**
 * 絞り込みを当てた一覧（並べ替えとページ分割の前の全件）。
 * 検索は番号・タイトルの部分一致で、先頭の `#` は無視する。最終レビューの期間は
 * 境界ちょうどを含み、レビューの時刻が無い PR（写しだけ）は期間を選ぶと外れる。
 */
export function filterPullRequests(
  list: readonly ReviewedPullRequest[],
  filters: PrListFilters,
  now: Date,
): ReviewedPullRequest[] {
  const q = filters.q.trim().toLowerCase().replace(/^#/, '');
  const since =
    filters.reviewed === 'all' ? null : now.getTime() - REVIEWED_DAYS[filters.reviewed] * DAY_MS;
  return list.filter(
    (pr) =>
      (filters.mineLogin === null || isMyPullRequest(pr, filters.mineLogin)) &&
      (filters.author === null || sameLogin(pr.pr_author, filters.author)) &&
      (filters.state === 'all' || pr.state === filters.state) &&
      (filters.unresolved === 'all' || pr.unresolved > 0 === (filters.unresolved === 'yes')) &&
      (since === null ||
        (!!pr.last_reviewed_at && new Date(pr.last_reviewed_at).getTime() >= since)) &&
      (q === '' ||
        String(pr.pr_number).includes(q) ||
        (pr.pr_title ?? '').toLowerCase().includes(q)),
  );
}

function reviewedTime(pr: ReviewedPullRequest): number {
  return pr.last_reviewed_at ? new Date(pr.last_reviewed_at).getTime() : 0;
}

/**
 * 並べ替え。`recent` は API の順（最終レビューの新しい順）のまま。`unresolved` は
 * High・Medium の残り（承認待ちを除く）→ 未解決の数 → 最終レビューの新しい順。
 */
export function sortPullRequests(
  list: readonly ReviewedPullRequest[],
  sort: PrSort,
): ReviewedPullRequest[] {
  const out = [...list];
  if (sort === 'number') return out.sort((a, b) => b.pr_number - a.pr_number);
  if (sort === 'unresolved') {
    return out.sort(
      (a, b) =>
        b.blocking - b.blocking_fixed - (a.blocking - a.blocking_fixed) ||
        b.unresolved - a.unresolved ||
        reviewedTime(b) - reviewedTime(a),
    );
  }
  return out;
}

/** 作成者の候補。重複なし（大文字小文字を無視）で、大文字小文字を無視した昇順。 */
export function pullRequestAuthors(list: readonly ReviewedPullRequest[]): string[] {
  const seen = new Map<string, string>();
  for (const pr of list) {
    if (pr.pr_author && !seen.has(pr.pr_author.toLowerCase())) {
      seen.set(pr.pr_author.toLowerCase(), pr.pr_author);
    }
  }
  return [...seen.values()].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}

/**
 * 一覧の 3 列の件数。High・Medium と Low・Nit は承認待ち（fixed）を除いた残りで、
 * 承認待ちは重大度を問わず数える。3 つを足すと `unresolved` になる。
 */
export function pullRequestCounts(pr: ReviewedPullRequest): {
  highMedium: number;
  lowNit: number;
  awaiting: number;
} {
  return {
    highMedium: pr.blocking - pr.blocking_fixed,
    lowNit: pr.unresolved - pr.blocking - (pr.unresolved_fixed - pr.blocking_fixed),
    awaiting: pr.unresolved_fixed,
  };
}

/** 最終レビューの経過（「30 分前」「昨日」「1 週間前」）。5 週間からは日付。 */
export function reviewedAgo(iso: string, now: Date): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return '';
  const diffMs = now.getTime() - then.getTime();
  if (diffMs < 60_000) return 'たった今';
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) return `${minutes} 分前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 時間前`;
  const days = Math.floor(hours / 24);
  if (days === 1) return '昨日';
  if (days < 7) return `${days} 日前`;
  if (days < 35) return `${Math.floor(days / 7)} 週間前`;
  return then.toLocaleDateString('ja-JP', { year: 'numeric', month: 'numeric', day: 'numeric' });
}

const PR_STATE_LABELS: Record<Exclude<PrStateFilter, 'all'>, string> = {
  open: 'Open',
  merged: 'Merged',
  closed: 'Closed',
  unknown: '状態不明',
};

/** PR の状態の絞り込みのラベル。 */
export function prStateFilterLabel(state: PrStateFilter): string {
  return state === 'all' ? 'すべて' : PR_STATE_LABELS[state];
}

/** 条件に合う PR が無いときの文。効いている条件から組み立てる。 */
export function noMatchMessage(filters: PrListFilters): string {
  const q = filters.q.trim();
  const parts: string[] = [];
  if (q !== '') parts.push(`「${q}」に一致する`);
  if (filters.unresolved !== 'all' || filters.reviewed !== 'all') parts.push('条件に合う');
  if (filters.mineLogin !== null) parts.push('自分が作成した');
  else if (filters.author !== null) parts.push(`${filters.author} が作成した`);
  const state = filters.state === 'all' ? '' : `${prStateFilterLabel(filters.state)} の `;
  return `${parts.join('、')}${parts.length > 0 ? ' ' : ''}${state}PR はありません`;
}

export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PR_PAGE_SIZE));
}
