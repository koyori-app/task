import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient, useQuery } from '@tanstack/vue-query';
import { loadEmojiData } from '@/lib/emoji-data';

const { createMutateAsync, updateMutateAsync, deleteMutateAsync, queryState, mutationPending } =
  vi.hoisted(() => ({
    createMutateAsync: vi.fn(),
    updateMutateAsync: vi.fn(),
    deleteMutateAsync: vi.fn(),
    queryState: {
      labels: [] as unknown[],
      isPending: false,
      isError: false,
    },
    // vi.hoisted は import 前に走るため vue の ref は使えない。
    // コンポーネントは .value を読むだけなので素のオブジェクトで足りる
    mutationPending: {
      post: { value: false },
      put: { value: false },
      delete: { value: false },
    },
  }));

// LabelsSection は共有ヘルパー projectLabelsQueryOptions を素の useQuery で消費するため、
// query は @tanstack/vue-query の useQuery をモックする（ヘルパー自体は実物 pass-through）。
vi.mock('@tanstack/vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tanstack/vue-query')>();
  return {
    ...actual,
    useQuery: vi.fn(() => ({
      data: { value: queryState.labels },
      isPending: { value: queryState.isPending },
      isError: { value: queryState.isError },
    })),
  };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string) => ({
        mutateAsync:
          method === 'post'
            ? createMutateAsync
            : method === 'put'
              ? updateMutateAsync
              : deleteMutateAsync,
        isPending:
          method === 'post'
            ? mutationPending.post
            : method === 'put'
              ? mutationPending.put
              : mutationPending.delete,
      })),
    },
  };
});

import LabelsSection from '../LabelsSection.vue';
import type { components } from '@/generated/api';

type LabelResponse = components['schemas']['LabelResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const bugLabel: LabelResponse = {
  id: '00000000-0000-4000-8000-000000000021',
  name: 'bug',
  description: '不具合の報告',
  color: '#ef4444',
  icon_url: null,
  project_id: PROJECT_UUID,
};

const enhancementLabel: LabelResponse = {
  id: '00000000-0000-4000-8000-000000000022',
  name: 'enhancement',
  description: '機能改善の提案',
  color: '#3b82f6',
  icon_url: null,
  project_id: PROJECT_UUID,
};

const designLabel: LabelResponse = {
  id: '00000000-0000-4000-8000-000000000023',
  name: 'デザイン',
  description: '',
  color: '#8b5cf6',
  icon_url: null,
  icon_emoji: '🎨',
  project_id: PROJECT_UUID,
};

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(LabelsSection, {
    props: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function input(id: string) {
  const el = document.body.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function formEl() {
  const el = document.body.querySelector('form');
  if (!el) throw new Error('form not found');
  return new DOMWrapper(el);
}

function clickBodyButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

/** 開いているダイアログ。閉じたダイアログも DOM に残るので open で絞る */
function openDialog() {
  return document.body.querySelector<HTMLDialogElement>('dialog[open]');
}

/** Esc（dialog の cancel）を直接シミュレート */
function pressEscape() {
  const dialog = openDialog();
  if (!dialog) throw new Error('open dialog not found');
  dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
}

function clickByAriaLabel(label: string) {
  const button = document.body.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  if (!button) throw new Error(`button[aria-label="${label}"] not found`);
  button.click();
}

/** ピッカーを開き、shortcode で検索して `emoji` のセルを選ぶ */
async function pickEmoji(query: string, emoji: string) {
  clickByAriaLabel('絵文字を選ぶ');
  await flushPromises();
  const search = document.body.querySelector<HTMLInputElement>('input[aria-label="絵文字を検索"]');
  if (!search) throw new Error('emoji search not found');
  await new DOMWrapper(search).setValue(query);
  await flushPromises();
  const cell = [...document.body.querySelectorAll<HTMLElement>('[role="gridcell"]')].find(
    (c) => c.textContent?.trim() === emoji,
  );
  if (!cell) throw new Error(`emoji cell ${emoji} not found`);
  cell.click();
  await flushPromises();
}

describe('LabelsSection', () => {
  beforeAll(async () => {
    await loadEmojiData();
  });

  beforeEach(() => {
    createMutateAsync.mockReset();
    updateMutateAsync.mockReset();
    deleteMutateAsync.mockReset();
    queryState.labels = [];
    queryState.isPending = false;
    queryState.isError = false;
    mutationPending.post.value = false;
    mutationPending.put.value = false;
    mutationPending.delete.value = false;
    document.body.innerHTML = '';
  });

  it('ラベルを行形式で一覧表示する', async () => {
    queryState.labels = [bugLabel, enhancementLabel];
    mountView();
    await flushPromises();

    expect(document.body.textContent).toContain('bug');
    expect(document.body.textContent).toContain('不具合の報告');
    expect(document.body.textContent).toContain('enhancement');
    expect(document.body.textContent).toContain('機能改善の提案');
    expect(document.body.querySelector('button[aria-label="ラベル「bug」を編集"]')).not.toBeNull();
    expect(document.body.querySelector('button[aria-label="ラベル「bug」を削除"]')).not.toBeNull();
  });

  it('ラベルが 0 件なら空状態を表示する', async () => {
    mountView();
    await flushPromises();

    expect(document.body.textContent).toContain('ラベルはまだありません');
  });

  it('作成ダイアログから POST を送り、成功でダイアログを閉じる', async () => {
    createMutateAsync.mockResolvedValue({ ...bugLabel, name: 'design' });
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();
    expect(document.body.textContent).toContain('ラベルを作成');

    await input('label-name').setValue('design');
    await input('label-description').setValue('デザイン関連');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID } },
      body: { name: 'design', color: '#ef4444', description: 'デザイン関連' },
    });
    expect(document.body.textContent).not.toContain('ラベルを作成');
  });

  it('編集ダイアログは既存値をプリフィルし、保存で PUT を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...bugLabel, name: 'defect' });
    queryState.labels = [bugLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「bug」を編集');
    await flushPromises();

    expect((input('label-name').element as HTMLInputElement).value).toBe('bug');
    expect((input('label-color').element as HTMLInputElement).value).toBe('#ef4444');

    await input('label-name').setValue('defect');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: bugLabel.id },
      },
      body: { name: 'defect', color: '#ef4444', description: '不具合の報告' },
    });
  });

  it('名前が空・色が不正なら送信せずエラーを表示する', async () => {
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();

    await input('label-color').setValue('red');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('名前は 1〜100 文字で入力してください');
    expect(document.body.textContent).toContain('色は #RRGGBB 形式で入力してください');
  });

  it('名前 100 文字は送信でき、101 文字は送信せずエラーを表示する', async () => {
    createMutateAsync.mockResolvedValue({ ...bugLabel, name: 'a'.repeat(100) });
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();

    await input('label-name').setValue('a'.repeat(101));
    await formEl().trigger('submit');
    await flushPromises();
    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('名前は 1〜100 文字で入力してください');

    // 100 文字ちょうどは成功する（絵文字 = UTF-16 では 200 単位でも通す）
    await input('label-name').setValue('😀'.repeat(100));
    await formEl().trigger('submit');
    await flushPromises();
    expect(createMutateAsync).toHaveBeenCalledTimes(1);
    expect(createMutateAsync.mock.calls[0][0].body.name).toBe('😀'.repeat(100));
  });

  it('保存リクエスト進行中はフォームダイアログのクローズ要求を無視する', async () => {
    mutationPending.post.value = true;
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();
    expect(openDialog()?.textContent).toContain('ラベルを作成');

    pressEscape();
    await flushPromises();

    expect(openDialog()?.textContent).toContain('ラベルを作成');
  });

  it('削除リクエスト進行中は確認ダイアログのクローズ要求を無視する', async () => {
    mutationPending.delete.value = true;
    queryState.labels = [bugLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「bug」を削除');
    await flushPromises();
    expect(openDialog()?.textContent).toContain('ラベルを削除しますか？');

    pressEscape();
    await flushPromises();

    expect(openDialog()?.textContent).toContain('ラベルを削除しますか？');
  });

  it('行の削除ボタン → 確認ダイアログで DELETE を送る', async () => {
    deleteMutateAsync.mockResolvedValue(undefined);
    queryState.labels = [bugLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「bug」を削除');
    await flushPromises();

    expect(document.body.textContent).toContain('「bug」を削除します。この操作は取り消せません。');

    clickBodyButton('削除する');
    await flushPromises();

    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: bugLabel.id },
      },
    });
    expect(openDialog()).toBeNull();
  });

  it('保存失敗時はダイアログ内にエラーを表示する', async () => {
    createMutateAsync.mockRejectedValue(new Error('forbidden'));
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();
    await input('label-name').setValue('design');
    await formEl().trigger('submit');
    await flushPromises();

    expect(document.body.textContent).toContain('ラベルを保存できませんでした');
  });

  it('削除失敗時は確認ダイアログ内にエラーを表示する', async () => {
    deleteMutateAsync.mockRejectedValue(new Error('forbidden'));
    queryState.labels = [bugLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「bug」を削除');
    await flushPromises();
    clickBodyButton('削除する');
    await flushPromises();

    expect(document.body.textContent).toContain('ラベルを削除できませんでした');
    expect(document.body.textContent).toContain('ラベルを削除しますか？');
  });

  /**
   * projectId が変わったらクエリキーも切り替わる。
   *
   * vike-vue はサイドバーからのプロジェクト切り替えでこのコンポーネントを作り直さない
   * ので、setup 時の props で options を組むと一覧だけが前のプロジェクトのまま残る。
   * ここでは useQuery ごとモックしているため、渡している options が props に追従して
   * 再評価されることを見る。
   */
  it('projectId が変わったらクエリキーが切り替わる', async () => {
    const OTHER_PROJECT_UUID = '00000000-0000-4000-8000-000000000020';
    queryState.labels = [bugLabel];
    const wrapper = mountView();
    await flushPromises();

    const optionsRef = vi.mocked(useQuery).mock.calls.at(-1)?.[0] as {
      value: { queryKey: readonly unknown[] };
    };
    expect(JSON.stringify(optionsRef.value.queryKey)).toContain(PROJECT_UUID);

    await wrapper.setProps({ projectId: OTHER_PROJECT_UUID });
    await flushPromises();

    expect(JSON.stringify(optionsRef.value.queryKey)).toContain(OTHER_PROJECT_UUID);
  });

  it('絵文字を持つラベルの行は絵文字を出し、色の丸を出さない', async () => {
    queryState.labels = [designLabel, bugLabel];
    mountView();
    await flushPromises();

    const rows = [...document.body.querySelectorAll('li')];
    const designRow = rows.find((li) => li.textContent?.includes('デザイン'));
    const bugRow = rows.find((li) => li.textContent?.includes('bug'));
    expect(designRow?.textContent).toContain('🎨');
    expect(designRow?.querySelector('.rounded-full[style*="background-color"]')).toBeNull();
    // 対照: 絵文字を持たないラベルは色の丸
    expect(bugRow?.querySelector('.rounded-full[style*="background-color"]')).not.toBeNull();
  });

  it('作成で絵文字を選ぶと色の欄が消え、POST に icon_emoji が入る', async () => {
    createMutateAsync.mockResolvedValue({ ...bugLabel, name: 'design', icon_emoji: '🎨' });
    mountView();
    await flushPromises();

    clickBodyButton('新しいラベル');
    await flushPromises();
    expect(document.body.querySelector('#label-color')).not.toBeNull();

    await pickEmoji('art', '🎨');
    expect(document.body.querySelector('button[aria-label="絵文字 🎨 を変更"]')).not.toBeNull();
    expect(document.body.querySelector('#label-color')).toBeNull();

    await input('label-name').setValue('design');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID } },
      body: { name: 'design', color: '#ef4444', description: '', icon_emoji: '🎨' },
    });
  });

  it('編集で既存の絵文字を出し、「絵文字なし」で色の欄が戻り PUT に clear_icon_emoji が入る', async () => {
    updateMutateAsync.mockResolvedValue({ ...designLabel, icon_emoji: null });
    queryState.labels = [designLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「デザイン」を編集');
    await flushPromises();
    expect(document.body.querySelector('button[aria-label="絵文字 🎨 を変更"]')).not.toBeNull();
    expect(document.body.querySelector('#label-color')).toBeNull();

    clickBodyButton('絵文字なし');
    await flushPromises();
    expect((input('label-color').element as HTMLInputElement).value).toBe('#8b5cf6');
    expect(document.body.querySelector('button[aria-label="絵文字を選ぶ"]')).not.toBeNull();

    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: designLabel.id },
      },
      body: { name: 'デザイン', color: '#8b5cf6', description: '', clear_icon_emoji: true },
    });
  });

  it('編集で絵文字を替えると PUT に新しい icon_emoji が入る', async () => {
    updateMutateAsync.mockResolvedValue(designLabel);
    queryState.labels = [bugLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「bug」を編集');
    await flushPromises();
    await pickEmoji('beetle', '🐞');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync.mock.calls[0][0].body).toEqual({
      name: 'bug',
      color: '#ef4444',
      description: '不具合の報告',
      icon_emoji: '🐞',
    });
  });

  it('絵文字に触れない編集は icon_emoji も clear_icon_emoji も送らない', async () => {
    updateMutateAsync.mockResolvedValue(designLabel);
    queryState.labels = [designLabel];
    mountView();
    await flushPromises();

    clickByAriaLabel('ラベル「デザイン」を編集');
    await flushPromises();
    await input('label-name').setValue('design');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync.mock.calls[0][0].body).toEqual({
      name: 'design',
      color: '#8b5cf6',
      description: '',
    });
  });
});
