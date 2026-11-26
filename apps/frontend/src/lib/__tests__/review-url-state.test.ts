import { describe, expect, it } from 'vitest';

import {
  DEFAULT_REVIEW_DETAIL_URL_STATE,
  DEFAULT_REVIEW_LIST_URL_STATE,
  applyReviewDetailUrlState,
  applyReviewListUrlState,
  legacyReviewDetailHref,
  parsePrNumber,
  parseReviewDetailUrlState,
  parseReviewListUrlState,
  relativeReviewDetailFindingHref,
  reviewDetailHref,
  reviewListHref,
  type ReviewDetailUrlState,
  type ReviewListUrlState,
} from '@/lib/review-url-state';

const LIST_URL = 'https://app.example.com/acme/projects/APP/reviews';
const DETAIL_URL = 'https://app.example.com/acme/projects/APP/reviews/738';

describe('PR の一覧の URL の状態', () => {
  it('クエリが無ければ既定値で、既定値は URL に載せない', () => {
    expect(parseReviewListUrlState(undefined)).toEqual({
      state: DEFAULT_REVIEW_LIST_URL_STATE,
      warnings: [],
    });
    const url = applyReviewListUrlState(
      new URL(
        `${LIST_URL}?q=old&pr_state=all&author=x&unresolved=yes&reviewed=7d&mine=1&sort=number&page=4`,
      ),
      DEFAULT_REVIEW_LIST_URL_STATE,
    );
    expect(url.search).toBe('');
  });

  it('書いた URL を読み直すと同じ状態になる', () => {
    const state: ReviewListUrlState = {
      q: '検索 語',
      prState: 'merged',
      author: 'hana',
      unresolved: 'yes',
      reviewed: '30d',
      mine: true,
      sort: 'unresolved',
      page: 3,
    };
    const url = applyReviewListUrlState(new URL(LIST_URL), state);
    expect(parseReviewListUrlState(url.searchParams)).toEqual({ state, warnings: [] });
  });

  it('この画面が持たないクエリは残す', () => {
    const url = applyReviewListUrlState(new URL(`${LIST_URL}?tab=activity&q=old`), {
      ...DEFAULT_REVIEW_LIST_URL_STATE,
      q: 'new',
    });
    expect(url.searchParams.get('tab')).toBe('activity');
    expect(url.searchParams.get('q')).toBe('new');
  });

  it('転送できずに残った旧 URL の pr は、書き直すときに消す', () => {
    const url = applyReviewListUrlState(new URL(`${LIST_URL}?pr=abc&q=old`), {
      ...DEFAULT_REVIEW_LIST_URL_STATE,
      q: 'new',
    });
    expect(url.search).toBe('?q=new');
  });

  it('選んだ PR の状態は既定と同じ open でも載せ、1 ページ目は載せない', () => {
    const url = applyReviewListUrlState(new URL(LIST_URL), {
      ...DEFAULT_REVIEW_LIST_URL_STATE,
      prState: 'open',
      page: 1,
    });
    expect(url.searchParams.get('pr_state')).toBe('open');
    expect(url.searchParams.has('page')).toBe(false);
  });

  it('不正な値は警告を出して捨てる', () => {
    const parsed = parseReviewListUrlState({
      pr_state: 'draft',
      unresolved: 'maybe',
      reviewed: '1y',
      mine: 'yes',
      sort: 'random',
      page: '0',
      author: 'a'.repeat(101),
    });
    expect(parsed.state).toEqual(DEFAULT_REVIEW_LIST_URL_STATE);
    expect(parsed.warnings).toHaveLength(7);
    expect(parsed.warnings.join(' ')).toContain('PR の状態');
    expect(parsed.warnings.join(' ')).toContain('自分の PR だけ');
    expect(parsed.warnings.join(' ')).toContain('ページ');
  });

  it('検索語は 200 文字で黙って切り、書く側も同じ上限で切る', () => {
    const parsed = parseReviewListUrlState({ q: 'a'.repeat(250) });
    expect(parsed.state.q).toBe('a'.repeat(200));
    expect(parsed.warnings).toEqual([]);
    const written = applyReviewListUrlState(new URL(LIST_URL), {
      ...DEFAULT_REVIEW_LIST_URL_STATE,
      q: 'b'.repeat(250),
    });
    expect(written.searchParams.get('q')).toBe('b'.repeat(200));
  });
});

describe('PR の詳細の URL の状態', () => {
  it('クエリが無ければ既定値で、既定値は URL に載せない', () => {
    expect(parseReviewDetailUrlState(undefined)).toEqual({
      state: DEFAULT_REVIEW_DETAIL_URL_STATE,
      warnings: [],
    });
    const url = applyReviewDetailUrlState(
      new URL(`${DETAIL_URL}?q=a&state=open&severity=high&round=2&reviewer=u1&finding=f1`),
      DEFAULT_REVIEW_DETAIL_URL_STATE,
    );
    expect(url.search).toBe('');
  });

  it('書いた URL を読み直すと同じ状態になる', () => {
    const state: ReviewDetailUrlState = {
      q: 'url',
      states: ['open', 'fixing', 'fixed'],
      severities: ['high', 'medium'],
      round: 2,
      reviewer: '00000000-0000-0000-0000-0000000000bb',
      finding: '4f20d810-3851-4cd0-95b1-bf17bdaf715c',
    };
    const url = applyReviewDetailUrlState(new URL(DETAIL_URL), state);
    expect(url.searchParams.get('state')).toBe('open,fixing,fixed');
    expect(url.searchParams.get('severity')).toBe('high,medium');
    expect(parseReviewDetailUrlState(url.searchParams)).toEqual({ state, warnings: [] });
  });

  it('旧 URL の単一値を読める', () => {
    expect(parseReviewDetailUrlState({ state: 'open', severity: 'high' }).state).toMatchObject({
      states: ['open'],
      severities: ['high'],
    });
  });

  it('知らない値は警告を出して捨て、知っている値だけ残す', () => {
    const parsed = parseReviewDetailUrlState({ state: 'open,bogus', severity: 'urgent' });
    expect(parsed.state.states).toEqual(['open']);
    expect(parsed.state.severities).toEqual([]);
    expect(parsed.warnings).toHaveLength(2);
    expect(parsed.warnings[0]).toContain('bogus');
  });

  it('重複は 1 つにまとめ、決まった順に並べる', () => {
    expect(parseReviewDetailUrlState({ state: 'fixed,open,fixed' }).state.states).toEqual([
      'open',
      'fixed',
    ]);
  });

  it('不正な Round・レビュアー・指摘 ID は警告を出して捨てる', () => {
    const parsed = parseReviewDetailUrlState({
      round: '-1',
      reviewer: '<script>',
      finding: '<script>',
    });
    expect(parsed.state).toEqual(DEFAULT_REVIEW_DETAIL_URL_STATE);
    expect(parsed.warnings).toHaveLength(3);
    expect(parsed.warnings.join(' ')).toContain('指摘 ID');
  });

  it('この画面が持たないクエリは残す', () => {
    const url = applyReviewDetailUrlState(new URL(`${DETAIL_URL}?tab=activity&round=9`), {
      ...DEFAULT_REVIEW_DETAIL_URL_STATE,
      finding: 'f-1',
    });
    expect(url.searchParams.get('tab')).toBe('activity');
    expect(url.searchParams.has('round')).toBe(false);
    expect(url.searchParams.get('finding')).toBe('f-1');
  });

  it('指摘へのリンクは SSR と client の土台で同じ文字列になり、画面の外のクエリを保つ', () => {
    // SSR は requestUrl を仮の origin に載せ、client は window の絶対 URL を使う。
    // 同じ属性が両者で別の文字列になれば hydration が食い違う
    const state: ReviewDetailUrlState = { ...DEFAULT_REVIEW_DETAIL_URL_STATE, round: 2 };
    const ssr = relativeReviewDetailFindingHref(
      new URL('/acme/projects/APP/reviews/738?round=2&tab=activity', 'http://ssr.local'),
      state,
      'f-9',
    );
    const client = relativeReviewDetailFindingHref(
      new URL(`${DETAIL_URL}?round=2&tab=activity`),
      state,
      'f-9',
    );
    expect(ssr).toBe(client);
    expect(ssr.startsWith('/acme/projects/APP/reviews/738?')).toBe(true);
    expect(ssr).toContain('tab=activity');
    expect(ssr).toContain('round=2');
    expect(ssr).toContain('finding=f-9');
  });
});

describe('レビュー画面の URL', () => {
  it('PR 番号は正の整数だけを読む', () => {
    expect(parsePrNumber('231')).toBe(231);
    for (const raw of ['0', '-1', '1.5', 'abc', '', undefined]) {
      expect(parsePrNumber(raw)).toBeNull();
    }
  });

  it('一覧と詳細の href を組む', () => {
    expect(reviewListHref('acme', 'APP')).toBe('/acme/projects/APP/reviews');
    expect(reviewDetailHref('acme', 'APP', 231)).toBe('/acme/projects/APP/reviews/231');
  });

  it('旧 URL の ?pr= は詳細へ転送し、指摘の絞り込みと注目する指摘を引き継ぐ', () => {
    expect(
      legacyReviewDetailHref(
        '/acme/projects/APP/reviews',
        new URLSearchParams('pr=231&finding=x&state=open'),
      ),
    ).toBe('/acme/projects/APP/reviews/231?state=open&finding=x');
    expect(
      legacyReviewDetailHref('/acme/projects/APP/reviews/', {
        pr: '231',
        round: '2',
        severity: 'high',
      }),
    ).toBe('/acme/projects/APP/reviews/231?severity=high&round=2');
  });

  it('旧 URL の一覧のクエリ（mine・page・q・pr_state）は捨てる', () => {
    expect(
      legacyReviewDetailHref('/acme/projects/APP/reviews', {
        pr: '231',
        mine: '1',
        page: '2',
        q: 'fix',
        pr_state: 'all',
      }),
    ).toBe('/acme/projects/APP/reviews/231');
  });

  it('pr が無い・不正なら転送しない', () => {
    expect(legacyReviewDetailHref('/acme/projects/APP/reviews', { q: 'fix' })).toBeNull();
    expect(legacyReviewDetailHref('/acme/projects/APP/reviews', { pr: 'abc' })).toBeNull();
    expect(legacyReviewDetailHref('/acme/projects/APP/reviews', undefined)).toBeNull();
  });
});
