import type { components } from '@/generated/api';

type Finding = components['schemas']['FindingResponse'];
type FindingState = components['schemas']['FindingState'];

/**
 * 遷移のあとに backend が返す指摘の代役。単体テストとストーリーの fetch のモックが共有する。
 *
 * 遷移先の表は backend の `can_transition`（`service/src/reviews.rs`）と同じ形にしてある。
 * モックが遷移後も元の `available_actions` を返すと、遷移後のボタンを確かめるテストが
 * 実際には出ないボタンに合わせた期待を書くことになる。役割の規則のうち写しているのは
 * 「自分の修正は自分で確認できない」と「High / Medium は繰り延べられない」だけで、
 * 閲覧者はレビュー側として扱う。
 */
export function transitionedFinding(
  finding: Finding,
  to: FindingState,
  viewer: Finding['transitions'][number]['actor'],
  note: string | null,
  at: string,
): Finding {
  // 差し戻しても、修正した人は残る（差し戻した本人が verified に進めるのを防ぐ）
  const fixedBy = to === 'fixed' ? viewer.id : finding.fixed_by;
  const blocksMerge = finding.severity === 'high' || finding.severity === 'medium';
  const next: Record<FindingState, FindingState[]> = {
    open: ['fixing', 'fixed', ...(blocksMerge ? [] : (['deferred'] as const)), 'rejected'],
    fixing: ['open', 'fixed'],
    fixed: [...(fixedBy === viewer.id ? [] : (['verified'] as const)), 'open'],
    deferred: ['open'],
    rejected: ['open'],
    verified: [],
  };
  return {
    ...finding,
    state: to,
    fixed_by: fixedBy,
    available_actions: next[to],
    updated_at: at,
    transitions: [
      ...finding.transitions,
      {
        id: `t-${finding.id}-${finding.transitions.length}`,
        actor: viewer,
        from_state: finding.state,
        to_state: to,
        note,
        created_at: at,
      },
    ],
  };
}
