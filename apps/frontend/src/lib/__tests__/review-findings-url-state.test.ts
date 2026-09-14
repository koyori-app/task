import { describe, expect, it } from 'vitest';

import {
  DEFAULT_REVIEW_FINDINGS_URL_STATE,
  applyReviewFindingsUrlState,
  parseReviewFindingsUrlState,
  reviewFindingHref,
} from '@/lib/review-findings-url-state';

describe('review findings URL state', () => {
  it('既存の ?pr= だけのリンクをそのまま読める', () => {
    expect(parseReviewFindingsUrlState({ pr: '618' })).toEqual({
      state: { ...DEFAULT_REVIEW_FINDINGS_URL_STATE, pr: 618 },
      warnings: [],
    });
  });

  it('PR・三つの絞り込み・注目する指摘を復元する', () => {
    const parsed = parseReviewFindingsUrlState(
      new URLSearchParams(
        'pr=738&round=2&severity=high&state=open&finding=4f20d810-3851-4cd0-95b1-bf17bdaf715c',
      ),
    );

    expect(parsed).toEqual({
      state: {
        pr: 738,
        round: 2,
        severity: 'high',
        state: 'open',
        finding: '4f20d810-3851-4cd0-95b1-bf17bdaf715c',
      },
      warnings: [],
    });
  });

  it('知らない値を安全な既定値へ倒し、黙って捨てない', () => {
    const parsed = parseReviewFindingsUrlState({
      pr: 'missing',
      round: '-1',
      severity: 'urgent',
      state: 'pending',
      finding: '<script>',
    });

    expect(parsed.state).toEqual(DEFAULT_REVIEW_FINDINGS_URL_STATE);
    expect(parsed.warnings).toHaveLength(5);
    expect(parsed.warnings.join(' ')).toContain('PR 番号');
    expect(parsed.warnings.join(' ')).toContain('指摘 ID');
  });

  it('既定値を URL に載せず、この画面が持たない query は保つ', () => {
    const url = applyReviewFindingsUrlState(
      new URL('https://app.example.com/acme/projects/APP/reviews?tab=activity&round=9'),
      { ...DEFAULT_REVIEW_FINDINGS_URL_STATE, pr: 738 },
    );

    expect(url.searchParams.get('pr')).toBe('738');
    expect(url.searchParams.get('tab')).toBe('activity');
    expect(url.searchParams.has('round')).toBe(false);
    expect(url.searchParams.has('severity')).toBe(false);
    expect(url.searchParams.has('state')).toBe(false);
    expect(url.searchParams.has('finding')).toBe(false);
  });

  it('指摘ごとの共有 URL は現在の選択と絞り込みを保つ', () => {
    const href = reviewFindingHref(
      new URL('https://app.example.com/acme/projects/APP/reviews'),
      {
        pr: 738,
        round: 2,
        severity: 'medium',
        state: 'fixed',
        finding: null,
      },
      'finding-2',
    );

    expect(href).toBe(
      'https://app.example.com/acme/projects/APP/reviews?pr=738&round=2&severity=medium&state=fixed&finding=finding-2',
    );
  });
});
