import { PhGithubLogo, PhGitlabLogo } from '@phosphor-icons/vue';
import type { Component } from 'vue';

import { httpUrl } from '@/lib/task-activity';

type ForgeHostKind = 'github' | 'gitlab';
type ForgeHost = { kind: ForgeHostKind; label: string; icon: Component; baseUrl: string };

/** リンクを組む材料。PR の一覧の行（`ReviewedPullRequest`）がそのまま当てはまる。 */
export type ForgeTarget = {
  /** PR 行のホスト（`github` など） */
  host: string | null;
  /** ホスト上の PR の URL。webhook を受けるまでは無い */
  html_url: string | null;
  /** `owner/name` */
  repository: string | null;
};

const HOSTS: Record<ForgeHostKind, ForgeHost> = {
  github: { kind: 'github', label: 'GitHub', icon: PhGithubLogo, baseUrl: 'https://github.com' },
  gitlab: { kind: 'gitlab', label: 'GitLab', icon: PhGitlabLogo, baseUrl: 'https://gitlab.com' },
};

/** ホストの表示（名前とロゴ）。知らない値と null は null。 */
export function forgeHost(host: string | null | undefined): ForgeHost | null {
  return host && Object.hasOwn(HOSTS, host) ? HOSTS[host as ForgeHostKind] : null;
}

/** リポジトリのページの URL（`{base}/{owner}/{name}`）。組めなければ null。 */
function repositoryUrl(target: ForgeTarget): { kind: ForgeHostKind; url: string } | null {
  const host = forgeHost(target.host);
  if (!host || !target.repository || !/^[\w.-]+(\/[\w.-]+)+$/.test(target.repository)) return null;
  // セルフホストのホストは、PR の URL の origin から分かる
  const known = target.html_url ? httpUrl(target.html_url) : null;
  const base = known && URL.canParse(known) ? new URL(known).origin : host.baseUrl;
  return { kind: host.kind, url: `${base}/${target.repository}` };
}

/** ホスト上の PR の URL。`html_url` が http(s) ならそれ、無ければホストとリポジトリから組む。 */
export function pullRequestHref(target: ForgeTarget, prNumber: number): string | null {
  const known = target.html_url ? httpUrl(target.html_url) : null;
  if (known) return known;
  const repo = repositoryUrl(target);
  if (!repo) return null;
  return repo.kind === 'gitlab'
    ? `${repo.url}/-/merge_requests/${prNumber}`
    : `${repo.url}/pull/${prNumber}`;
}

/** ホスト上のコミットの URL。組めなければ null。 */
export function commitHref(target: ForgeTarget, sha: string): string | null {
  const repo = repositoryUrl(target);
  if (!repo || !/^[0-9a-f]{7,40}$/i.test(sha)) return null;
  return repo.kind === 'gitlab' ? `${repo.url}/-/commit/${sha}` : `${repo.url}/commit/${sha}`;
}
