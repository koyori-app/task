import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import { transitionedFinding } from '@/components/reviews/__tests__/transition-mock';
import type { components } from '@/generated/api';

type Finding = components['schemas']['FindingResponse'];
type PullRequest = components['schemas']['ReviewedPullRequest'];
type Round = components['schemas']['ReviewResponse'];
type Summary = components['schemas']['ReviewSummaryResponse'];

export const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
export const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';
export const VIEWER_ID = '00000000-0000-0000-0000-0000000000aa';
const OTHER_ID = '00000000-0000-0000-0000-0000000000bb';
/** レビューした commit ＝ 今の head（鮮度は満たしている状態）。 */
const HEAD_SHA = 'be31c05f9a1244d3b0e6f7a8c9d0e1f2a3b4c5d6';
const DEFERRED_TASK_ID = '00000000-0000-4000-8000-000000000512';

const viewer = { id: VIEWER_ID, username: 'yupix', avatar_url: null };
const reviewer = { id: OTHER_ID, username: 'reviewer', avatar_url: null };

// Storybook の時刻は 2026-07-15T12:00+09:00 に固定してある（.storybook/preview.ts）
const at = (minutesAgo: number) =>
  new Date(Date.parse('2026-07-15T12:00:00+09:00') - minutesAgo * 60_000).toISOString();

function pullRequest(overrides: Partial<PullRequest> = {}): PullRequest {
  return {
    pr_number: 231,
    rounds: 3,
    pr_title: 'タスク一覧の絞り込みを URL に持つ',
    pr_author: 'yupix',
    unresolved: 6,
    blocking: 4,
    blocking_fixed: 1,
    unresolved_fixed: 1,
    repository: 'koyori-app/task',
    host: 'github',
    html_url: 'https://github.com/koyori-app/task/pull/231',
    last_reviewed_at: at(120),
    state: 'open',
    ...overrides,
  };
}

/** figma/1 の 6 行と、ページの境界（10 件）を越えるための 6 行 */
const listPullRequests: PullRequest[] = [
  pullRequest({
    pr_number: 236,
    pr_title: 'IconPicker の選択を連動させる',
    rounds: 1,
    unresolved: 2,
    blocking: 2,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    last_reviewed_at: at(30),
  }),
  pullRequest(),
  pullRequest({
    pr_number: 233,
    pr_title: 'メンバー一覧に役割の絞り込みを足す',
    pr_author: 'hana',
    rounds: 2,
    unresolved: 3,
    blocking: 2,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    last_reviewed_at: at(26 * 60),
  }),
  pullRequest({
    pr_number: 228,
    pr_title: 'メール確認の再送で古いトークンが勝つのを直す',
    pr_author: 'hana',
    rounds: 1,
    unresolved: 1,
    blocking: 1,
    blocking_fixed: 1,
    unresolved_fixed: 1,
    last_reviewed_at: at(3 * 24 * 60),
  }),
  pullRequest({
    pr_number: 229,
    pr_title: 'ガントの依存線をドラッグで張る',
    pr_author: 'sota',
    rounds: 2,
    unresolved: 5,
    blocking: 0,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    last_reviewed_at: at(4 * 24 * 60),
  }),
  pullRequest({
    pr_number: 222,
    pr_title: '通知の既読をまとめて付ける',
    rounds: 1,
    unresolved: 1,
    blocking: 0,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    last_reviewed_at: at(8 * 24 * 60),
  }),
  ...Array.from({ length: 6 }, (_, i) =>
    pullRequest({
      pr_number: 215 - i,
      pr_title: `一覧の細かな修正 ${i + 1}`,
      pr_author: i % 2 === 0 ? 'sota' : 'hana',
      rounds: 1,
      unresolved: 0,
      blocking: 0,
      blocking_fixed: 0,
      unresolved_fixed: 0,
      last_reviewed_at: at((10 + i) * 24 * 60),
    }),
  ),
  pullRequest({
    pr_number: 201,
    pr_title: 'サイドバーの折りたたみを覚える',
    pr_author: 'sota',
    rounds: 2,
    unresolved: 0,
    blocking: 0,
    blocking_fixed: 0,
    unresolved_fixed: 0,
    state: 'merged',
    last_reviewed_at: at(40 * 24 * 60),
  }),
];

function finding(overrides: Partial<Finding>): Finding {
  const state = overrides.state ?? 'open';
  return {
    id: 'f-1',
    review_id: `r-${overrides.round ?? 1}`,
    pr_number: 231,
    round: 1,
    severity: 'medium',
    title: '',
    body: '再現条件と根拠。',
    file: null,
    line: null,
    state,
    deferred_task_id: state === 'deferred' ? DEFERRED_TASK_ID : null,
    fixed_by: null,
    created_at: at(3 * 24 * 60),
    updated_at: at(120),
    transitions: [
      {
        id: `t-${overrides.id}-0`,
        actor: reviewer,
        from_state: null,
        to_state: 'open',
        note: null,
        created_at: at(3 * 24 * 60),
      },
    ],
    available_actions: [],
    ...overrides,
  };
}

const LIB = 'apps/frontend/src/lib';

/** figma/2 の 10 件（6 ファイル） */
export function detailFindings(): Finding[] {
  return [
    finding({
      id: 'f-url-state',
      severity: 'high',
      state: 'open',
      round: 3,
      file: `${LIB}/task-list-url-state.ts`,
      line: 42,
      title: 'URL の絞り込みを読むときに未知の値で落ちる',
      body: 'status= に未知の値が来ると find が undefined を返し、その先で .id を読んで例外になる。未知の値は捨てて警告に回すこと。',
      available_actions: ['fixing', 'fixed', 'rejected'],
    }),
    finding({
      id: 'f-assignee',
      severity: 'medium',
      state: 'verified',
      file: `${LIB}/task-list-url-state.ts`,
      line: 20,
      title: '担当者 ID の形を確かめていない',
    }),
    finding({
      id: 'f-typo',
      severity: 'nit',
      state: 'verified',
      file: `${LIB}/task-list-url-state.ts`,
      line: 3,
      title: 'コメントの誤字（絞り混み）',
    }),
    finding({
      id: 'f-sort-default',
      severity: 'medium',
      state: 'open',
      round: 3,
      file: `${LIB}/task-list-url.ts`,
      line: 57,
      title: '並び替えの既定値が URL に毎回載る',
      available_actions: ['fixing', 'fixed', 'rejected'],
    }),
    finding({
      id: 'f-query-order',
      severity: 'low',
      state: 'open',
      round: 3,
      file: `${LIB}/task-list-url.ts`,
      line: 30,
      title: 'クエリの並び順が保存のたびに変わる',
      available_actions: ['fixing', 'fixed', 'deferred', 'rejected'],
    }),
    finding({
      id: 'f-split',
      severity: 'low',
      state: 'deferred',
      file: `${LIB}/task-list-url.ts`,
      line: 12,
      title: 'URL の組み立てを関数に分ける',
      available_actions: ['open'],
    }),
    finding({
      id: 'f-rename',
      severity: 'low',
      state: 'rejected',
      round: 2,
      file: `${LIB}/task-list-url.ts`,
      line: 8,
      title: 'label を labels に改名する',
      available_actions: ['open'],
    }),
    finding({
      id: 'f-composable',
      severity: 'high',
      state: 'fixed',
      round: 2,
      file: 'apps/frontend/src/composables/useTaskListUrl.ts',
      line: 18,
      title: '戻る・進むで絞り込みが復元されない',
      fixed_by: OTHER_ID,
      available_actions: ['verified', 'open'],
    }),
    finding({
      id: 'f-page',
      severity: 'medium',
      state: 'fixing',
      round: 3,
      file: 'apps/frontend/src/pages/@tenant/projects/@projectKey/tasks/+Page.vue',
      line: 318,
      title: 'ページ番号が URL に残る',
      available_actions: ['fixed', 'open'],
    }),
    finding({
      id: 'f-query-field',
      severity: 'low',
      state: 'open',
      round: 3,
      file: 'apps/frontend/src/components/tasks/query/TaskQueryField.vue',
      line: 88,
      title: '入力のたびに URL を書き直している',
      available_actions: ['fixing', 'fixed', 'deferred', 'rejected'],
    }),
  ];
}

const rounds: Round[] = [3, 2, 1].map((round) => ({
  id: `r-${round}`,
  project_id: PROJECT_UUID,
  pr_number: 231,
  round,
  // API は 40 桁の小文字 16 進しか受け付けない。短縮 SHA をモックに置くと、
  // 実データでは起きない見え方（表示側の slice 前提など）を通してしまう
  head_sha: round === 3 ? HEAD_SHA : `${round}7bd214c8e0198a7b6c5d4e3f2a1b0c9d8e7f6a5`,
  reviewer,
  reviewer_left_tenant: false,
  summary: '総評',
  pr_title: null,
  pr_author: null,
  created_at: at(round === 3 ? 120 : (4 - round) * 24 * 60),
  finding_count: 3,
}));

export type ReviewMockOptions = {
  pullRequests?: PullRequest[];
  findings?: Finding[];
  summary?: Partial<Summary>;
  /** 閲覧者の GitHub のユーザー名。null は未連携 */
  githubLogin?: string | null;
  /** PR の一覧を失敗させる */
  prsStatus?: number;
  /** PR の一覧の応答を返さない（読み込み中のまま） */
  hangPullRequests?: boolean;
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

const blocks = (f: Finding) =>
  (f.severity === 'high' || f.severity === 'medium') &&
  (f.state === 'open' || f.state === 'fixing' || f.state === 'fixed');

/** レビュー系 API をインメモリの配列で応答する fetch のモック（story の beforeEach に渡す） */
export function mockReviewFetch(options: ReviewMockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    let findings = (options.findings ?? detailFindings()).map((f) => ({ ...f }));

    globalThis.fetch = (async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;

      if (method === 'GET' && pathname.endsWith('/reviews/pull-requests')) {
        if (options.hangPullRequests) return new Promise<Response>(() => {});
        if (options.prsStatus) return jsonResponse({ message: 'error' }, options.prsStatus);
        return jsonResponse(options.pullRequests ?? listPullRequests);
      }
      if (method === 'GET' && pathname.endsWith('/v1/auth/oauth/connections')) {
        return jsonResponse({
          connections:
            options.githubLogin === null
              ? []
              : [
                  {
                    provider: 'github',
                    provider_login: options.githubLogin ?? 'yupix',
                    connected_at: '2026-06-01T00:00:00Z',
                  },
                ],
        });
      }
      if (method === 'GET' && pathname.endsWith('/reviews/summary')) {
        const blocking = findings.filter(blocks).length;
        return jsonResponse({
          pr_number: 231,
          rounds: 3,
          counts: findings.map((f) => ({ severity: f.severity, state: f.state, count: 1 })),
          blocking,
          mergeable: blocking === 0,
          // gate は backend の判定。材料の欄もそれと矛盾しない値にしておく
          gate: blocking === 0 ? 'ready' : 'blocked',
          repository: 'koyori-app/task',
          owner_override_rejections: 0,
          latest_head_sha: HEAD_SHA,
          cached_pr_head_sha: HEAD_SHA,
          pr_head_checked_at: at(55),
          ...options.summary,
        } satisfies Summary);
      }
      if (method === 'GET' && pathname.endsWith('/reviews')) return jsonResponse(rounds);
      if (method === 'POST' && pathname.endsWith('/reviews')) {
        return jsonResponse({ id: 'r-4', round: 4 }, 201);
      }
      if (method === 'GET' && pathname.endsWith('/review-findings')) {
        return jsonResponse(findings);
      }
      if (method === 'PATCH' && pathname.includes('/review-findings/')) {
        const body = (await (req as Request).json()) as {
          state: Finding['state'];
          note: string | null;
        };
        const id = pathname.split('/').pop();
        findings = findings.map((f) =>
          f.id === id ? transitionedFinding(f, body.state, viewer, body.note, at(0)) : f,
        );
        return jsonResponse(findings.find((f) => f.id === id));
      }
      if (method === 'GET' && pathname.endsWith(`/tasks/${DEFERRED_TASK_ID}`)) {
        return jsonResponse({ id: DEFERRED_TASK_ID, seq_id: 512, title: 'URL の組み立てを分ける' });
      }
      return jsonResponse({ message: 'not-found' }, 404);
    }) as typeof fetch;

    return () => {
      globalThis.fetch = original;
    };
  };
}

/** story ごとに QueryClient を作り直す（前の story のキャッシュを持ち越さない） */
export function queryClientDecorator() {
  return () => ({
    setup() {
      provide(
        VUE_QUERY_CLIENT,
        new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        }),
      );
    },
    template: '<story />',
  });
}
