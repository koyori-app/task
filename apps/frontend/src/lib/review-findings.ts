import type { components } from '@/generated/api';

export type FindingSeverity = components['schemas']['FindingSeverity'];
export type FindingState = components['schemas']['FindingState'];
export type ReviewFinding = components['schemas']['FindingResponse'];
export type Review = components['schemas']['ReviewResponse'];
export type ReviewedPullRequest = components['schemas']['ReviewedPullRequest'];
export type ReviewSummary = components['schemas']['ReviewSummaryResponse'];
export type ReviewGate = components['schemas']['ReviewGate'];

export const SEVERITIES: FindingSeverity[] = ['high', 'medium', 'low', 'nit'];
export const STATES: FindingState[] = ['open', 'fixed', 'verified', 'deferred', 'rejected'];

export const SEVERITY_LABELS: Record<FindingSeverity, string> = {
  high: 'High',
  medium: 'Medium',
  low: 'Low',
  nit: 'Nit',
};

export const STATE_LABELS: Record<FindingState, string> = {
  open: 'Open',
  fixed: 'Fixed',
  verified: 'Verified',
  deferred: 'Deferred',
  rejected: 'Rejected',
};

/** マージ判定で「未解決」と数える状態（backend の `counts_as_unresolved` と対）。一覧の並びに使う。 */
export function countsAsUnresolved(state: FindingState): boolean {
  return state === 'open' || state === 'fixed';
}

export type FindingAction = {
  to: FindingState;
  label: string;
};

/**
 * 指摘に出す操作。遷移先は backend が要求者ごとに返す `available_actions` だけを使う。
 *
 * 遷移規則と役割規則（誰が確認・差し戻し・取り下げをできるか、High / Medium は
 * 繰り延べられない、自分の修正は自分で確認できない）は backend だけが持つ。
 * 画面に写すと片方だけ直したときに「押せるのに失敗する」ボタンができる。
 */
export function findingActions(finding: ReviewFinding): FindingAction[] {
  const labels: Partial<Record<FindingState, string>> = {
    fixed: '修正した',
    verified: '確認した',
    deferred: '繰り延べる',
    rejected: '指摘を取り下げる',
    open: finding.state === 'fixed' ? 'レビューに戻す' : '再オープン',
  };
  return finding.available_actions.map((to) => ({ to, label: labels[to] ?? STATE_LABELS[to] }));
}

/**
 * マージ可否の見出しと説明。判定は backend の `gate` をそのまま使い、再計算しない。
 *
 * 「レビューが 1 件も無い」と「レビュー済みで指摘なし」は別物なので出し分ける。
 * レビューした commit は出しておく——手元の HEAD と見比べれば、レビュー後に
 * 積まれたコミットに気づける。
 */
export function mergeVerdict(summary: ReviewSummary): {
  kind: ReviewGate;
  title: string;
  detail: string;
} {
  const reviewed = summary.latest_head_sha
    ? `最新ラウンドは ${summary.latest_head_sha.slice(0, 7)} を見ています`
    : '';
  const join = (...parts: string[]) => parts.filter(Boolean).join(' · ');
  const kind = summary.gate;
  switch (kind) {
    case 'unlinked':
      return {
        kind,
        title: 'リポジトリ未確定',
        detail:
          'GitHub 連携が無いため、どのリポジトリの PR を見た集計か決まりません。マージの判断には使えません',
      };
    case 'unreviewed':
      return {
        kind,
        title: '未レビュー',
        detail: 'まだレビューされていません。レビューを 1 ラウンド出してください',
      };
    case 'blocked':
      return {
        kind,
        title: `マージ不可（${summary.blocking} 件）`,
        detail: join('High / Medium が未解決です。Low / Nit は繰り延べできます', reviewed),
      };
    case 'stale_unknown':
      return {
        kind,
        title: '鮮度不明',
        detail: join(
          'High / Medium の未解決はありませんが、現在の HEAD を確認できていません',
          reviewed,
        ),
      };
    case 'outdated':
      return {
        kind,
        title: 'レビューが古い',
        detail: join(
          `レビュー後にコミットが積まれています（現在 ${(summary.cached_pr_head_sha ?? '').slice(0, 7)}）`,
          reviewed,
        ),
      };
    case 'ready':
      return {
        kind,
        title: 'マージ可',
        // キャッシュは push では更新されないので、いつ時点の確認かを必ず添える
        detail: join(
          'High / Medium の未解決はありません',
          reviewed,
          summary.pr_head_checked_at
            ? `GitHub 確認: ${new Date(summary.pr_head_checked_at).toLocaleString('ja-JP')} 時点`
            : '',
        ),
      };
  }
}

/** 指摘の位置（`file:line`）。位置情報が無ければ null。 */
export function findingLocation(finding: ReviewFinding): string | null {
  if (!finding.file) return null;
  return finding.line ? `${finding.file}:${finding.line}` : finding.file;
}

/** 一覧の並び: 重大度が高い順 → 未解決を先に → 新しいラウンドを先に。 */
export function sortFindings(findings: ReviewFinding[]): ReviewFinding[] {
  const severityRank = (severity: FindingSeverity) => SEVERITIES.indexOf(severity);
  return [...findings].sort(
    (a, b) =>
      severityRank(a.severity) - severityRank(b.severity) ||
      Number(countsAsUnresolved(b.state)) - Number(countsAsUnresolved(a.state)) ||
      b.round - a.round,
  );
}

/** 集計から「重大度 → 状態 → 件数」の表示用の行を作る（件数 0 は出さない）。 */
export function summaryRows(
  summary: ReviewSummary,
): { severity: FindingSeverity; state: FindingState; count: number }[] {
  const rows: { severity: FindingSeverity; state: FindingState; count: number }[] = [];
  for (const severity of SEVERITIES) {
    for (const state of STATES) {
      const count = summary.counts
        .filter((entry) => entry.severity === severity && entry.state === state)
        .reduce((total, entry) => total + entry.count, 0);
      if (count > 0) rows.push({ severity, state, count });
    }
  }
  return rows;
}
