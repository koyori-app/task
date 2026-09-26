import { describe, it, expect } from 'vitest';
import {
  STATES,
  countsAsUnresolved,
  findingActions,
  findingLocation,
  sortFindings,
  mergeVerdict,
  summaryRows,
  type FindingSeverity,
  type ReviewFinding,
} from '../review-findings';

function finding(overrides: Partial<ReviewFinding> = {}): ReviewFinding {
  return {
    id: 'f-1',
    review_id: 'r-1',
    pr_number: 618,
    round: 1,
    severity: 'high',
    title: '認可漏れ',
    body: '本文',
    file: null,
    line: null,
    state: 'open',
    deferred_task_id: null,
    fixed_by: null,
    created_at: '2026-08-26T00:00:00Z',
    updated_at: '2026-08-26T00:00:00Z',
    transitions: [],
    available_actions: [],
    ...overrides,
  };
}

describe('findingActions', () => {
  // 遷移先を決めるのは backend（available_actions）。ここはラベル付けだけ
  it('available_actions の順にラベルを付けて返す', () => {
    const actions = findingActions(
      finding({ severity: 'low', available_actions: ['fixed', 'deferred', 'rejected'] }),
    );
    expect(actions).toEqual([
      { to: 'fixed', label: '修正した' },
      { to: 'deferred', label: '繰り延べる' },
      { to: 'rejected', label: '指摘を取り下げる' },
    ]);
  });

  it('open への遷移は fixed からなら差し戻し、それ以外は再オープンと呼ぶ', () => {
    expect(findingActions(finding({ state: 'fixed', available_actions: ['open'] }))[0].label).toBe(
      'レビューに戻す',
    );
    expect(
      findingActions(finding({ state: 'rejected', available_actions: ['open'] }))[0].label,
    ).toBe('再オープン');
  });

  it('available_actions が空なら操作を出さない', () => {
    expect(findingActions(finding({ state: 'verified' }))).toEqual([]);
  });
});

describe('マージ判定の材料', () => {
  it('open と fixed を未解決に数える（fixed は確認が済んでいない）', () => {
    expect(STATES.filter(countsAsUnresolved)).toEqual(['open', 'fixed']);
  });
});

describe('mergeVerdict', () => {
  const REVIEWED = '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e';

  const summary = (over: Partial<Parameters<typeof mergeVerdict>[0]>) =>
    mergeVerdict({
      pr_number: 618,
      rounds: 1,
      counts: [],
      blocking: 0,
      latest_head_sha: REVIEWED,
      // 既定は「連携あり・レビューした commit が現在の head」＝可を出してよい状態
      repository: 'acme/app',
      cached_pr_head_sha: REVIEWED,
      pr_head_checked_at: '2026-08-28T10:00:00Z',
      owner_override_rejections: 0,
      mergeable: true,
      gate: 'ready',
      ...over,
    });

  // 判定は backend の gate。ここは gate ごとの見出しと説明だけを見る
  it('連携が無ければ「可」と言わない（集計の視界が空になるため）', () => {
    const verdict = summary({ repository: null, gate: 'unlinked' });
    expect(verdict.kind).toBe('unlinked');
    expect(verdict.title).toBe('リポジトリ未確定');
  });

  it('レビューが 1 件も無い PR は「可」と言わない', () => {
    const verdict = summary({
      rounds: 0,
      latest_head_sha: null,
      mergeable: false,
      gate: 'unreviewed',
    });
    expect(verdict.kind).toBe('unreviewed');
    expect(verdict.title).toBe('未レビュー');
    expect(verdict.detail).toContain('まだレビューされていません');
  });

  it('レビュー後にコミットが積まれていれば「可」と言わない', () => {
    const verdict = summary({
      cached_pr_head_sha: 'ffffffffffffffffffffffffffffffffffffffff',
      gate: 'outdated',
    });
    expect(verdict.kind).toBe('outdated');
    expect(verdict.title).toBe('レビューが古い');
    expect(verdict.detail).toContain('fffffff');
  });

  it('現在の HEAD を確かめられていなければ「可」と言わない', () => {
    const verdict = summary({ cached_pr_head_sha: null, gate: 'stale_unknown' });
    expect(verdict.kind).toBe('stale_unknown');
    expect(verdict.title).toBe('鮮度不明');
  });

  it('可のときはレビューした commit と確認時刻を添える', () => {
    const verdict = summary({});
    expect(verdict.kind).toBe('ready');
    expect(verdict.title).toBe('マージ可');
    expect(verdict.detail).toContain('60cdd77');
    // キャッシュは push で更新されないので、いつ時点の確認かを必ず出す
    expect(verdict.detail).toContain('GitHub 確認');
  });

  it('未解決が残っていれば件数つきで不可', () => {
    const verdict = summary({ blocking: 2, mergeable: false, gate: 'blocked' });
    expect(verdict.kind).toBe('blocked');
    expect(verdict.title).toBe('マージ不可（2 件）');
    expect(verdict.detail).toContain('60cdd77');
  });
});

describe('summaryRows', () => {
  it('件数 0 の組み合わせは出さず、重大度 → 状態の順に並べる', () => {
    const rows = summaryRows({
      pr_number: 618,
      rounds: 2,
      blocking: 2,
      latest_head_sha: null,
      repository: 'acme/app',
      cached_pr_head_sha: null,
      pr_head_checked_at: null,
      owner_override_rejections: 0,
      mergeable: false,
      gate: 'blocked',
      counts: [
        { severity: 'low', state: 'deferred', count: 3 },
        { severity: 'high', state: 'open', count: 2 },
        { severity: 'medium', state: 'verified', count: 0 },
      ],
    });
    expect(rows).toEqual([
      { severity: 'high', state: 'open', count: 2 },
      { severity: 'low', state: 'deferred', count: 3 },
    ]);
  });
});

describe('表示ヘルパー', () => {
  it('位置は file:line、行が無ければ file だけ、file が無ければ null', () => {
    expect(findingLocation(finding({ file: 'src/App.vue', line: 42 }))).toBe('src/App.vue:42');
    expect(findingLocation(finding({ file: 'src/App.vue' }))).toBe('src/App.vue');
    expect(findingLocation(finding())).toBeNull();
  });

  it('重大度が高い順 → 未解決を先に → 新しいラウンドを先に並べる', () => {
    const sorted = sortFindings([
      finding({ id: 'nit', severity: 'nit' as FindingSeverity }),
      finding({ id: 'high-verified', severity: 'high', state: 'verified' }),
      finding({ id: 'high-open-r1', severity: 'high', state: 'open', round: 1 }),
      finding({ id: 'high-open-r2', severity: 'high', state: 'open', round: 2 }),
      finding({ id: 'medium', severity: 'medium' }),
    ]);
    expect(sorted.map((f) => f.id)).toEqual([
      'high-open-r2',
      'high-open-r1',
      'high-verified',
      'medium',
      'nit',
    ]);
  });
});
