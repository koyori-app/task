import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { nextTick } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import TaskComments from '../TaskComments.vue';
import { useAuthStore } from '@/stores/auth';
import { resetCommentDrafts } from '@/composables/useCommentDrafts';
import { openMentionPickerSpy } from './comment-editor-stub';
import type { CommentThread } from '@/composables/useTaskComments';
import type { ActivityItem } from '@/lib/task-activity';

// 実物の入力欄は CodeMirror で happy-dom では起動できない。配線だけを見る代役に替える
// （札・候補・IME の実物の挙動は stories/components/CommentEditor.stories.ts が実ブラウザで見る）
vi.mock('@/components/tasks/CommentEditor.vue', async () => ({
  default: (await import('./comment-editor-stub')).default,
}));

enableAutoUnmount(afterEach);
let pinia: ReturnType<typeof createPinia>;
beforeEach(() => {
  resetCommentDrafts();
  pinia = createPinia();
  setActivePinia(pinia);
  signInAs('user-1');
});

function signInAs(id: string) {
  useAuthStore().setUser({
    id,
    email: `${id}@example.com`,
    username: id,
    email_verified: true,
    is_admin: false,
    is_suspended: false,
    totp_enabled: false,
  });
}

const user = { id: 'user-1', name: '田中太郎' };

function thread(id: string, body: string | null, overrides: Partial<CommentThread> = {}) {
  return {
    id,
    body,
    is_deleted: false,
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
    user,
    replies: [],
    reactions: [],
    ...overrides,
  } satisfies CommentThread;
}

/** コメントの操作はメニューの中にある。パネルは閉じていても DOM に残る */
function menuItems(wrapper: VueWrapper, label: string) {
  return wrapper.findAll('[role="menuitem"]').filter((item) => item.text() === label);
}

function activity(id: string, created_at: string, overrides: Partial<ActivityItem> = {}) {
  return {
    id,
    created_at,
    event_type: 'status_changed',
    payload: { from: 'Todo', to: 'Done' },
    user,
    ...overrides,
  } as ActivityItem;
}

function mountComments(props: Partial<InstanceType<typeof TaskComments>['$props']> = {}) {
  return mount(TaskComments, {
    global: { plugins: [pinia] },
    props: {
      threads: [],
      onSubmit: vi.fn(async () => true),
      onUpdate: vi.fn(async () => true),
      onDelete: vi.fn(async () => true),
      ...props,
    },
  });
}

describe('TaskComments の下書き（タスクごと）', () => {
  const box = (w: VueWrapper) => w.get('textarea[aria-label="コメントを入力"]');

  it('タスク A の下書きをタスク B の入力欄に出さず、A へ戻ると復元する', async () => {
    const wrapper = mountComments({ taskId: 'a' });
    await box(wrapper).setValue('A の下書き');
    await wrapper.setProps({ taskId: 'b' });
    expect((box(wrapper).element as HTMLTextAreaElement).value).toBe('');
    await wrapper.setProps({ taskId: 'a' });
    expect((box(wrapper).element as HTMLTextAreaElement).value).toBe('A の下書き');
  });

  it('別テナントの同じ表示用 ID（ENG-42）のタスクには下書きを出さない', async () => {
    const first = mountComments({ tenantId: 't-a', projectId: 'p-a', taskId: 'ENG-42' });
    await first.find('textarea').setValue('テナント A 向け');
    first.unmount();
    const other = mountComments({ tenantId: 't-b', projectId: 'p-b', taskId: 'ENG-42' });
    expect((other.find('textarea').element as HTMLTextAreaElement).value).toBe('');
    other.unmount();
    const back = mountComments({ tenantId: 't-a', projectId: 'p-a', taskId: 'ENG-42' });
    expect((back.find('textarea').element as HTMLTextAreaElement).value).toBe('テナント A 向け');
  });

  it('開き直し（再マウント）でも同じタスクの下書きが戻る', async () => {
    const first = mountComments({ taskId: 'a' });
    await box(first).setValue('書きかけ');
    first.unmount();
    const second = mountComments({ taskId: 'a' });
    expect((box(second).element as HTMLTextAreaElement).value).toBe('書きかけ');
  });

  it('利用者 A の下書きは同じタスクでも利用者 B の入力欄に復元しない', async () => {
    const first = mountComments({ taskId: 'a' });
    await box(first).setValue('A の書きかけ');
    first.unmount();
    signInAs('user-2');
    const second = mountComments({ taskId: 'a' });
    expect((box(second).element as HTMLTextAreaElement).value).toBe('');
    second.unmount();
    signInAs('user-1');
    const back = mountComments({ taskId: 'a' });
    expect((box(back).element as HTMLTextAreaElement).value).toBe('A の書きかけ');
  });

  it('clearUser（/me の一時障害でも呼ばれる）では下書きを消さない', async () => {
    const first = mountComments({ taskId: 'a' });
    await box(first).setValue('残る');
    first.unmount();
    useAuthStore().clearUser();
    signInAs('user-1');
    const second = mountComments({ taskId: 'a' });
    expect((box(second).element as HTMLTextAreaElement).value).toBe('残る');
  });

  it('投稿に成功したら下書きを消す（再マウントしても戻らない）', async () => {
    const wrapper = mountComments({ taskId: 'a' });
    await box(wrapper).setValue('送る');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    wrapper.unmount();
    const again = mountComments({ taskId: 'a' });
    expect((box(again).element as HTMLTextAreaElement).value).toBe('');
  });
});

describe('TaskComments', () => {
  it('スレッドと返信を素テキストで表示し、HTML はエスケープされたまま出す', async () => {
    const wrapper = mountComments({
      threads: [
        thread('c-1', '一行目\n<script>alert(1)</script>', {
          replies: [
            {
              id: 'r-1',
              body: '返信本文',
              is_deleted: false,
              created_at: '2026-08-19T01:00:00Z',
              updated_at: '2026-08-19T01:00:00Z',
              user,
              reactions: [],
            },
          ],
        }),
      ],
    });

    // 一覧は親コメントだけを出す（返信はスレッド表示で見る）
    const listBodies = wrapper.findAll('[data-task-comment] p.whitespace-pre-wrap');
    expect(listBodies).toHaveLength(1);
    // v-html ではないため、タグはテキストとしてそのまま残る
    expect(listBodies[0].text()).toContain('<script>alert(1)</script>');
    expect(wrapper.find('script').exists()).toBe(false);

    // 返信件数のボタンでスレッドへ切り替えると、親と返信が並ぶ
    const openThread = wrapper.findAll('button').find((b) => b.text() === '1件の返信');
    expect(openThread).toBeDefined();
    await openThread!.trigger('click');
    const threadBodies = wrapper.findAll('[data-task-comment] p.whitespace-pre-wrap');
    expect(threadBodies).toHaveLength(2);
    expect(threadBodies[1].text()).toBe('返信本文');
    expect(wrapper.find('script').exists()).toBe(false);
  });

  it('profileHref を渡したときだけ投稿者をプロフィールへのリンクにする', () => {
    const linked = mountComments({
      threads: [thread('c-1', '本文')],
      profileHref: (userId: string) => `/acme/users/${userId}`,
    });
    const author = linked.find('[data-comment-author]');
    expect(author.element.tagName).toBe('A');
    expect(author.attributes('href')).toBe('/acme/users/user-1');
    expect(author.text()).toContain('田中太郎');

    const plain = mountComments({ threads: [thread('c-1', '本文')] });
    expect(plain.find('[data-comment-author]').element.tagName).toBe('SPAN');
    expect(plain.find('[data-comment-author]').attributes('href')).toBeUndefined();
  });

  it('コメントが無いときは空メッセージを出す', () => {
    const wrapper = mountComments();
    expect(wrapper.text()).toContain('アクティビティはまだありません');
  });

  it('一覧の読み込み失敗はリスト位置でエラー表示しつつ、投稿フォームは出したままにする', async () => {
    const onRetry = vi.fn();
    const wrapper = mountComments({ listError: true, onRetry });
    expect(wrapper.text()).toContain('コメントを読み込めませんでした');
    // 一覧の GET が失敗しても POST は独立に成功しうるため、書く導線は残す
    expect(wrapper.find('form').exists()).toBe(true);

    const retryButton = wrapper.findAll('button').find((button) => button.text() === '再試行');
    expect(retryButton).toBeDefined();
    await retryButton!.trigger('click');
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('投稿は onSubmit を呼び、成功したら下書きを消す', async () => {
    const onSubmit = vi.fn(async () => true);
    const wrapper = mountComments({ onSubmit });

    const textarea = wrapper.get('textarea[aria-label="コメントを入力"]');
    await textarea.setValue('新しいコメント');
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(onSubmit).toHaveBeenCalledWith('新しいコメント');
    expect((textarea.element as HTMLTextAreaElement).value).toBe('');
  });

  it('投稿が失敗（拒否）したら下書きを残し、submitError を表示する', async () => {
    const onSubmit = vi.fn(async () => false);
    const wrapper = mountComments({ onSubmit });

    const textarea = wrapper.get('textarea[aria-label="コメントを入力"]');
    await textarea.setValue('拒否されるコメント');
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect((textarea.element as HTMLTextAreaElement).value).toBe('拒否されるコメント');

    await wrapper.setProps({ submitError: 'コメントを投稿できませんでした（forbidden）' });
    expect(wrapper.text()).toContain('コメントを投稿できませんでした（forbidden）');
  });

  it('返信フォームは parent としてスレッド ID を渡す', async () => {
    const onSubmit = vi.fn(async () => true);
    const wrapper = mountComments({ threads: [thread('c-1', '親コメント')], onSubmit });

    // 返信は列をスレッド表示へ切り替えてから、下端の入力欄で送る
    const replyOpenButton = wrapper.findAll('button').find((button) => button.text() === '返信');
    expect(replyOpenButton).toBeDefined();
    await replyOpenButton!.trigger('click');
    const replyArea = wrapper.get('textarea[aria-label="返信を入力"]');
    await replyArea.setValue('返信します');
    // 送信ボタンは type=submit なので、フォームの submit で確定させる
    expect(wrapper.find('button[aria-label="返信する"]').exists()).toBe(true);
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(onSubmit).toHaveBeenCalledWith('返信します', 'c-1');
    // 成功で下書きは消える（スレッド表示は続けて返信できるよう開いたまま）
    expect(
      wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]').element.value,
    ).toBe('');
  });

  // 送信中も「コメント一覧へ戻る」は押せる。await の後に openThreadId を読み直すと、
  // 消す先が返信ではなく一覧の下書きになり、利用者の入力が消える
  it('返信の送信中に一覧へ戻っても、消えるのは返信の下書きだけ', async () => {
    let resolveSubmit: ((posted: boolean) => void) | undefined;
    const onSubmit = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveSubmit = resolve;
        }),
    );
    const wrapper = mountComments({ threads: [thread('c-1', '親コメント')], onSubmit });

    // 先に一覧側の下書きを書いておく
    await wrapper.get('textarea[aria-label="コメントを入力"]').setValue('一覧の書きかけ');

    // スレッドを開いて返信を送る（まだ解決させない）
    const replyOpenButton = wrapper.findAll('button').find((button) => button.text() === '返信');
    await replyOpenButton!.trigger('click');
    await wrapper.get('textarea[aria-label="返信を入力"]').setValue('返信します');
    await wrapper.get('form').trigger('submit');

    // 送信中に一覧へ戻る
    await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
    resolveSubmit!(true);
    await flushPromises();

    // 一覧の下書きは残る
    expect(
      wrapper.get<HTMLTextAreaElement>('textarea[aria-label="コメントを入力"]').element.value,
    ).toBe('一覧の書きかけ');

    // 送信済みの返信の下書きは消えている
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '返信')!
      .trigger('click');
    expect(
      wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]').element.value,
    ).toBe('');
  });

  // 同じスレッドへ戻って書き直した下書きも、送信済みの本文ではないので残す
  it('送信中に離れて同じスレッドへ戻り書き直したら、その下書きは消さない', async () => {
    let resolveSubmit: ((posted: boolean) => void) | undefined;
    const onSubmit = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveSubmit = resolve;
        }),
    );
    const wrapper = mountComments({ threads: [thread('c-1', '親コメント')], onSubmit });

    const openThread = async () =>
      wrapper
        .findAll('button')
        .find((button) => button.text() === '返信')!
        .trigger('click');

    await openThread();
    await wrapper.get('textarea[aria-label="返信を入力"]').setValue('返信します');
    await wrapper.get('form').trigger('submit');

    // 一覧へ戻り、同じスレッドを開き直して別の下書きを書く
    await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
    await openThread();
    await wrapper.get('textarea[aria-label="返信を入力"]').setValue('書き直した返信');

    resolveSubmit!(true);
    await flushPromises();

    expect(
      wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]').element.value,
    ).toBe('書き直した返信');
  });

  it('編集は onUpdate をコメント ID と新本文で呼ぶ', async () => {
    const onUpdate = vi.fn(async () => true);
    const wrapper = mountComments({
      threads: [thread('c-1', '元の本文')],
      currentUserId: user.id,
      onUpdate,
    });

    await menuItems(wrapper, 'コメントを編集')[0]!.trigger('click');
    const editArea = wrapper.get('textarea[aria-label="コメントを編集"]');
    expect((editArea.element as HTMLTextAreaElement).value).toBe('元の本文');
    await editArea.setValue('直した本文');
    const saveButton = wrapper.findAll('button').find((button) => button.text() === '保存');
    await saveButton!.trigger('click');
    await flushPromises();

    expect(onUpdate).toHaveBeenCalledWith('c-1', '直した本文');
    // 成功で編集 UI は閉じ、表示に戻る
    expect(wrapper.find('textarea[aria-label="コメントを編集"]').exists()).toBe(false);
  });

  it('削除は確認を挟んでから onDelete を呼ぶ', async () => {
    const onDelete = vi.fn(async () => true);
    const wrapper = mountComments({ threads: [thread('c-1', '消すコメント')], onDelete });

    await menuItems(wrapper, 'コメントを削除')[0]!.trigger('click');
    expect(onDelete).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('このコメントを削除しますか？');

    const confirmButton = wrapper.findAll('button').find((button) => button.text() === '削除する');
    await confirmButton!.trigger('click');
    await flushPromises();

    expect(onDelete).toHaveBeenCalledTimes(1);
    // 成功（backend が許可）したら確認 UI を閉じる
    expect(wrapper.text()).not.toContain('このコメントを削除しますか？');
  });

  it('削除が backend に拒否されたら確認 UI を残し、拒否理由を対象コメントの中に出す', async () => {
    const onDelete = vi.fn(async () => false);
    const wrapper = mountComments({ threads: [thread('c-1', '消せないコメント')], onDelete });

    await menuItems(wrapper, 'コメントを削除')[0]!.trigger('click');
    const confirmButton = wrapper.findAll('button').find((button) => button.text() === '削除する');
    await confirmButton!.trigger('click');
    await flushPromises();

    expect(onDelete).toHaveBeenCalledTimes(1);
    // 失敗時は確認 UI を残す（成功系の「閉じる」との対照）
    expect(wrapper.text()).toContain('このコメントを削除しますか？');

    await wrapper.setProps({
      deleteError: 'コメントを削除できませんでした（forbidden）',
      deleteErrorCommentId: 'c-1',
    });
    expect(wrapper.get('[data-task-comment]').text()).toContain(
      'コメントを削除できませんでした（forbidden）',
    );
  });

  it('スレッドを開いたまま削除に成功したら一覧へ戻す', async () => {
    const onDelete = vi.fn(async () => true);
    const wrapper = mountComments({ threads: [thread('c-1', '消すスレッド')], onDelete });

    const replyOpenButton = wrapper.findAll('button').find((button) => button.text() === '返信');
    await replyOpenButton!.trigger('click');
    expect(wrapper.find('button[aria-label="コメント一覧へ戻る"]').exists()).toBe(true);

    await menuItems(wrapper, 'コメントを削除')[0]!.trigger('click');
    const confirmButton = wrapper.findAll('button').find((button) => button.text() === '削除する');
    await confirmButton!.trigger('click');
    await flushPromises();

    // 削除済みスレッドへは返信できない（backend が 400 で弾く）ため、一覧へ戻す
    expect(wrapper.find('button[aria-label="コメント一覧へ戻る"]').exists()).toBe(false);
  });

  it('削除済みコメントはプレースホルダを出し、編集・削除ボタンを出さない', () => {
    const wrapper = mountComments({
      threads: [thread('c-1', null, { is_deleted: true })],
    });

    expect(wrapper.text()).toContain('削除されたコメント');
    expect(wrapper.text()).not.toContain('(編集済み)');
    expect(wrapper.find('[role="menuitem"]').exists()).toBe(false);
  });

  it('削除済みでも updated_at が異なっていても (編集済み) は出さない', () => {
    const wrapper = mountComments({
      threads: [
        thread('c-1', null, {
          is_deleted: true,
          updated_at: '2026-08-19T02:00:00Z',
        }),
      ],
    });
    expect(wrapper.text()).not.toContain('(編集済み)');
  });

  it('編集済みコメントには (編集済み) を出す', () => {
    const wrapper = mountComments({
      threads: [thread('c-1', '本文', { updated_at: '2026-08-19T02:00:00Z' })],
    });
    expect(wrapper.text()).toContain('(編集済み)');
  });

  it('削除済みスレッドには返信ボタンを出さない（backend が必ず 400 で弾く導線）', () => {
    const wrapper = mountComments({
      threads: [thread('c-1', null, { is_deleted: true }), thread('c-2', '生きているスレッド')],
    });

    // 返信ボタンは生きているスレッドの 1 つだけ
    const replyButtons = wrapper.findAll('button').filter((button) => button.text() === '返信');
    expect(replyButtons).toHaveLength(1);
  });

  it('編集ボタンは投稿者本人のコメントにだけ出す（backend が本人以外を必ず 403 にする）', () => {
    const other = { id: 'user-2', name: '佐藤花子' };
    const wrapper = mountComments({
      threads: [thread('c-1', '自分のコメント'), thread('c-2', '他人のコメント', { user: other })],
      currentUserId: user.id,
    });

    const editButtons = menuItems(wrapper, 'コメントを編集');
    expect(editButtons).toHaveLength(1);
    // 削除ボタンはテナントオーナーも許されるため全コメントに出る
    expect(menuItems(wrapper, 'コメントを削除')).toHaveLength(2);
  });

  it('返信の失敗はスレッド表示の中に出す', async () => {
    const wrapper = mountComments({
      threads: [thread('c-1', '親コメント')],
      onSubmit: vi.fn(async () => false),
    });

    const replyOpenButton = wrapper.findAll('button').find((button) => button.text() === '返信');
    await replyOpenButton!.trigger('click');
    await wrapper.setProps({
      replyError: '返信を投稿できませんでした（bad-request）',
      replyErrorThreadId: 'c-1',
    });

    // スレッド表示のまま、その場に拒否理由を出す
    expect(wrapper.find('textarea[aria-label="返信を入力"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('返信を投稿できませんでした（bad-request）');
  });

  it('スレッドの開閉で前回の失敗表示を消す', async () => {
    const onClearReplyError = vi.fn();
    const wrapper = mountComments({
      threads: [thread('c-1', '親コメント')],
      onClearReplyError,
      replyError: '返信を投稿できませんでした（bad-request）',
      replyErrorThreadId: 'c-1',
    });

    const replyOpenButton = wrapper.findAll('button').find((button) => button.text() === '返信');
    await replyOpenButton!.trigger('click');
    expect(onClearReplyError).toHaveBeenCalledTimes(1);

    // 一覧へ戻るときにも消す（次に開いたスレッドへ前の失敗を持ち越さない）
    await wrapper.setProps({ replyError: null, replyErrorThreadId: null });
    await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
    expect(onClearReplyError).toHaveBeenCalledTimes(2);
  });

  // 消費側はコメントと履歴を別の query で取っていて「片方が落ちてももう片方は読める」形。
  // 表示側で入れ子にすると、コメントの GET が失敗しただけで取れている履歴まで消える
  it('コメント一覧が失敗しても履歴は出し続ける', () => {
    const wrapper = mountComments({
      listError: true,
      onRetry: vi.fn(),
      activities: [activity('h1', '2026-08-19T00:00:00Z')],
    });

    expect(wrapper.text()).toContain('コメントを読み込めませんでした');
    expect(wrapper.find('[data-activity-history]').text()).toContain('Done');
  });

  it('履歴が失敗してもコメントと入力欄は出し続け、履歴だけ再試行できる', async () => {
    const onRetryActivities = vi.fn();
    const wrapper = mountComments({
      threads: [thread('c-1', 'コメント')],
      activitiesError: true,
      onRetryActivities,
    });

    expect(wrapper.text()).toContain('履歴を読み込めませんでした');
    expect(wrapper.find('[data-task-comment]').exists()).toBe(true);
    expect(wrapper.find('form').exists()).toBe(true);
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '再試行')!
      .trigger('click');
    expect(onRetryActivities).toHaveBeenCalledTimes(1);
  });

  it('コメント一覧の読み込み中でも履歴は出す', () => {
    const wrapper = mountComments({
      loading: true,
      activities: [activity('h1', '2026-08-19T00:00:00Z')],
    });

    expect(wrapper.find('[data-activity-history]').exists()).toBe(true);
    expect(wrapper.findAll('[data-activity-skeleton]')).toHaveLength(1);
  });

  it('両方の読み込み中は骨組みを出し、空のメッセージは出さない', () => {
    const wrapper = mountComments({ loading: true, activitiesLoading: true });

    expect(wrapper.findAll('[data-activity-skeleton]')).toHaveLength(3);
    expect(wrapper.text()).not.toContain('まだありません');
  });

  describe('タイムライン', () => {
    const entries = (wrapper: VueWrapper) =>
      wrapper
        .findAll('[data-activity-history], [data-task-comment]')
        .map((entry) => (entry.attributes('data-comment-id') ? 'comment' : 'history'));

    it('履歴とコメントを古い順に混ぜ、投稿の履歴を重ねない', () => {
      const wrapper = mountComments({
        threads: [thread('c-1', 'コメント', { created_at: '2026-08-19T01:00:00Z' })],
        activities: [
          activity('h2', '2026-08-19T02:00:00Z'),
          activity('added', '2026-08-19T01:00:00Z', { event_type: 'comment_added', payload: {} }),
          activity('h1', '2026-08-19T00:00:00Z'),
        ],
      });

      expect(entries(wrapper)).toEqual(['history', 'comment', 'history']);
      expect(wrapper.text()).not.toContain('コメントしました');
    });

    it('絞り込みに応じて出す物を変える', async () => {
      const wrapper = mountComments({
        threads: [thread('c-1', 'コメント', { created_at: '2026-08-19T01:00:00Z' })],
        activities: [activity('h1', '2026-08-19T00:00:00Z')],
        filter: 'comments',
      });
      expect(entries(wrapper)).toEqual(['comment']);

      await wrapper.setProps({ filter: 'history' });
      expect(entries(wrapper)).toEqual(['history']);
    });

    it('絞り込みの結果が空なら、その種類の空メッセージを出す', async () => {
      const wrapper = mountComments({ filter: 'comments' });
      expect(wrapper.text()).toContain('コメントはまだありません');

      await wrapper.setProps({ filter: 'history' });
      expect(wrapper.text()).toContain('履歴はありません');
    });

    it('絞り込みを変えたらスレッドから一覧へ戻す', async () => {
      const wrapper = mountComments({ threads: [thread('c-1', 'コメント')] });
      await wrapper
        .findAll('button')
        .find((button) => button.text() === '返信')!
        .trigger('click');
      expect(wrapper.find('button[aria-label="コメント一覧へ戻る"]').exists()).toBe(true);

      await wrapper.setProps({ filter: 'comments' });
      expect(wrapper.find('button[aria-label="コメント一覧へ戻る"]').exists()).toBe(false);
    });

    it('続きの履歴があるあいだは、上端に導線を出して古いコメントを隠す', async () => {
      const onLoadMoreActivities = vi.fn();
      const wrapper = mountComments({
        threads: [
          thread('old', '古いコメント', { created_at: '2026-08-18T00:00:00Z' }),
          thread('new', '新しいコメント', { created_at: '2026-08-20T00:00:00Z' }),
        ],
        activities: [activity('h1', '2026-08-19T00:00:00Z')],
        hasMoreActivities: true,
        onLoadMoreActivities,
      });
      expect(wrapper.text()).not.toContain('古いコメント');

      const button = wrapper
        .findAll('button')
        .find((b) => b.text() === '以前のアクティビティを見る')!;
      // 導線は最初のエントリーより前にある
      const first = wrapper.find('[data-activity-section]').element;
      expect(
        button.element.compareDocumentPosition(first) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      await button.trigger('click');
      expect(onLoadMoreActivities).toHaveBeenCalledTimes(1);

      await wrapper.setProps({ hasMoreActivities: false });
      expect(wrapper.text()).toContain('古いコメント');
      expect(wrapper.text()).not.toContain('以前のアクティビティを見る');
    });

    it('続きの取得中は導線を押せない', () => {
      const wrapper = mountComments({
        activities: [activity('h1', '2026-08-19T00:00:00Z')],
        hasMoreActivities: true,
        activitiesFetchingMore: true,
        onLoadMoreActivities: () => {},
      });
      const button = wrapper.findAll('button').find((b) => b.text() === '読み込み中…');
      expect(button!.attributes('disabled')).toBeDefined();
    });

    it('開いたまま日付が変わると見出しも進む', async () => {
      vi.useFakeTimers();
      try {
        vi.setSystemTime(new Date(2026, 7, 19, 23, 59));
        const wrapper = mountComments({
          activities: [activity('h1', new Date(2026, 7, 19, 10, 0).toISOString())],
        });
        expect(wrapper.find('[data-activity-section] h3').text()).toBe('今日');

        // useNow の間隔（30 秒）を越えて日付をまたぐ
        vi.advanceTimersByTime(120_000);
        await nextTick();
        expect(wrapper.find('[data-activity-section] h3').text()).toBe('昨日');
      } finally {
        vi.useRealTimers();
      }
    });

    describe('タスクの切り替えとスクロール位置', () => {
      // 単体テストの DOM にはレイアウトが無いので、溢れた列の寸法を与える
      function overflowingFeed(wrapper: VueWrapper) {
        const feed = wrapper.get('[data-activity-feed]');
        const el = feed.element as HTMLElement;
        Object.defineProperty(el, 'scrollHeight', { configurable: true, value: 1000 });
        Object.defineProperty(el, 'clientHeight', { configurable: true, value: 100 });
        return { feed, el };
      }

      it('上へ読み進めた後に別のタスクへ切り替えたら、最新の位置へ移る', async () => {
        const wrapper = mountComments({ taskId: 'a', threads: [thread('c-a', 'タスク A')] });
        const { feed, el } = overflowingFeed(wrapper);
        el.scrollTop = 0;
        await feed.trigger('scroll');

        await wrapper.setProps({ taskId: 'b', threads: [thread('c-b', 'タスク B')] });
        await nextTick();

        expect(el.scrollTop).toBe(1000);
      });

      it('以前の分を取っている途中で切り替えたら、前のタスクの位置を持ち越さない', async () => {
        const wrapper = mountComments({
          taskId: 'a',
          activities: [activity('h1', '2026-08-19T00:00:00Z')],
          hasMoreActivities: true,
          onLoadMoreActivities: () => {},
        });
        const { feed, el } = overflowingFeed(wrapper);
        el.scrollTop = 0;
        await feed.trigger('scroll');
        await wrapper
          .findAll('button')
          .find((b) => b.text() === '以前のアクティビティを見る')!
          .trigger('click');

        await wrapper.setProps({
          taskId: 'b',
          activities: [activity('h2', '2026-08-20T00:00:00Z')],
        });
        await nextTick();

        expect(el.scrollTop).toBe(1000);
      });

      it('同じタスクで増えただけなら、上へ読み進めている位置を動かさない', async () => {
        const wrapper = mountComments({ taskId: 'a', threads: [thread('c-1', '古いコメント')] });
        const { feed, el } = overflowingFeed(wrapper);
        el.scrollTop = 0;
        await feed.trigger('scroll');

        await wrapper.setProps({
          threads: [
            thread('c-1', '古いコメント'),
            thread('c-2', '新しいコメント', { created_at: '2026-08-20T00:00:00Z' }),
          ],
        });
        await nextTick();

        expect(el.scrollTop).toBe(0);
      });
    });

    it('上へ溢れた分へスクロールできるよう、justify-end で下寄せしない', () => {
      const wrapper = mountComments();
      expect(wrapper.get('[data-activity-feed]').classes()).not.toContain('justify-end');
      expect(wrapper.get('[data-activity-feed] > div').classes()).toContain('mt-auto');
    });
  });

  describe('入力欄', () => {
    const toolbar = (wrapper: VueWrapper) =>
      wrapper.get('button[aria-label="コメントする"]').element.parentElement as HTMLElement;

    it('ふだんは操作を隠し、フォーカスで出し、離れたら隠す', async () => {
      const wrapper = mountComments();
      expect(toolbar(wrapper).style.display).toBe('none');

      await wrapper.get('form').trigger('focusin');
      expect(toolbar(wrapper).style.display).not.toBe('none');

      await wrapper.get('form').trigger('focusout', { relatedTarget: null });
      expect(toolbar(wrapper).style.display).toBe('none');
    });

    it('下書きがあれば離れても操作を出したままにする', async () => {
      const wrapper = mountComments();
      await wrapper.get('textarea').setValue('書きかけ');
      await wrapper.get('form').trigger('focusout', { relatedTarget: null });
      expect(toolbar(wrapper).style.display).not.toBe('none');
    });

    it('押せない操作を置かない', () => {
      const wrapper = mountComments();
      expect(wrapper.find('button[aria-label="追加"]').exists()).toBe(false);
      expect(wrapper.find('button[aria-label="ファイルを添付"]').exists()).toBe(false);
    });
  });

  // 入力欄は一覧とスレッドで 1 つを共有するので、単一の下書きだと
  // 覗くだけ・戻るだけで書きかけが消える
  describe('下書きは文脈ごとに持つ', () => {
    const threads = [thread('c-1', '親1', { replies: [] }), thread('c-2', '親2', { replies: [] })];

    async function openThread(wrapper: ReturnType<typeof mountComments>, index: number) {
      const buttons = wrapper.findAll('button').filter((b) => b.text() === '返信');
      await buttons[index].trigger('click');
    }

    it('スレッドを覗いて戻っても一覧の下書きが残る', async () => {
      const wrapper = mountComments({ threads });
      const input = () => wrapper.get<HTMLTextAreaElement>('textarea[aria-label="コメントを入力"]');

      await input().setValue('書きかけの新規コメント');
      await openThread(wrapper, 0);
      expect(
        wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]').element.value,
      ).toBe('');

      await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
      expect(input().element.value).toBe('書きかけの新規コメント');
    });

    it('返信の下書きはスレッドごとに分かれる', async () => {
      const wrapper = mountComments({ threads });
      const reply = () => wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]');

      await openThread(wrapper, 0);
      await reply().setValue('c-1 への返信');
      await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');

      await openThread(wrapper, 1);
      // 別スレッドの下書きを引き継がない
      expect(reply().element.value).toBe('');
      await reply().setValue('c-2 への返信');
      await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');

      await openThread(wrapper, 0);
      expect(reply().element.value).toBe('c-1 への返信');
    });

    it('投稿に成功したら、その文脈の下書きだけ消える', async () => {
      const wrapper = mountComments({ threads, onSubmit: vi.fn(async () => true) });

      await wrapper.get('textarea[aria-label="コメントを入力"]').setValue('一覧の下書き');
      await openThread(wrapper, 0);
      await wrapper.get('textarea[aria-label="返信を入力"]').setValue('返信の下書き');
      await wrapper.get('form').trigger('submit');
      await flushPromises();

      expect(
        wrapper.get<HTMLTextAreaElement>('textarea[aria-label="返信を入力"]').element.value,
      ).toBe('');
      await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
      expect(
        wrapper.get<HTMLTextAreaElement>('textarea[aria-label="コメントを入力"]').element.value,
      ).toBe('一覧の下書き');
    });
  });

  // 入力欄は一覧とスレッドで共有なので、新規投稿の失敗をそのまま出すと
  // スレッドを開いた瞬間「まだ送っていない返信が失敗した」ように見える
  it('新規投稿の失敗を返信の入力欄へ持ち越さない', async () => {
    const threads = [thread('c-1', '親1')];
    const wrapper = mountComments({
      threads,
      submitError: 'コメントを投稿できませんでした（403）',
    });

    expect(wrapper.text()).toContain('コメントを投稿できませんでした（403）');

    // スレッドを開くと、その失敗は出さない
    const reply = wrapper.findAll('button').find((b) => b.text() === '返信');
    await reply!.trigger('click');
    expect(wrapper.text()).not.toContain('コメントを投稿できませんでした（403）');

    // 一覧へ戻ると、下書きと同じでまた読める（消してはいない）
    await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
    expect(wrapper.text()).toContain('コメントを投稿できませんでした（403）');
  });

  it('返信の失敗はそのスレッドの中だけに出す', async () => {
    const threads = [thread('c-1', '親1'), thread('c-2', '親2')];
    const wrapper = mountComments({
      threads,
      replyError: '返信を投稿できませんでした',
      replyErrorThreadId: 'c-1',
    });

    // 一覧では出さない
    expect(wrapper.text()).not.toContain('返信を投稿できませんでした');

    const replies = wrapper.findAll('button').filter((b) => b.text() === '返信');
    await replies[1].trigger('click');
    expect(wrapper.text()).not.toContain('返信を投稿できませんでした');

    await wrapper.get('button[aria-label="コメント一覧へ戻る"]').trigger('click');
    await replies[0].trigger('click');
    expect(wrapper.text()).toContain('返信を投稿できませんでした');
  });

  it('更新・削除の失敗は対象コメントの中に出す', async () => {
    const other = { id: 'user-2', name: '佐藤花子' };
    const wrapper = mountComments({
      threads: [thread('c-1', '対象'), thread('c-2', '無関係', { user: other })],
      updateError: 'コメントを更新できませんでした（forbidden）',
      updateErrorCommentId: 'c-1',
    });

    const items = wrapper.findAll('[data-task-comment]');
    expect(items[0].text()).toContain('コメントを更新できませんでした（forbidden）');
    expect(items[1].text()).not.toContain('コメントを更新できませんでした');
  });
});

describe('TaskComments のメンション', () => {
  const ALICE = '11111111-1111-4111-8111-111111111111';

  it('本文の <@id> を、対応表の名の札に描く（一覧でも返信でも id は出さない）', async () => {
    const wrapper = mountComments({
      threads: [
        thread('c-1', `<@${ALICE}> さん確認を`, {
          replies: [
            {
              id: 'r-1',
              body: `<@${ALICE}> 返信`,
              is_deleted: false,
              created_at: '2026-08-19T01:00:00Z',
              updated_at: '2026-08-19T01:00:00Z',
              user,
              reactions: [],
            },
          ],
        }),
      ],
      mentions: { [ALICE]: { status: 'member', name: 'alice' } },
    });
    await vi.waitFor(
      () => expect(wrapper.find('[data-task-comment] .kfm-mention').exists()).toBe(true),
      { timeout: 10_000 },
    );
    expect(wrapper.get('[data-task-comment] .kfm-mention').text()).toBe('@alice');
    expect(wrapper.text()).not.toContain(ALICE);

    await wrapper
      .findAll('button')
      .find((b) => b.text() === '1件の返信')!
      .trigger('click');
    await vi.waitFor(
      () => expect(wrapper.findAll('[data-task-comment] .kfm-mention')).toHaveLength(2),
      { timeout: 10_000 },
    );
    const chips = wrapper.findAll('[data-task-comment] .kfm-mention');
    expect(chips).toHaveLength(2);
    expect(chips.every((c) => c.text() === '@alice')).toBe(true);
  }, 30_000);

  it('既存の素の @username は札にせず今のまま描く', () => {
    const wrapper = mountComments({ threads: [thread('c-1', '@alice 旧い形')] });
    expect(wrapper.find('[data-mention]').exists()).toBe(false);
    expect(wrapper.get('[data-task-comment] p.whitespace-pre-wrap').text()).toBe('@alice 旧い形');
  });

  it('メンションのボタンは入力欄の候補を開き、候補を入力欄へ渡す', async () => {
    const wrapper = mountComments({
      mentionCandidates: [
        { id: ALICE, username: 'alice' },
        { id: 'u-2', username: 'bob' },
      ],
    });
    expect(wrapper.get('textarea[aria-label="コメントを入力"]').attributes('data-candidates')).toBe(
      '2',
    );
    const before = openMentionPickerSpy.calls;
    const button = wrapper.get('button[aria-label="メンション"]');
    expect(button.attributes('disabled')).toBeUndefined();
    await button.trigger('click');
    expect(openMentionPickerSpy.calls).toBe(before + 1);
  });

  it('送信中はメンションのボタンも押せない', () => {
    const wrapper = mountComments({ submitPending: true });
    expect(wrapper.get('button[aria-label="メンション"]').attributes('disabled')).toBeDefined();
  });
});
