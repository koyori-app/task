// 受け入れ条件（docs/dev/features/review-findings/index.md から移した。テストの無い条件はページの「未実装と既知の制限」に残す）
// - 画面のゲート表示: 連携の無いプロジェクトでは「マージ可」を出さず、リポジトリ未確定として示す（対照: 連携があってラウンド 1 件・未解決 0 なら「可」）
// - 画面の鮮度表示: キャッシュ済みの現在 head と最新ラウンドの `head_sha` が不一致なら「レビューが古い」、キャッシュが無ければ「鮮度不明」で、どちらも「可」を出さない。一致なら「可」に GitHub の確認時刻が併記される

import { describe, it, expect } from 'vitest';
import {
  editedTitle,
  STATES,
  countsAsUnresolved,
  fileProgress,
  filterFindings,
  filterLabel,
  findingActionButtons,
  groupFindingsByFile,
  mergeGateView,
  sortFindings,
  transitionErrorMessage,
  type FindingFilters,
  type FindingSeverity,
  type FindingState,
  type ReviewFinding,
  type ReviewSummary,
} from '../review-findings';

const VIEWER = '00000000-0000-0000-0000-0000000000aa';
const OTHER = '00000000-0000-0000-0000-0000000000bb';

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

function summary(overrides: Partial<ReviewSummary> = {}): ReviewSummary {
  return {
    pr_number: 618,
    rounds: 1,
    counts: [],
    blocking: 0,
    latest_head_sha: '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e',
    repository: 'acme/app',
    cached_pr_head_sha: '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e',
    pr_head_checked_at: '2026-08-28T10:00:00Z',
    owner_override_rejections: 0,
    mergeable: true,
    gate: 'ready',
    ...overrides,
  };
}

const states = (...list: FindingState[]) => list.map((state) => ({ state }));

describe('マージ判定の材料', () => {
  it('open と fixed を未解決に数える（fixed は確認が済んでいない）', () => {
    expect(STATES.filter(countsAsUnresolved)).toEqual(['open', 'fixing', 'fixed']);
  });
});

describe('sortFindings', () => {
  it('重大度が高い順 → 未解決を先に → 新しいラウンドを先に並べる', () => {
    const sorted = sortFindings([
      finding({ id: 'low-open', severity: 'low', state: 'open' }),
      finding({ id: 'high-verified', severity: 'high', state: 'verified' }),
      finding({ id: 'high-open-r1', severity: 'high', state: 'open', round: 1 }),
      finding({ id: 'high-open-r2', severity: 'high', state: 'open', round: 2 }),
    ]);
    expect(sorted.map((f) => f.id)).toEqual([
      'high-open-r2',
      'high-open-r1',
      'high-verified',
      'low-open',
    ]);
  });

  it('入力を書き換えない', () => {
    const input = [finding({ id: 'b', severity: 'low' }), finding({ id: 'a', severity: 'high' })];
    sortFindings(input);
    expect(input.map((f) => f.id)).toEqual(['b', 'a']);
  });
});

describe('fileProgress', () => {
  it('open が残れば danger', () => {
    expect(fileProgress(states('open', 'verified', 'fixed'))).toEqual({
      resolved: 1,
      total: 3,
      tone: 'danger',
    });
  });

  it('fixing だけでも danger', () => {
    expect(fileProgress(states('fixing', 'verified')).tone).toBe('danger');
  });

  it('残りが fixed だけなら warning で、fixed は解決に数えない（2 / 3）', () => {
    expect(fileProgress(states('verified', 'deferred', 'fixed'))).toEqual({
      resolved: 2,
      total: 3,
      tone: 'warning',
    });
  });

  it('全部が verified・deferred・rejected なら success（2 / 2）', () => {
    expect(fileProgress(states('verified', 'rejected'))).toEqual({
      resolved: 2,
      total: 2,
      tone: 'success',
    });
    expect(fileProgress(states('deferred')).tone).toBe('success');
  });

  it('空は 0 / 0 で success', () => {
    expect(fileProgress([])).toEqual({ resolved: 0, total: 0, tone: 'success' });
  });

  it('知らない状態は未解決として danger に倒す', () => {
    expect(fileProgress([{ state: 'archived' as FindingState }, { state: 'verified' }])).toEqual({
      resolved: 1,
      total: 2,
      tone: 'danger',
    });
  });
});

describe('groupFindingsByFile', () => {
  const sorted = [
    finding({ id: 'a1', file: 'src/a.ts', state: 'open' }),
    finding({ id: 'n1', file: null, state: 'open' }),
    finding({ id: 'b1', file: 'src/b.ts', state: 'verified' }),
    finding({ id: 'a2', file: 'src/a.ts', state: 'verified' }),
  ];

  it('最初に出たファイルから並べ、ファイルの指定なしは最後に置く', () => {
    const groups = groupFindingsByFile(sorted, new Set(sorted.map((f) => f.id)));
    expect(groups.map((g) => g.file)).toEqual(['src/a.ts', 'src/b.ts', null]);
    expect(groups[0].findings.map((f) => f.id)).toEqual(['a1', 'a2']);
  });

  it('見えていない指摘は findings に入らないが、progress には数える', () => {
    const groups = groupFindingsByFile(sorted, new Set(['a2', 'b1']));
    expect(groups[0].findings.map((f) => f.id)).toEqual(['a2']);
    expect(groups[0].progress).toEqual({ resolved: 1, total: 2, tone: 'danger' });
  });

  it('見える指摘が 0 件のまとまりは出さない', () => {
    const groups = groupFindingsByFile(sorted, new Set(['b1']));
    expect(groups.map((g) => g.file)).toEqual(['src/b.ts']);
  });
});

describe('filterFindings', () => {
  const list = [
    finding({ id: 'a', severity: 'high', state: 'open', round: 2, title: 'URL の読み取り' }),
    finding({
      id: 'b',
      severity: 'low',
      state: 'fixed',
      round: 1,
      title: '並び順',
      body: 'Query の順序',
    }),
    finding({
      id: 'c',
      severity: 'nit',
      state: 'verified',
      round: 1,
      title: '誤字',
      file: 'src/Lib/Url.ts',
    }),
  ];
  const reviewerOfRound = new Map([
    [1, 'u-1'],
    [2, 'u-2'],
  ]);
  const none: FindingFilters = { q: '', states: [], severities: [], round: null, reviewer: null };
  const ids = (filters: Partial<FindingFilters>) =>
    filterFindings(list, { ...none, ...filters }, reviewerOfRound).map((f) => f.id);

  it('空の集合は絞らない', () => {
    expect(ids({})).toEqual(['a', 'b', 'c']);
  });

  it('状態と重大度は集合で絞る', () => {
    expect(ids({ states: ['open', 'fixed'] })).toEqual(['a', 'b']);
    expect(ids({ severities: ['low', 'nit'] })).toEqual(['b', 'c']);
  });

  it('Round とレビュアーで絞る', () => {
    expect(ids({ round: 1 })).toEqual(['b', 'c']);
    expect(ids({ reviewer: 'u-2' })).toEqual(['a']);
    expect(ids({ reviewer: 'u-9' })).toEqual([]);
  });

  it('検索は題・本文・ファイルの部分一致で、大文字小文字を無視する', () => {
    expect(ids({ q: 'url' })).toEqual(['a', 'c']);
    expect(ids({ q: ' query ' })).toEqual(['b']);
    expect(ids({ q: 'LIB/' })).toEqual(['c']);
  });

  it('条件は AND で重なる', () => {
    expect(ids({ q: 'url', states: ['verified'], round: 1, reviewer: 'u-1' })).toEqual(['c']);
    expect(ids({ q: 'url', severities: ['low'] })).toEqual([]);
  });
});

describe('filterLabel', () => {
  it('選択なしは「すべて」、1 つはそのラベル', () => {
    expect(filterLabel('state', [])).toBe('状態: すべて');
    expect(filterLabel('state', ['open'])).toBe('状態: Open');
    expect(filterLabel('severity', [])).toBe('重大度: すべて');
    expect(filterLabel('severity', ['nit'])).toBe('重大度: Nit');
  });

  it('open・fixing・fixed ちょうどは「未解決」', () => {
    expect(filterLabel('state', ['fixed', 'open', 'fixing'])).toBe('状態: 未解決');
  });

  it('ほかは件数', () => {
    expect(filterLabel('state', ['open', 'fixed'])).toBe('状態: 2 件');
    expect(filterLabel('state', ['open', 'fixing', 'fixed', 'verified'])).toBe('状態: 4 件');
    expect(filterLabel('severity', ['high', 'medium'])).toBe('重大度: 2 件');
  });
});

describe('findingActionButtons', () => {
  const labels = (list: { label: string }[]) => list.map((b) => b.label);

  it('open は左に着手・修正した、右に繰り延べ・取り下げ', () => {
    const { left, right } = findingActionButtons(
      finding({
        severity: 'low',
        available_actions: ['rejected', 'deferred', 'fixed', 'fixing'],
      }),
      VIEWER,
    );
    expect(labels(left)).toEqual(['修正に着手する', '修正した']);
    expect(labels(right)).toEqual(['繰り延べる…', '取り下げる…']);
    // 左の先頭だけ secondary、右は ghost
    expect(left.map((b) => b.variant)).toEqual(['secondary', 'tertiary']);
    expect(right.map((b) => b.variant)).toEqual(['ghost', 'ghost']);
    // 取り下げだけ赤
    expect([...left, ...right].filter((b) => b.danger).map((b) => b.to)).toEqual(['rejected']);
    // 入力欄を開くのは繰り延べと取り下げだけ
    expect([...left, ...right].map((b) => b.form)).toEqual([null, null, 'defer', 'reject']);
    expect([...left, ...right].every((b) => b.disabledReason === null)).toBe(true);
  });

  it('available_actions に無いものは出さない（High の open に繰り延べが無い）', () => {
    const { right } = findingActionButtons(
      finding({ severity: 'high', available_actions: ['fixing', 'fixed', 'rejected'] }),
      VIEWER,
    );
    expect(labels(right)).toEqual(['取り下げる…']);
  });

  it('fixing は修正した・着手を取り消す', () => {
    const { left, right } = findingActionButtons(
      finding({ state: 'fixing', available_actions: ['open', 'fixed'] }),
      VIEWER,
    );
    expect(labels(left)).toEqual(['修正した', '着手を取り消す']);
    expect(right).toEqual([]);
  });

  it('fixed は左に確認した、右にレビューに戻す', () => {
    const { left, right } = findingActionButtons(
      finding({ state: 'fixed', fixed_by: OTHER, available_actions: ['verified', 'open'] }),
      VIEWER,
    );
    expect(labels(left)).toEqual(['確認した']);
    expect(left[0].disabledReason).toBeNull();
    expect(labels(right)).toEqual(['レビューに戻す']);
  });

  it('deferred と rejected は再オープン、verified は空', () => {
    for (const state of ['deferred', 'rejected'] as const) {
      const { left, right } = findingActionButtons(
        finding({ state, available_actions: ['open'] }),
        VIEWER,
      );
      expect(labels(left)).toEqual(['再オープン']);
      expect(right).toEqual([]);
    }
    expect(findingActionButtons(finding({ state: 'verified' }), VIEWER)).toEqual({
      left: [],
      right: [],
    });
  });

  it('修正した本人の fixed には、無効の「確認した」と理由を出す', () => {
    const { left, right } = findingActionButtons(
      finding({ state: 'fixed', fixed_by: VIEWER, available_actions: ['open'] }),
      VIEWER,
    );
    expect(left).toHaveLength(1);
    expect(left[0]).toMatchObject({
      to: 'verified',
      label: '確認した',
      disabledReason: '修正した人は確認できません',
    });
    expect(labels(right)).toEqual(['レビューに戻す']);
  });

  it('本人でない fixed で verified が無ければ、何も足さない', () => {
    const { left } = findingActionButtons(
      finding({ state: 'fixed', fixed_by: OTHER, available_actions: ['open'] }),
      VIEWER,
    );
    expect(left).toEqual([]);
  });
});

describe('mergeGateView', () => {
  const count = (severity: FindingSeverity, state: FindingState, n: number) => ({
    severity,
    state,
    count: n,
  });

  it('ready は条件が無く、確認時刻を持つ', () => {
    const view = mergeGateView(summary());
    expect(view).toMatchObject({ kind: 'ready', tone: 'success', title: 'マージ可' });
    expect(view.condition).toBeNull();
    expect(view.checkedAt).toMatch(/^GitHub 確認: \d{1,2}\/\d{1,2} \d{1,2}:\d{2} 時点$/);
  });

  it('ready で確認時刻が無ければ null', () => {
    expect(mergeGateView(summary({ pr_head_checked_at: null })).checkedAt).toBeNull();
  });

  it('blocked は件数と承認待ちの内数を出し、未解決を見るボタンを持つ', () => {
    const view = mergeGateView(
      summary({
        gate: 'blocked',
        blocking: 4,
        counts: [
          count('high', 'open', 2),
          count('high', 'fixed', 1),
          count('medium', 'fixing', 1),
          // Low の fixed は承認待ちの内数に入れない
          count('low', 'fixed', 3),
        ],
      }),
    );
    expect(view).toMatchObject({ kind: 'blocked', tone: 'danger', title: 'マージ不可' });
    expect(view.condition).toEqual({
      text: 'High・Medium の未解決が 4 件（うち承認待ち 1）',
      sha: null,
      action: 'show-unresolved',
    });
    expect(view.checkedAt).toBeNull();
  });

  it('blocked で fixed が 0 なら「うち承認待ち」を括弧ごと出さない', () => {
    const view = mergeGateView(
      summary({ gate: 'blocked', blocking: 2, counts: [count('high', 'open', 2)] }),
    );
    expect(view.condition?.text).toBe('High・Medium の未解決が 2 件');
  });

  it('unreviewed は起票のボタンを持つ', () => {
    expect(mergeGateView(summary({ gate: 'unreviewed', rounds: 0 }))).toMatchObject({
      tone: 'warning',
      title: '未レビュー',
      condition: { text: 'まだレビューがありません', sha: null, action: 'compose' },
    });
  });

  it('outdated は今の head の先頭 7 桁を文の中に置く', () => {
    const view = mergeGateView(
      summary({ gate: 'outdated', cached_pr_head_sha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678' }),
    );
    expect(view).toMatchObject({ tone: 'warning', title: 'レビューが古い' });
    expect(view.condition).toEqual({
      text: 'レビューのあとにコミットが積まれています（今は {sha}）',
      sha: 'a1b2c3d',
      action: 'compose',
    });
  });

  it('stale_unknown と unlinked はボタンを持たない', () => {
    expect(
      mergeGateView(summary({ gate: 'stale_unknown', cached_pr_head_sha: null })),
    ).toMatchObject({
      tone: 'warning',
      title: '鮮度不明',
      condition: { text: '今の head を GitHub で確かめられていません', action: null },
    });
    expect(mergeGateView(summary({ gate: 'unlinked', repository: null }))).toMatchObject({
      tone: 'warning',
      title: 'リポジトリ未確定',
      condition: {
        text: 'GitHub と連携していないため、どの PR を見た集計か決まりません',
        action: null,
      },
    });
  });
});

describe('transitionErrorMessage', () => {
  it('理由つきの本文はそのまま出す', () => {
    expect(transitionErrorMessage(409, 'High / Medium の指摘は繰り延べできません')).toBe(
      'High / Medium の指摘は繰り延べできません',
    );
  });

  it('スラグだけの本文は、状態コードごとの文に倒す', () => {
    expect(transitionErrorMessage(403, 'forbidden')).toBe(
      'この操作はレビュー側だけが行えます（自分の修正は自分で確認できません）。',
    );
    expect(transitionErrorMessage(409, 'conflict')).toBe(
      'いまの状態からは行えない操作です。画面を再読み込みしてください。',
    );
    expect(transitionErrorMessage(500, undefined)).toBe('状態を更新できませんでした。');
    expect(transitionErrorMessage(undefined, undefined)).toBe('状態を更新できませんでした。');
  });
});

describe('editedTitle', () => {
  it('直した指摘だけに、直した時刻の title を返す', () => {
    expect(editedTitle(finding())).toBeNull();
    expect(editedTitle(finding({ edited_at: null }))).toBeNull();
    const title = editedTitle(finding({ edited_at: '2026-11-22T03:04:05Z' }));
    expect(title).toBe(`${new Date('2026-11-22T03:04:05Z').toLocaleString('ja-JP')} に編集`);
  });
});
