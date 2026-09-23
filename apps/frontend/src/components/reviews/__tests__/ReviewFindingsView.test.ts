import { describe, it, expect, afterEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import ReviewFindingsView from '../ReviewFindingsView.vue';
import { Select } from '@/components/ui/select';
import type { components } from '@/generated/api';
import type { ReviewFindingsUrlState } from '@/lib/review-findings-url-state';

const TENANT_ID = '11111111-1111-1111-1111-111111111111';
const PROJECT_ID = '00000000-0000-4000-8000-000000000010';
const VIEWER_ID = '00000000-0000-0000-0000-0000000000aa';
const OTHER_ID = '00000000-0000-0000-0000-0000000000bb';

type Finding = components['schemas']['FindingResponse'];

function finding(overrides: Partial<Finding> = {}): Finding {
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
    available_actions: ['fixed'],
    ...overrides,
  };
}

type MockState = {
  findings: Finding[];
  blocking?: number;
  prsStatus?: number;
  patchStatus?: number;
  patchMessage?: string;
  /** 集計の gate。既定は未解決があれば blocked、無ければ ready */
  gate?: components['schemas']['ReviewGate'];
  /** これまでのラウンド数（0 = 未レビュー） */
  rounds?: number;
  /** URL 履歴の試験では選び直せる PR を二つ用意する。 */
  prNumbers?: number[];
  /** 集計対象のリポジトリ。null で「連携なし」を作る */
  repository?: string | null;
  /** 要約ジョブが確かめた現在の head。null で「鮮度不明」を作る */
  cachedHeadSha?: string | null;
  ownerOverrideRejections?: number;
};

const REVIEWED_HEAD = '60cdd7795f94fa4e4148ce996c2efb4c363e3f5e';

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function stubFetch(state: MockState) {
  const patched: { path: string; body: unknown }[] = [];
  const fetchMock = vi.fn(async (req: Request) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;

    if (req.method === 'GET' && pathname.endsWith('/reviews/pull-requests')) {
      if (state.prsStatus) return jsonResponse({ message: 'error' }, state.prsStatus);
      const blocking =
        state.blocking ??
        state.findings.filter(
          (f) =>
            (f.severity === 'high' || f.severity === 'medium') &&
            (f.state === 'open' || f.state === 'fixed'),
        ).length;
      return jsonResponse(
        (state.prNumbers ?? [618]).map((prNumber) => ({
          pr_number: prNumber,
          rounds: state.rounds ?? 1,
          pr_title: `feat: PR ${prNumber}`,
          pr_author: 'yupix',
          unresolved: state.findings.filter((f) => f.state === 'open' || f.state === 'fixed')
            .length,
          blocking,
          last_reviewed_at: '2026-08-26T00:00:00Z',
        })),
      );
    }
    if (req.method === 'GET' && pathname.endsWith('/reviews/summary')) {
      const blocking =
        state.blocking ??
        state.findings.filter(
          (f) =>
            (f.severity === 'high' || f.severity === 'medium') &&
            (f.state === 'open' || f.state === 'fixed'),
        ).length;
      const rounds = state.rounds ?? 1;
      return jsonResponse({
        pr_number: 618,
        rounds,
        counts: state.findings.map((f) => ({
          severity: f.severity,
          state: f.state,
          count: 1,
        })),
        blocking,
        latest_head_sha: rounds > 0 ? REVIEWED_HEAD : null,
        // 既定は「連携あり・レビューした commit が現在の head」＝可を出してよい状態
        repository: state.repository === undefined ? 'acme/app' : state.repository,
        cached_pr_head_sha: state.cachedHeadSha === undefined ? REVIEWED_HEAD : state.cachedHeadSha,
        pr_head_checked_at: '2026-08-28T10:00:00Z',
        owner_override_rejections: state.ownerOverrideRejections ?? 0,
        mergeable: rounds > 0 && blocking === 0,
        gate: state.gate ?? (blocking > 0 ? 'blocked' : 'ready'),
      });
    }
    if (req.method === 'GET' && pathname.endsWith('/reviews')) {
      return jsonResponse([
        {
          id: 'r-1',
          project_id: PROJECT_ID,
          pr_number: 618,
          round: 1,
          head_sha: '60cdd7795f94',
          reviewer: {
            id: OTHER_ID,
            username: 'reviewer',
            avatar_url: null,
          },
          reviewer_left_tenant: false,
          summary: '総評',
          pr_title: null,
          pr_author: null,
          created_at: '2026-08-26T00:00:00Z',
          finding_count: state.findings.length,
        },
      ]);
    }
    if (req.method === 'GET' && pathname.endsWith('/review-findings')) {
      return jsonResponse(state.findings);
    }
    if (req.method === 'PATCH' && pathname.includes('/review-findings/')) {
      const body = await req.clone().json();
      patched.push({ path: pathname, body });
      if (state.patchStatus)
        return jsonResponse({ message: state.patchMessage ?? 'error' }, state.patchStatus);
      const id = pathname.split('/').pop();
      state.findings = state.findings.map((f) =>
        f.id === id ? { ...f, state: (body as { state: Finding['state'] }).state } : f,
      );
      return jsonResponse(state.findings.find((f) => f.id === id));
    }
    return jsonResponse({ message: 'not-found' }, 404);
  });
  vi.stubGlobal('fetch', fetchMock);
  return { patched };
}

function mountView(
  extraProps: {
    initialUrlState?: ReviewFindingsUrlState;
    initialUrlWarnings?: string[];
  } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ReviewFindingsView, {
    props: {
      tenantId: TENANT_ID,
      tenantSlug: 'acme',
      projectId: PROJECT_ID,
      projectKey: 'APP',
      viewerId: VIEWER_ID,
      initialUrlState: extraProps.initialUrlState ?? {
        pr: 618,
        round: null,
        severity: null,
        state: null,
        finding: null,
      },
      ...extraProps,
    },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function bodyButton(label: string) {
  return [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === label);
}

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ReviewFindingsView', () => {
  it('指摘とマージ判定を表示する', async () => {
    stubFetch({ findings: [finding()] });
    const wrapper = mountView();
    await flushPromises();

    const gate = wrapper.get('[data-testid="merge-gate"]');
    expect(gate.text()).toContain('マージ不可');
    const list = wrapper.get('[data-testid="finding-list"]');
    expect(list.text()).toContain('認可が抜けている');
    expect(list.text()).toContain('src/App.vue:42');
    expect(list.text()).toContain('High');
  });

  it('未解決が無ければマージ可を出す', async () => {
    stubFetch({ findings: [finding({ state: 'verified' })] });
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.get('[data-testid="merge-gate"]').text()).toContain('マージ可');
  });

  it('状態遷移を送り、一覧に反映する', async () => {
    const { patched } = stubFetch({ findings: [finding()] });
    const wrapper = mountView();
    await flushPromises();

    bodyButton('修正した')!.click();
    await flushPromises();

    expect(patched).toHaveLength(1);
    expect(patched[0].body).toEqual({ state: 'fixed', note: null });
    expect(patched[0].path.endsWith('/review-findings/f-1')).toBe(true);
    expect(wrapper.get('[data-testid="finding-list"]').text()).toContain('Fixed');
  });

  it('自分が直した指摘には確認が出ず、理由を出す', async () => {
    // 確認を出さない判定は backend（available_actions）。画面は説明を添えるだけ
    stubFetch({
      findings: [finding({ state: 'fixed', fixed_by: VIEWER_ID, available_actions: ['open'] })],
    });
    const wrapper = mountView();
    await flushPromises();

    expect(bodyButton('確認した')).toBeUndefined();
    expect(wrapper.text()).toContain('修正者と確認者は別の人である必要があります');
    expect(bodyButton('レビューに戻す')?.disabled).toBe(false);
  });

  it('available_actions の遷移先がそのまま操作ボタンとして出る', async () => {
    stubFetch({
      findings: [finding({ severity: 'low', available_actions: ['fixed', 'deferred'] })],
    });
    mountView();
    await flushPromises();

    expect(bodyButton('修正した')?.disabled).toBe(false);
    expect(bodyButton('繰り延べる')?.disabled).toBe(false);
    expect(bodyButton('指摘を取り下げる')).toBeUndefined();
  });

  it('一覧バッジは件数だけを出し、可否を断定しない', async () => {
    // 未解決ゼロでも「マージ可」ではなく「未解決なし」。可否には鮮度と連携の
    // 有無も要り、一覧はその材料（latest_head_sha / cached_pr_head_sha）を持たない
    stubFetch({ findings: [] });
    const wrapper = mountView();
    await flushPromises();
    expect(wrapper.text()).toContain('未解決なし');

    // 未解決があれば件数
    stubFetch({ findings: [finding({ severity: 'high' })] });
    const blocked = mountView();
    await flushPromises();
    expect(blocked.text()).toContain('1 件が未解決');
  });

  it('mergeVerdict の title / detail が merge-gate に出る', async () => {
    // 不可の分岐（未レビュー・連携なし・鮮度）は lib の mergeVerdict 側で見る。
    stubFetch({ findings: [] });
    const wrapper = mountView();
    await flushPromises();

    const gate = wrapper.get('[data-testid="merge-gate"]').text();
    expect(gate).toContain('マージ可');
    expect(gate).toContain(REVIEWED_HEAD.slice(0, 7));
  });

  it('オーナー代行での棄却は件数を出す', async () => {
    stubFetch({ findings: [], ownerOverrideRejections: 2 });
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.get('[data-testid="merge-gate"]').text()).toContain('オーナー代行での棄却 2 件');
  });

  it('サーバーが理由を返した 409 はその文言を出す', async () => {
    stubFetch({
      findings: [finding({ severity: 'low', available_actions: ['fixed', 'deferred'] })],
      patchStatus: 409,
      patchMessage: 'high の指摘は繰り延べられません（繰り延べは low / nit のみ）',
    });
    const wrapper = mountView();
    await flushPromises();

    bodyButton('繰り延べる')!.click();
    await flushPromises();

    expect(wrapper.text()).toContain('繰り延べは low / nit のみ');
  });

  it('スラグだけの本文は出さず、状態に応じた説明に落とす', async () => {
    stubFetch({
      findings: [finding({ severity: 'low', available_actions: ['fixed', 'deferred'] })],
      patchStatus: 409,
      patchMessage: 'conflict',
    });
    const wrapper = mountView();
    await flushPromises();

    bodyButton('繰り延べる')!.click();
    await flushPromises();

    expect(wrapper.text()).toContain('いまの状態からは行えない操作です');
    expect(wrapper.text()).not.toContain('conflict');
  });

  it('403 のときは理由を表示する', async () => {
    stubFetch({
      findings: [
        finding({ state: 'fixed', fixed_by: OTHER_ID, available_actions: ['open', 'verified'] }),
      ],
      patchStatus: 403,
    });
    const wrapper = mountView();
    await flushPromises();

    bodyButton('確認した')!.click();
    await flushPromises();

    expect(wrapper.text()).toContain('この操作はレビュー側だけが行えます');
  });

  it('409 のときは再読み込みを促す', async () => {
    stubFetch({ findings: [finding()], patchStatus: 409 });
    const wrapper = mountView();
    await flushPromises();

    bodyButton('修正した')!.click();
    await flushPromises();

    expect(wrapper.text()).toContain('いまの状態からは行えない操作です');
  });

  it('読み込みに失敗したらエラーを表示する', async () => {
    stubFetch({ findings: [], prsStatus: 500 });
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.text()).toContain('レビューを読み込めませんでした');
  });

  it('冷えた URL から PR・絞り込み・指摘を復元して強調する', async () => {
    window.history.replaceState(
      {},
      '',
      '/acme/projects/APP/reviews?pr=618&round=1&severity=high&state=open&finding=f-1',
    );
    stubFetch({
      findings: [finding(), finding({ id: 'f-2', title: '軽微な指摘', severity: 'low' })],
    });
    const wrapper = mountView({
      initialUrlState: {
        pr: 618,
        round: 1,
        severity: 'high',
        state: 'open',
        finding: 'f-1',
      },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="finding-list"]').text()).toContain('認可が抜けている');
    expect(wrapper.get('[data-testid="finding-list"]').text()).not.toContain('軽微な指摘');
    expect(wrapper.get('#finding-f-1').attributes('data-focused')).toBe('true');
    const findingLink = wrapper
      .get('a[aria-label*="認可が抜けている"]')
      .element.getAttribute('href');
    expect(findingLink).toContain('finding=f-1');
  });

  it('存在しない PR と指摘を指す URL でも壊れず理由を表示する', async () => {
    stubFetch({ findings: [finding()] });
    const wrapper = mountView({
      initialUrlState: {
        pr: 999,
        round: null,
        severity: null,
        state: null,
        finding: 'missing-finding',
      },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="missing-pr"]').text()).toContain('PR #999');
    expect(wrapper.get('[data-testid="missing-finding"]').text()).toContain('missing-finding');
  });

  it('不正な query を黙って捨てず警告する', async () => {
    stubFetch({ findings: [finding()] });
    const wrapper = mountView({
      initialUrlWarnings: ['URL の重大度「urgent」は知らない値のため無視しました。'],
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="url-warning"]').text()).toContain('urgent');
  });

  it('PR の選び直しを履歴へ積み、戻る・進むの popstate で URL を正本にする', async () => {
    window.history.replaceState({}, '', '/acme/projects/APP/reviews?pr=617');
    const pushSpy = vi.spyOn(window.history, 'pushState');
    stubFetch({ findings: [finding()], prNumbers: [617, 618] });
    const wrapper = mountView({
      initialUrlState: {
        pr: 617,
        round: null,
        severity: null,
        state: null,
        finding: null,
      },
    });
    await flushPromises();

    await wrapper
      .findAll('nav[aria-label="レビューのある PR"] button')
      .find((button) => button.text().includes('#618'))!
      .trigger('click');
    expect(pushSpy).toHaveBeenCalledOnce();
    expect(new URL(window.location.href).searchParams.get('pr')).toBe('618');

    window.history.replaceState({}, '', '/acme/projects/APP/reviews?pr=617');
    window.dispatchEvent(new PopStateEvent('popstate'));
    await flushPromises();
    const previous = wrapper
      .findAll('nav[aria-label="レビューのある PR"] button')
      .find((button) => button.text().includes('#617'))!;
    expect(previous.attributes('aria-current')).toBe('true');

    window.history.replaceState({}, '', '/acme/projects/APP/reviews?pr=618');
    window.dispatchEvent(new PopStateEvent('popstate'));
    await flushPromises();
    const next = wrapper
      .findAll('nav[aria-label="レビューのある PR"] button')
      .find((button) => button.text().includes('#618'))!;
    expect(next.attributes('aria-current')).toBe('true');
  });

  it('絞り込み変更は履歴を積まず現在の URL を差し替える', async () => {
    window.history.replaceState({}, '', '/acme/projects/APP/reviews?pr=618');
    const replaceSpy = vi.spyOn(window.history, 'replaceState');
    stubFetch({ findings: [finding()] });
    const wrapper = mountView({
      initialUrlState: {
        pr: 618,
        round: null,
        severity: null,
        state: null,
        finding: null,
      },
    });
    await flushPromises();

    wrapper.findAllComponents(Select)[1].vm.$emit('update:modelValue', 'high');
    await flushPromises();

    expect(replaceSpy).toHaveBeenCalledOnce();
    expect(new URL(window.location.href).searchParams.get('severity')).toBe('high');
  });

  it('vike の遷移（props の更新）でも URL 状態が復元される', async () => {
    // 同じ +Page.vue に解決される URL 間の遷移では component は差し替わらず props だけ
    // 変わる。popstate は飛ばないため、props 経路の復元が無いと setup 時の状態で凍る
    stubFetch({ findings: [finding()], prNumbers: [617, 618] });
    const wrapper = mountView({
      initialUrlState: { pr: 617, round: null, severity: null, state: null, finding: null },
    });
    await flushPromises();

    await wrapper.setProps({
      initialUrlState: { pr: 618, round: null, severity: null, state: null, finding: null },
    });
    await flushPromises();

    const next = wrapper
      .findAll('nav[aria-label="レビューのある PR"] button')
      .find((button) => button.text().includes('#618'))!;
    expect(next.attributes('aria-current')).toBe('true');
  });

  it('注目指摘への強制スクロールは一度きり——再取得で画面が勝手に戻らない', async () => {
    const scrollSpy = vi.fn();
    Element.prototype.scrollIntoView = scrollSpy;
    window.history.replaceState({}, '', '/acme/projects/APP/reviews?pr=618&finding=f-1');
    stubFetch({ findings: [finding()] });
    const wrapper = mountView({
      initialUrlState: { pr: 618, round: null, severity: null, state: null, finding: 'f-1' },
    });
    await flushPromises();
    expect(scrollSpy).toHaveBeenCalledTimes(1);

    // 状態変更 → invalidate → findings の再取得。ここで再スクロールしてはならない
    // （一覧を繰った直後に画面が注目指摘へ戻る、最も起きてほしくない場面）
    const fixedButton = wrapper
      .findAll('#finding-f-1 button')
      .find((button) => button.text().includes('修正した'))!;
    await fixedButton.trigger('click');
    await flushPromises();
    expect(scrollSpy).toHaveBeenCalledTimes(1);
    // @ts-expect-error jsdom には元より無いので試験専用の後片付け
    delete Element.prototype.scrollIntoView;
  });

  it('PR を選び直して URL から不正値が消えたら、警告の帯も片付く', async () => {
    stubFetch({ findings: [finding()], prNumbers: [617, 618] });
    const wrapper = mountView({
      initialUrlState: { pr: 617, round: null, severity: null, state: null, finding: null },
      initialUrlWarnings: ['URL の重大度「urgent」は知らない値のため無視しました。'],
    });
    await flushPromises();
    expect(wrapper.find('[data-testid="url-warning"]').exists()).toBe(true);

    await wrapper
      .findAll('nav[aria-label="レビューのある PR"] button')
      .find((button) => button.text().includes('#618'))!
      .trigger('click');
    await flushPromises();
    expect(wrapper.find('[data-testid="url-warning"]').exists()).toBe(false);
  });

  it('存在しない Round を指す URL には理由を表示する（pr・finding と同じ扱い）', async () => {
    stubFetch({ findings: [finding()] });
    const wrapper = mountView({
      initialUrlState: { pr: 618, round: 99, severity: null, state: null, finding: null },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="missing-round"]').text()).toContain('Round 99');
  });

  const manyPrNumbers = [770, 776, 777, 778, 779, 618];

  function prNavButtons() {
    return [...document.body.querySelectorAll('nav[aria-label="レビューのある PR"] button')];
  }

  it('PR 番号の部分一致で一覧を絞り、空にすると全件へ戻る', async () => {
    stubFetch({ findings: [finding()], prNumbers: manyPrNumbers });
    const wrapper = mountView({
      initialUrlState: { pr: 618, round: null, severity: null, state: null, finding: null },
    });
    await flushPromises();

    expect(prNavButtons()).toHaveLength(6);

    const input = wrapper.get('[data-testid="filter-pr-number"]');
    await input.setValue('77');
    await flushPromises();
    expect(prNavButtons()).toHaveLength(5);
    expect(prNavButtons().every((b) => b.textContent?.includes('#77'))).toBe(true);

    await input.setValue('99999');
    await flushPromises();
    expect(prNavButtons()).toHaveLength(0);
    expect(wrapper.get('[data-testid="no-pr-match"]').text()).toContain('該当する PR');

    await input.setValue('');
    await flushPromises();
    expect(prNavButtons()).toHaveLength(6);
  });
});
