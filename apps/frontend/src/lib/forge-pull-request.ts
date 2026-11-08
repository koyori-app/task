import {
  CircleDashed,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  type LucideIcon,
} from '@lucide/vue';

export interface PullRequestStateDisplay {
  icon: LucideIcon;
  label: string;
  class: string;
}

/** ホスト上の PR の状態（`forge_pull_requests.state`）の表示。タスク詳細とレビュー画面で共有する。 */
export const PR_STATES = {
  open: { icon: GitPullRequest, label: 'Open', class: 'text-green-700 dark:text-green-400' },
  merged: { icon: GitMerge, label: 'Merged', class: 'text-violet-700 dark:text-violet-400' },
  closed: { icon: GitPullRequestClosed, label: 'Closed', class: 'text-destructive' },
} as const satisfies Record<string, PullRequestStateDisplay>;

// webhook をまだ受けていない PR（CLI のレビューだけで作られた行など）
const UNKNOWN_STATE: PullRequestStateDisplay = {
  icon: CircleDashed,
  label: '状態不明',
  class: 'text-muted-foreground',
};

export function pullRequestState(state: string): PullRequestStateDisplay {
  return PR_STATES[state as keyof typeof PR_STATES] ?? UNKNOWN_STATE;
}
