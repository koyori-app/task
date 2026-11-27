import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import ReviewFindingRow from '../ReviewFindingRow.vue';
import type { ReviewFinding } from '@/lib/review-findings';
import {
  OTHER_ID,
  PROJECT_ID,
  TENANT_ID,
  VIEWER_ID,
  bodyButton,
  finding,
  menuItem,
  stubFetch,
} from './review-fixtures';

function mountRow(
  overrides: Partial<ReviewFinding> = {},
  props: { selected?: boolean; pending?: boolean; error?: string | null } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ReviewFindingRow, {
    props: {
      finding: finding(overrides),
      selected: props.selected ?? true,
      href: '/acme/projects/APP/reviews/618?finding=f-1',
      viewerId: VIEWER_ID,
      tenantId: TENANT_ID,
      tenantSlug: 'acme',
      projectId: PROJECT_ID,
      projectKey: 'APP',
      pending: props.pending ?? false,
      error: props.error ?? null,
    },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function buttonLabels(root: Element): string[] {
  return [...root.querySelectorAll('button')].map((b) => b.textContent?.trim() ?? '');
}

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ReviewFindingRow', () => {
  it('畳んだ行は 1 行で、本文と操作を出さない', () => {
    const wrapper = mountRow({}, { selected: false });
    const row = wrapper.get('[data-testid="finding-row"]');
    expect(row.text()).toContain('High');
    expect(row.text()).toContain('Open');
    expect(row.text()).toContain('R1');
    expect(row.text()).toContain(':42');
    expect(row.text()).toContain('認可が抜けている');
    expect(wrapper.find('[data-testid="finding-detail"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('再現条件と根拠');
    expect(row.attributes('data-selected')).toBeUndefined();
    expect(row.attributes('aria-current')).toBeUndefined();
  });

  it('選んだ行は本文と操作を開き、根に data-selected を付ける', () => {
    const wrapper = mountRow();
    const row = wrapper.get('[data-testid="finding-row"]');
    expect(row.attributes('data-selected')).toBe('');
    expect(row.attributes('aria-current')).toBe('true');
    expect(row.attributes('id')).toBe('finding-f-1');
    expect(wrapper.get('[data-testid="finding-detail"]').text()).toContain('再現条件と根拠');
  });

  it('題のクリックは toggle を emit し、修飾キーつきはリンクとして開かせる', async () => {
    const wrapper = mountRow({}, { selected: false });
    const title = wrapper.get('[data-testid="finding-title"]');
    expect(title.attributes('href')).toBe('/acme/projects/APP/reviews/618?finding=f-1');

    // vike のリンク横取りは document の click で、defaultPrevented を見ない。届けばリンク先へ遷移する
    const reachedDocument = vi.fn();
    document.addEventListener('click', reachedDocument);
    try {
      await title.trigger('click', { ctrlKey: true });
      expect(wrapper.emitted('toggle')).toBeUndefined();
      expect(reachedDocument).toHaveBeenCalledTimes(1);

      await title.trigger('click');
      expect(wrapper.emitted('toggle')).toHaveLength(1);
      expect(reachedDocument).toHaveBeenCalledTimes(1);
    } finally {
      document.removeEventListener('click', reachedDocument);
    }
  });

  it('重大度のチップは High・Medium が danger、Low・Nit が neutral', () => {
    const tone = (severity: ReviewFinding['severity']) =>
      mountRow({ severity }, { selected: false })
        .get('[data-testid="finding-row"] [data-appearance="outline"]')
        .attributes('data-tone');
    expect(tone('high')).toBe('danger');
    expect(tone('medium')).toBe('danger');
    expect(tone('low')).toBe('neutral');
    expect(tone('nit')).toBe('neutral');
  });

  it('open は左に着手・修正した、右に繰り延べ・取り下げを出し、取り下げだけ赤い文字にする', () => {
    const wrapper = mountRow({
      severity: 'low',
      available_actions: ['fixing', 'fixed', 'deferred', 'rejected'],
    });
    const actions = wrapper.get('[data-testid="finding-actions"]');
    expect(buttonLabels(actions.element)).toEqual([
      '修正に着手する',
      '修正した',
      '繰り延べる…',
      '取り下げる…',
    ]);
    const buttons = actions.findAll('button');
    expect(buttons.map((b) => b.attributes('data-variant'))).toEqual([
      'secondary',
      'tertiary',
      'ghost',
      'ghost',
    ]);
    expect(buttons[3].attributes('data-tone')).toBe('danger');
    expect(buttons[2].attributes('data-tone')).not.toBe('danger');
  });

  it('入力欄の無い操作は、理由なしで transition を emit する', async () => {
    const wrapper = mountRow();
    bodyButton('修正した')!.click();
    expect(wrapper.emitted('transition')).toEqual([['fixed', null]]);
  });

  it('修正した本人の fixed には、無効の「確認した」と理由を出す', () => {
    const wrapper = mountRow({ state: 'fixed', fixed_by: VIEWER_ID, available_actions: ['open'] });
    expect(bodyButton('確認した')?.disabled).toBe(true);
    expect(wrapper.text()).toContain('修正した人は確認できません');
    expect(bodyButton('レビューに戻す')?.disabled).toBe(false);
  });

  it('本人でない fixed は「確認した」を押せ、理由を出さない', () => {
    const wrapper = mountRow({
      state: 'fixed',
      fixed_by: OTHER_ID,
      available_actions: ['verified', 'open'],
    });
    expect(bodyButton('確認した')?.disabled).toBe(false);
    expect(wrapper.text()).not.toContain('修正した人は確認できません');
  });

  it('遷移を待つ間は全部のボタンを無効にする', () => {
    const wrapper = mountRow({}, { pending: true });
    const buttons = wrapper.get('[data-testid="finding-actions"]').findAll('button');
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.every((b) => b.attributes('disabled') !== undefined)).toBe(true);
  });

  it('繰り延べは行の中に入力欄を開き、理由が空でも送れて note は null', async () => {
    const wrapper = mountRow({ severity: 'low', available_actions: ['fixed', 'deferred'] });
    bodyButton('繰り延べる…')!.click();
    await flushPromises();

    const form = wrapper.get('[data-testid="finding-form-defer"]');
    expect(form.text()).toContain('この指摘を繰り延べる');
    expect(form.text()).toContain('繰り延べると、あとで扱うタスクを自動で作ります。');
    // 開いている間は操作のボタンを隠す
    expect(wrapper.find('[data-testid="finding-actions"]').exists()).toBe(false);
    // タスクの入力欄は出さない（入力は理由だけ）
    expect(form.findAll('textarea, input')).toHaveLength(1);

    const confirm = wrapper.get('[data-testid="finding-form-confirm"]');
    expect(confirm.attributes('disabled')).toBeUndefined();
    await confirm.trigger('click');
    expect(wrapper.emitted('transition')).toEqual([['deferred', null]]);
  });

  it('繰り延べの理由は前後の空白を落として note に入れる', async () => {
    const wrapper = mountRow({ severity: 'low', available_actions: ['deferred'] });
    bodyButton('繰り延べる…')!.click();
    await flushPromises();
    await wrapper.get('textarea').setValue('  別の PR で扱う  ');
    await wrapper.get('[data-testid="finding-form-confirm"]').trigger('click');
    expect(wrapper.emitted('transition')).toEqual([['deferred', '別の PR で扱う']]);
  });

  it('取り下げは理由が空だと確定できず、入力すると理由を note に入れて送る', async () => {
    const wrapper = mountRow();
    bodyButton('取り下げる…')!.click();
    await flushPromises();

    const form = wrapper.get('[data-testid="finding-form-reject"]');
    expect(form.text()).toContain('この指摘を取り下げる');
    // 必須の印は `*`（読み上げには混ぜない）
    expect(form.get('label').find('[aria-hidden="true"]').text()).toBe('*');
    const confirm = wrapper.get('[data-testid="finding-form-confirm"]');
    expect(confirm.attributes('data-variant')).toBe('danger');
    expect(confirm.attributes('disabled')).toBeDefined();

    await wrapper.get('textarea').setValue('   ');
    expect(confirm.attributes('disabled')).toBeDefined();
    await confirm.trigger('click');
    expect(wrapper.emitted('transition')).toBeUndefined();

    await wrapper.get('textarea').setValue('仕様どおりの挙動だった');
    expect(confirm.attributes('disabled')).toBeUndefined();
    await confirm.trigger('click');
    expect(wrapper.emitted('transition')).toEqual([['rejected', '仕様どおりの挙動だった']]);
  });

  it('キャンセルで入力欄を閉じ、操作のボタンへ戻る', async () => {
    const wrapper = mountRow();
    bodyButton('取り下げる…')!.click();
    await flushPromises();
    await wrapper.get('textarea').setValue('書きかけ');
    bodyButton('キャンセル')!.click();
    await flushPromises();

    expect(wrapper.find('[data-testid="finding-form-reject"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="finding-actions"]').exists()).toBe(true);
    expect(wrapper.emitted('transition')).toBeUndefined();
  });

  it('遷移が通って状態が変わると入力欄を閉じる', async () => {
    const wrapper = mountRow();
    bodyButton('取り下げる…')!.click();
    await flushPromises();
    await wrapper.setProps({
      finding: finding({ state: 'rejected', available_actions: ['open'] }),
    });
    expect(wrapper.find('[data-testid="finding-form-reject"]').exists()).toBe(false);
    expect(bodyButton('再オープン')).toBeDefined();
  });

  it('失敗の文は行の中に出す', () => {
    const wrapper = mountRow({}, { error: 'High / Medium の指摘は繰り延べできません' });
    expect(wrapper.get('[data-testid="finding-detail"] [role="alert"]').text()).toContain(
      'High / Medium の指摘は繰り延べできません',
    );
  });

  it('最後の遷移を 1 件だけ出し、メニューから全件の履歴に切り替えられる', async () => {
    const actor = { id: OTHER_ID, username: 'reviewer', avatar_url: null };
    const wrapper = mountRow({
      state: 'fixed',
      transitions: [
        {
          id: 't-1',
          actor,
          from_state: null,
          to_state: 'open',
          note: null,
          created_at: '2026-08-26T00:00:00Z',
        },
        {
          id: 't-2',
          actor: { ...actor, username: 'yupix' },
          from_state: 'open',
          to_state: 'fixed',
          note: '型を直した',
          created_at: '2026-08-27T00:00:00Z',
        },
      ],
    });
    const last = wrapper.get('[data-testid="finding-last-transition"]');
    expect(last.text()).toContain('yupix が Open → Fixed にしました');
    expect(last.text()).toContain('型を直した');
    expect(wrapper.find('[data-testid="finding-history"]').exists()).toBe(false);

    menuItem(wrapper.element, '履歴を表示').click();
    await flushPromises();
    const history = wrapper.get('[data-testid="finding-history"]');
    expect(history.findAll('li')).toHaveLength(2);
    expect(history.text()).toContain('reviewer が Open として登録しました');
    expect(wrapper.find('[data-testid="finding-last-transition"]').exists()).toBe(false);
  });

  it('選んでいない行で「履歴を表示」を選ぶと、行を選ばせる', async () => {
    const wrapper = mountRow({}, { selected: false });
    menuItem(wrapper.element, '履歴を表示').click();
    await flushPromises();
    expect(wrapper.emitted('toggle')).toHaveLength(1);
  });

  it('「リンクをコピー」は絶対 URL を clipboard に入れる', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    const wrapper = mountRow({}, { selected: false });
    menuItem(wrapper.element, 'リンクをコピー').click();
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith(
      `${window.location.origin}/acme/projects/APP/reviews/618?finding=f-1`,
    );
  });

  it('繰り延べ先のタスクは deferred の行だけ、番号のリンクで出す', async () => {
    stubFetch({ tasks: { 'task-uuid': 512 } });
    const wrapper = mountRow(
      { state: 'deferred', deferred_task_id: 'task-uuid', available_actions: ['open'] },
      { selected: false },
    );
    await flushPromises();
    const link = wrapper.get('[data-testid="deferred-task"]');
    expect(link.text()).toBe('APP-512');
    expect(link.attributes('href')).toBe('/acme/projects/APP/tasks/APP-512');
    expect(link.get('[data-tone]').attributes('data-tone')).toBe('info');
  });

  it('繰り延べ先のタスクが取れない（削除済み）ときは何も出さない', async () => {
    stubFetch({});
    const wrapper = mountRow(
      { state: 'deferred', deferred_task_id: 'gone', available_actions: ['open'] },
      { selected: false },
    );
    await flushPromises();
    expect(wrapper.find('[data-testid="deferred-task"]').exists()).toBe(false);
  });

  it('内容を直した指摘にだけ「編集済み」の印を出し、直した時刻を title に出す', () => {
    const edited = mountRow({ edited_at: '2026-11-22T03:04:05Z' });
    const mark = edited.get('[data-testid="finding-edited"]');
    expect(mark.text()).toBe('編集済み');
    expect(mark.attributes('title')).toBe(
      `${new Date('2026-11-22T03:04:05Z').toLocaleString('ja-JP')} に編集`,
    );
    // 画面から直す操作は持たない（直すのは CLI と API）
    expect(edited.text()).not.toContain('編集する');

    const plain = mountRow();
    expect(plain.find('[data-testid="finding-edited"]').exists()).toBe(false);
  });
});
