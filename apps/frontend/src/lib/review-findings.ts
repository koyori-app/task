import type { components } from '@/generated/api';

export type FindingSeverity = components['schemas']['FindingSeverity'];
export type FindingState = components['schemas']['FindingState'];
export type ReviewFinding = components['schemas']['FindingResponse'];
export type Review = components['schemas']['ReviewResponse'];
export type ReviewedPullRequest = components['schemas']['ReviewedPullRequest'];
export type ReviewSummary = components['schemas']['ReviewSummaryResponse'];
type ReviewGate = components['schemas']['ReviewGate'];

export const SEVERITIES: FindingSeverity[] = ['high', 'medium', 'low', 'nit'];
export const STATES: FindingState[] = [
  'open',
  'fixing',
  'fixed',
  'verified',
  'deferred',
  'rejected',
];

export const SEVERITY_LABELS: Record<FindingSeverity, string> = {
  high: 'High',
  medium: 'Medium',
  low: 'Low',
  nit: 'Nit',
};

export const STATE_LABELS: Record<FindingState, string> = {
  open: 'Open',
  fixing: 'Fixing',
  fixed: 'Fixed',
  verified: 'Verified',
  deferred: 'Deferred',
  rejected: 'Rejected',
};

/** マージ判定で「未解決」と数える状態（backend の `counts_as_unresolved` と対）。一覧の並びに使う。 */
export function countsAsUnresolved(state: FindingState): boolean {
  return state === 'open' || state === 'fixing' || state === 'fixed';
}

/** 一覧の並び: 重大度が高い順 → 未解決を先に → 新しいラウンドを先に。 */
export function sortFindings(findings: readonly ReviewFinding[]): ReviewFinding[] {
  const severityRank = (severity: FindingSeverity) => SEVERITIES.indexOf(severity);
  return [...findings].sort(
    (a, b) =>
      severityRank(a.severity) - severityRank(b.severity) ||
      Number(countsAsUnresolved(b.state)) - Number(countsAsUnresolved(a.state)) ||
      b.round - a.round,
  );
}

// --- ファイルごとのまとまりと件数 ---

type ProgressTone = 'danger' | 'warning' | 'success';
export type FileProgress = { resolved: number; total: number; tone: ProgressTone };

const RESOLVED_STATES: readonly string[] = ['verified', 'deferred', 'rejected'];

/**
 * ファイルの件数（解決 / 全体）と色。解決は verified・deferred・rejected で、fixed は
 * 確認が済んでいないので数えない。open・fixing が残れば danger、残りが fixed だけなら
 * warning、全部解決なら success。知らない状態は未解決として danger に倒す。
 */
export function fileProgress(findings: readonly Pick<ReviewFinding, 'state'>[]): FileProgress {
  let resolved = 0;
  let awaiting = 0;
  for (const finding of findings) {
    if (RESOLVED_STATES.includes(finding.state)) resolved += 1;
    else if (finding.state === 'fixed') awaiting += 1;
  }
  const total = findings.length;
  const tone = resolved + awaiting < total ? 'danger' : awaiting > 0 ? 'warning' : 'success';
  return { resolved, total, tone };
}

export type FileGroup = {
  /** ファイルの指定が無いまとまりは null */
  file: string | null;
  /** 絞り込み後の、出す指摘 */
  findings: ReviewFinding[];
  /** そのファイルの全指摘で数えた件数 */
  progress: FileProgress;
};

/**
 * 指摘をファイルごとにまとめる。並びは `sorted` で最初に出たファイルから、ファイルの
 * 指定が無いまとまりは最後。件数は絞り込みに関係なくそのファイルの全指摘で数え、
 * 見える指摘が 0 件のまとまりは出さない。
 */
export function groupFindingsByFile(
  sorted: readonly ReviewFinding[],
  visibleIds: ReadonlySet<string>,
): FileGroup[] {
  const all = new Map<string | null, ReviewFinding[]>();
  for (const finding of sorted) {
    const file = finding.file || null;
    const list = all.get(file);
    if (list) list.push(finding);
    else all.set(file, [finding]);
  }
  const groups: FileGroup[] = [];
  for (const [file, list] of all) {
    const findings = list.filter((finding) => visibleIds.has(finding.id));
    if (findings.length > 0) groups.push({ file, findings, progress: fileProgress(list) });
  }
  return [...groups.filter((g) => g.file !== null), ...groups.filter((g) => g.file === null)];
}

// --- 絞り込み ---

export type FindingFilters = {
  q: string;
  states: readonly FindingState[];
  severities: readonly FindingSeverity[];
  round: number | null;
  reviewer: string | null;
};

/**
 * 指摘の絞り込み。状態と重大度は集合（空は絞らない）、検索は題・本文・ファイルの
 * 部分一致で大文字小文字を無視する。レビュアーはその Round のラウンドの
 * レビュアー（`reviewerOfRound`）で見る。
 */
export function filterFindings(
  findings: readonly ReviewFinding[],
  filters: FindingFilters,
  reviewerOfRound: ReadonlyMap<number, string>,
): ReviewFinding[] {
  const q = filters.q.trim().toLowerCase();
  return findings.filter(
    (finding) =>
      (filters.states.length === 0 || filters.states.includes(finding.state)) &&
      (filters.severities.length === 0 || filters.severities.includes(finding.severity)) &&
      (filters.round === null || finding.round === filters.round) &&
      (filters.reviewer === null || reviewerOfRound.get(finding.round) === filters.reviewer) &&
      (q === '' ||
        finding.title.toLowerCase().includes(q) ||
        finding.body.toLowerCase().includes(q) ||
        (finding.file ?? '').toLowerCase().includes(q)),
  );
}

/** 未解決に数える状態（「未解決を見る」が選ぶ集合）。 */
export const UNRESOLVED_STATES: FindingState[] = STATES.filter(countsAsUnresolved);

/** 絞り込みの Dropdown のラベル（「状態: Open」「状態: 未解決」「重大度: 2 件」）。 */
export function filterLabel(kind: 'state' | 'severity', selected: readonly string[]): string {
  const name = kind === 'state' ? '状態' : '重大度';
  const labels: Record<string, string> = kind === 'state' ? STATE_LABELS : SEVERITY_LABELS;
  if (selected.length === 0) return `${name}: すべて`;
  if (selected.length === 1) return `${name}: ${labels[selected[0]] ?? selected[0]}`;
  if (
    kind === 'state' &&
    selected.length === UNRESOLVED_STATES.length &&
    UNRESOLVED_STATES.every((state) => selected.includes(state))
  ) {
    return `${name}: 未解決`;
  }
  return `${name}: ${selected.length} 件`;
}

// --- 操作 ---

export type FindingActionButton = {
  to: FindingState;
  label: string;
  variant: 'secondary' | 'tertiary' | 'ghost';
  icon: 'start' | 'check' | 'undo' | 'defer' | 'reject';
  danger: boolean;
  /** 押すと理由の入力欄を開く（deferred / rejected） */
  form: 'defer' | 'reject' | null;
  /** 無効で出すときの理由。押せるボタンは null */
  disabledReason: string | null;
};

type ActionSpec = Pick<FindingActionButton, 'to' | 'label' | 'icon'> & {
  side: 'left' | 'right';
  form?: 'defer' | 'reject';
};

/**
 * 指摘に出す操作。遷移先は backend が要求者ごとに返す `available_actions` だけを使う。
 *
 * 遷移規則と役割規則（誰が確認・差し戻し・取り下げをできるか、High / Medium は
 * 繰り延べられない、自分の修正は自分で確認できない）は backend だけが持つ。
 * 画面に写すと片方だけ直したときに「押せるのに失敗する」ボタンができる。
 * ここが決めるのはラベル・並び・左右だけ。
 */
export function findingActionButtons(
  finding: ReviewFinding,
  viewerId: string,
): { left: FindingActionButton[]; right: FindingActionButton[] } {
  const reopen: ActionSpec =
    finding.state === 'fixed'
      ? { to: 'open', label: 'レビューに戻す', icon: 'undo', side: 'right' }
      : {
          to: 'open',
          label: finding.state === 'fixing' ? '着手を取り消す' : '再オープン',
          icon: 'undo',
          side: 'left',
        };
  const specs: ActionSpec[] = [
    { to: 'fixing', label: '修正に着手する', icon: 'start', side: 'left' },
    { to: 'fixed', label: '修正した', icon: 'check', side: 'left' },
    { to: 'verified', label: '確認した', icon: 'check', side: 'left' },
    reopen,
    { to: 'deferred', label: '繰り延べる…', icon: 'defer', side: 'right', form: 'defer' },
    { to: 'rejected', label: '取り下げる…', icon: 'reject', side: 'right', form: 'reject' },
  ];
  // 確認が出ない理由を言えるのは「修正した本人」のときだけ。ほかの理由では何も足さない
  const selfFixed =
    finding.state === 'fixed' &&
    finding.fixed_by === viewerId &&
    !finding.available_actions.includes('verified');

  const left: FindingActionButton[] = [];
  const right: FindingActionButton[] = [];
  for (const spec of specs) {
    const available = finding.available_actions.includes(spec.to);
    const disabledReason =
      !available && spec.to === 'verified' && selfFixed ? '修正した人は確認できません' : null;
    if (!available && disabledReason === null) continue;
    const side = spec.side === 'left' ? left : right;
    side.push({
      to: spec.to,
      label: spec.label,
      icon: spec.icon,
      variant: spec.side === 'right' ? 'ghost' : left.length === 0 ? 'secondary' : 'tertiary',
      danger: spec.to === 'rejected',
      form: spec.form ?? null,
      disabledReason,
    });
  }
  return { left, right };
}

/**
 * サーバーが本文に入れた理由。読ませる意味のあるものだけ返す。
 *
 * 共通のエラー本文は `conflict` / `forbidden` のようなスラグで、そのまま出しても
 * 利用者は何も判断できない。理由付きの 409（High の繰り延べなど）だけを拾う。
 */
function serverReason(message: string | undefined): string | null {
  if (!message || /^[a-z][a-z-]*$/.test(message)) return null;
  return message;
}

/** 状態の遷移に失敗したときに出す文。 */
export function transitionErrorMessage(
  status: number | undefined,
  message: string | undefined,
): string {
  return (
    serverReason(message) ??
    (status === 403
      ? 'この操作はレビュー側だけが行えます（自分の修正は自分で確認できません）。'
      : status === 409
        ? 'いまの状態からは行えない操作です。画面を再読み込みしてください。'
        : '状態を更新できませんでした。')
  );
}

// --- マージ判定 ---

export type MergeGateView = {
  kind: ReviewGate;
  tone: 'success' | 'danger' | 'warning';
  title: string;
  /** 満たしていない条件。ready は null */
  condition: {
    text: string;
    /** 文の中でリンクにするコミット（outdated だけ）。`text` は `{sha}` を置き場所に持つ */
    sha: string | null;
    action: 'show-unresolved' | 'compose' | null;
  } | null;
  /** ready のときの「GitHub 確認: … 時点」。ほかは null */
  checkedAt: string | null;
};

/**
 * マージ判定の見出しと、満たしていない条件。判定は backend の `gate` をそのまま使い、
 * 再計算しない。満たしている条件と、条件の説明文は出さない。
 */
export function mergeGateView(summary: ReviewSummary): MergeGateView {
  const kind = summary.gate;
  const warning = (
    title: string,
    text: string,
    action: 'compose' | null,
    sha: string | null = null,
  ): MergeGateView => ({
    kind,
    tone: 'warning',
    title,
    condition: { text, sha, action },
    checkedAt: null,
  });
  switch (kind) {
    case 'ready':
      return {
        kind,
        tone: 'success',
        title: 'マージ可',
        condition: null,
        // キャッシュは push では更新されないので、いつ時点の確認かを添える
        checkedAt: summary.pr_head_checked_at
          ? `GitHub 確認: ${new Date(summary.pr_head_checked_at).toLocaleString('ja-JP', {
              month: 'numeric',
              day: 'numeric',
              hour: 'numeric',
              minute: '2-digit',
            })} 時点`
          : null,
      };
    case 'blocked': {
      const awaiting = summary.counts
        .filter(
          (entry) =>
            (entry.severity === 'high' || entry.severity === 'medium') && entry.state === 'fixed',
        )
        .reduce((total, entry) => total + entry.count, 0);
      return {
        kind,
        tone: 'danger',
        title: 'マージ不可',
        condition: {
          text: `High・Medium の未解決が ${summary.blocking} 件${
            awaiting > 0 ? `（うち承認待ち ${awaiting}）` : ''
          }`,
          sha: null,
          action: 'show-unresolved',
        },
        checkedAt: null,
      };
    }
    case 'unreviewed':
      return warning('未レビュー', 'まだレビューがありません', 'compose');
    case 'outdated': {
      const sha = summary.cached_pr_head_sha ? summary.cached_pr_head_sha.slice(0, 7) : null;
      return warning(
        'レビューが古い',
        sha
          ? 'レビューのあとにコミットが積まれています（今は {sha}）'
          : 'レビューのあとにコミットが積まれています',
        'compose',
        sha,
      );
    }
    case 'stale_unknown':
      return warning('鮮度不明', '今の head を GitHub で確かめられていません', null);
    case 'unlinked':
      return warning(
        'リポジトリ未確定',
        'GitHub と連携していないため、どの PR を見た集計か決まりません',
        null,
      );
  }
}
