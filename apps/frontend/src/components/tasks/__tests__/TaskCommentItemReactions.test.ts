import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';

import ReactionBar from '@/components/reactions/ReactionBar.vue';
import ReactionPeoplePanel from '@/components/reactions/ReactionPeoplePanel.vue';
import type { ReactionController } from '@/composables/useReactions';
import type { CommentThread } from '@/composables/useTaskComments';
import type { ReactionSummary } from '@/lib/reactions';

import TaskCommentItem from '../TaskCommentItem.vue';
import CommentEditorStub from './comment-editor-stub';

const user = { id: 'u-1', name: '田中', avatar_url: null };
const THUMBS: ReactionSummary = { emoji: '👍', count: 2, reacted: false, users: [user] };

function controller(overrides: Partial<ReactionController> = {}): ReactionController {
  return {
    isBusy: () => false,
    errorFor: () => null,
    toggle: vi.fn(async () => undefined),
    fetchUsers: vi.fn(async () => [user]),
    ...overrides,
  };
}

function comment(overrides: Partial<CommentThread> = {}): CommentThread {
  return {
    id: 'c-1',
    body: '本文',
    is_deleted: false,
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
    user,
    replies: [],
    reactions: [],
    ...overrides,
  };
}

function mountItem(
  props: { comment?: CommentThread; reactionController?: ReactionController | undefined } = {},
) {
  return mount(TaskCommentItem, {
    attachTo: document.body,
    global: { stubs: { CommentEditor: CommentEditorStub } },
    props: {
      comment: comment(),
      reactionController: controller(),
      onUpdate: async () => true,
      onDelete: async () => true,
      ...props,
    },
  });
}

const addSelector = '[aria-label="リアクションを追加"]';

function menuLabels(wrapper: ReturnType<typeof mountItem>) {
  return wrapper.findAll('[role="menuitem"]').map((item) => item.text());
}

describe('TaskCommentItem のリアクション', () => {
  it('札が無ければ ⋯ の隣に追加ボタンを出し、札の行は出さない', () => {
    const wrapper = mountItem();
    expect(wrapper.find('[data-reaction-chip]').exists()).toBe(false);
    expect(wrapper.find('.absolute').find(addSelector).exists()).toBe(true);
    expect(menuLabels(wrapper)).not.toContain('リアクションを見る');
  });

  it('札があれば本文の下に並べ、追加ボタンは行の末尾へ移り、メニューに「リアクションを見る」が出る', () => {
    const wrapper = mountItem({ comment: comment({ reactions: [THUMBS] }) });
    expect(wrapper.find('[data-reaction-chip]').exists()).toBe(true);
    expect(wrapper.find('.absolute').find(addSelector).exists()).toBe(false);
    expect(wrapper.findAll(addSelector)).toHaveLength(1);
    expect(menuLabels(wrapper)).toContain('リアクションを見る');
  });

  it('札を押すとそのコメントの id で切り替える', async () => {
    const reactionController = controller();
    const wrapper = mountItem({ comment: comment({ reactions: [THUMBS] }), reactionController });
    await wrapper.get('[data-reaction-chip]').trigger('click');
    expect(reactionController.toggle).toHaveBeenCalledWith('c-1', '👍');
  });

  it('メニューの「リアクションを見る」で全員の一覧を開く', async () => {
    const wrapper = mountItem({ comment: comment({ reactions: [THUMBS] }) });
    const item = wrapper
      .findAll('[role="menuitem"]')
      .find((menuItem) => menuItem.text() === 'リアクションを見る');
    await item?.trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(ReactionPeoplePanel).exists()).toBe(true);
  });

  it('削除済みコメントには札も追加ボタンも出さない', () => {
    const wrapper = mountItem({ comment: comment({ is_deleted: true, body: null }) });
    expect(wrapper.find(addSelector).exists()).toBe(false);
    expect(wrapper.findComponent(ReactionBar).exists()).toBe(false);
  });

  it('編集中は札も追加ボタンも出さない', async () => {
    const wrapper = mountItem({
      comment: comment({ reactions: [THUMBS], user }),
    });
    await wrapper.setProps({ currentUserId: user.id });
    const edit = wrapper
      .findAll('[role="menuitem"]')
      .find((item) => item.text() === 'コメントを編集');
    await edit?.trigger('click');
    expect(wrapper.find('[data-reaction-chip]').exists()).toBe(false);
    expect(wrapper.find(addSelector).exists()).toBe(false);
  });

  it('失敗の文言をそのコメントの札の下に出す', () => {
    const wrapper = mountItem({
      reactionController: controller({ errorFor: () => 'これ以上種類を増やせません' }),
    });
    expect(wrapper.get('[role="alert"]').text()).toBe('これ以上種類を増やせません');
  });

  it('リアクションの操作を渡さなければ何も出さない', () => {
    const wrapper = mountItem({ reactionController: undefined });
    expect(wrapper.find(addSelector).exists()).toBe(false);
  });

  it('送信中は追加ボタンから送らない（ボタンは aria-disabled）', () => {
    const busy = ref(true);
    const wrapper = mountItem({
      reactionController: controller({ isBusy: () => busy.value }),
    });
    expect(wrapper.get(addSelector).attributes('aria-disabled')).toBe('true');
  });
});
