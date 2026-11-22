import { vi } from 'vitest';

import type { components } from '@/generated/api';

import { transitionedFinding } from './transition-mock';

type Finding = components['schemas']['FindingResponse'];
type PullRequest = components['schemas']['ReviewedPullRequest'];
type Round = components['schemas']['ReviewResponse'];
type Summary = components['schemas']['ReviewSummaryResponse'];

export const TENANT_ID = '11111111-1111-1111-1111-111111111111';
export const PROJECT_ID = '00000000-0000-4000-8000-000000000010';
export const VIEWER_ID = '00000000-0000-0000-0000-0000000000aa';
export const OTHER_ID = '00000000-0000-0000-0000-0000000000bb';
const REVIEWED_HEAD = '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e';

export function finding(overrides: Partial<Finding> = {}): Finding {
  return {
    id: 'f-1',
    review_id: 'r-1',
    pr_number: 618,
    round: 1,
    severity: 'high',
    title: '認可が抜けている',
    body: '再現条件と根拠',
    file: 'src/App.vue',
    line: 42,
    state: 'open',
    deferred_task_id: null,
    fixed_by: null,
    created_at: '2026-08-26T00:00:00Z',
    updated_at: '2026-08-26T00:00:00Z',
    transitions: [],
    // 遷移先は backend が要求者ごとに返す。画面はこれをそのままボタンにする
    available_actions: ['fixing', 'fixed', 'rejected'],
    ...overrides,
  };
}

export function pullRequest(overrides: Partial<PullRequest> = {}): PullRequest {
  return {
    pr_number: 618,
    rounds: 1,
    pr_title: 'feat: PR 618',
    pr_author: 'yupix',
    unresolved: 0,
    blocking: 0,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    repository: 'acme/app',
    host: 'github',
    html_url: null,
    last_reviewed_at: '2026-08-26T00:00:00Z',
    state: 'open',
    ...overrides,
  };
}

export function round(overrides: Partial<Round> = {}): Round {
  return {
    id: 'r-1',
    project_id: PROJECT_ID,
    pr_number: 618,
    round: 1,
    head_sha: REVIEWED_HEAD,
    reviewer: { id: OTHER_ID, username: 'reviewer', avatar_url: null },
    reviewer_left_tenant: false,
    summary: '総評',
    pr_title: null,
    pr_author: null,
    created_at: '2026-08-26T00:00:00Z',
    finding_count: 1,
    ...overrides,
  };
}

export function summary(overrides: Partial<Summary> = {}): Summary {
  return {
    pr_number: 618,
    rounds: 1,
    counts: [],
    blocking: 0,
    latest_head_sha: REVIEWED_HEAD,
    repository: 'acme/app',
    cached_pr_head_sha: REVIEWED_HEAD,
    pr_head_checked_at: '2026-08-28T10:00:00Z',
    owner_override_rejections: 0,
    mergeable: true,
    gate: 'ready',
    ...overrides,
  };
}

export type MockState = {
  pullRequests?: PullRequest[];
  findings?: Finding[];
  rounds?: Round[];
  /** 集計の上書き。gate の既定は、塞ぐ指摘があれば blocked、無ければ ready */
  summary?: Partial<Summary>;
  prsStatus?: number;
  findingsStatus?: number;
  /** 指摘の一覧の応答を返さない（取り直しを止めておく） */
  hangFindings?: boolean;
  summaryStatus?: number;
  /** ラウンドの一覧の応答を失敗させる */
  roundsStatus?: number;
  /** ラウンドの一覧の応答を返さない */
  hangRounds?: boolean;
  patchStatus?: number;
  patchMessage?: string;
  /** 閲覧者の連携 GitHub のユーザー名。null・未指定で未連携 */
  githubLogin?: string | null;
  /** GitHub と連携しているがユーザー名が分からない（名前を控える前からある連携） */
  githubLinkedWithoutLogin?: boolean;
  connectionsStatus?: number;
  /** 連携一覧の応答を返さない（外部への問い合わせで遅れている） */
  hangConnections?: boolean;
  /** 繰り延べ先のタスク（UUID → 番号） */
  tasks?: Record<string, number>;
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

const blocks = (f: Finding) =>
  (f.severity === 'high' || f.severity === 'medium') &&
  (f.state === 'open' || f.state === 'fixing' || f.state === 'fixed');

/** レビュー系 API をインメモリの状態で応答する fetch のモック */
export function stubFetch(state: MockState) {
  const patched: { path: string; body: unknown }[] = [];
  const posted: unknown[] = [];
  const calls: string[] = [];
  const fetchMock = vi.fn(async (req: Request) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    calls.push(`${req.method} ${pathname}`);
    const findings = state.findings ?? [];

    if (req.method === 'GET' && pathname.endsWith('/reviews/pull-requests')) {
      if (state.prsStatus) return jsonResponse({ message: 'error' }, state.prsStatus);
      return jsonResponse(state.pullRequests ?? [pullRequest()]);
    }
    if (req.method === 'GET' && pathname.endsWith('/v1/auth/oauth/connections')) {
      if (state.hangConnections) return new Promise<Response>(() => {});
      if (state.connectionsStatus)
        return jsonResponse({ message: 'error' }, state.connectionsStatus);
      return jsonResponse({
        connections: state.githubLogin
          ? [
              {
                provider: 'github',
                provider_login: state.githubLogin,
                connected_at: '2026-08-01T00:00:00Z',
              },
            ]
          : state.githubLinkedWithoutLogin
            ? [{ provider: 'github', connected_at: '2026-08-01T00:00:00Z' }]
            : [],
      });
    }
    if (req.method === 'GET' && pathname.endsWith('/reviews/summary')) {
      if (state.summaryStatus) return jsonResponse({ message: 'error' }, state.summaryStatus);
      const blocking = findings.filter(blocks).length;
      return jsonResponse(
        summary({
          counts: findings.map((f) => ({ severity: f.severity, state: f.state, count: 1 })),
          blocking,
          gate: blocking > 0 ? 'blocked' : 'ready',
          ...state.summary,
        }),
      );
    }
    if (req.method === 'GET' && pathname.endsWith('/reviews')) {
      if (state.hangRounds) return new Promise<Response>(() => {});
      if (state.roundsStatus) return jsonResponse({ message: 'error' }, state.roundsStatus);
      return jsonResponse(state.rounds ?? [round()]);
    }
    if (req.method === 'POST' && pathname.endsWith('/reviews')) {
      posted.push(await req.clone().json());
      state.rounds = [...(state.rounds ?? [round()]), round({ id: 'r-new', round: 2 })];
      return jsonResponse({ id: 'r-new', round: 2 }, 201);
    }
    if (req.method === 'GET' && pathname.endsWith('/review-findings')) {
      if (state.hangFindings) return new Promise<Response>(() => {});
      if (state.findingsStatus) return jsonResponse({ message: 'error' }, state.findingsStatus);
      return jsonResponse(findings);
    }
    if (req.method === 'PATCH' && pathname.includes('/review-findings/')) {
      const body = (await req.clone().json()) as {
        state: Finding['state'];
        note: string | null;
      };
      patched.push({ path: pathname, body });
      if (state.patchStatus)
        return jsonResponse({ message: state.patchMessage ?? 'error' }, state.patchStatus);
      const id = pathname.split('/').pop();
      state.findings = findings.map((f) =>
        f.id === id
          ? transitionedFinding(
              f,
              body.state,
              { id: VIEWER_ID, username: 'yupix', avatar_url: null },
              body.note,
              '2026-08-27T00:00:00Z',
            )
          : f,
      );
      return jsonResponse(state.findings.find((f) => f.id === id));
    }
    if (req.method === 'GET' && pathname.includes('/tasks/')) {
      const id = pathname.split('/').pop() ?? '';
      const seqId = state.tasks?.[id];
      if (seqId === undefined) return jsonResponse({ message: 'not-found' }, 404);
      return jsonResponse({ id, seq_id: seqId, title: '繰り延べ先' });
    }
    return jsonResponse({ message: 'not-found' }, 404);
  });
  vi.stubGlobal('fetch', fetchMock);
  return { patched, posted, calls };
}

/** 文字が一致するボタン（body 全体から探す） */
export function bodyButton(label: string): HTMLButtonElement | undefined {
  return [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === label);
}

/** Dropdown の項目（閉じている間も DOM にある）を、役割と文字で引く */
export function menuItem(root: ParentNode, label: string): HTMLElement {
  const item = [...root.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find(
    (el) => el.textContent?.trim() === label,
  );
  if (!item) throw new Error(`menu item ${label} not found`);
  return item;
}
