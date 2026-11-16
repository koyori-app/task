import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import type { CommentThread } from '@/composables/useTaskComments';

import TaskCommentItem from '../TaskCommentItem.vue';
import CommentEditorStub from './comment-editor-stub';

const author = { id: 'u-author', name: '田中', avatar_url: null };

function comment(overrides: Partial<CommentThread> = {}): CommentThread {
  return {
    id: 'c-1',
    body: '本文',
    is_deleted: false,
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
    user: author,
    replies: [],
    reactions: [],
    ...overrides,
  };
}

function mountItem(props: { currentUserId?: string | null; isTenantOwner?: boolean } = {}) {
  return mount(TaskCommentItem, {
    attachTo: document.body,
    global: { stubs: { CommentEditor: CommentEditorStub } },
    props: {
      comment: comment(),
      onUpdate: async () => true,
      onDelete: async () => true,
      ...props,
    },
  });
}

function menuLabels(wrapper: ReturnType<typeof mountItem>) {
  // Dropdown は項目の外に「項目がありません」の行を置くので、操作の行だけを数える
  return wrapper
    .findAll('[role="menuitem"]')
    .map((item) => item.text())
    .filter((label) => label !== '項目がありません');
}

/**
 * 編集は投稿者だけ、削除は投稿者とテナントオーナーだけを backend が通す。
 * 通らない人にはメニューの行を出さない（押してから拒否される形にしない）。
 */
describe('TaskCommentItem の操作の権限', () => {
  it('他人のコメントは、オーナーでなければ編集も削除も出さず、⋯ 自体も出さない', () => {
    const wrapper = mountItem({ currentUserId: 'u-other' });
    expect(menuLabels(wrapper)).toEqual([]);
    expect(wrapper.find('[aria-label="コメントの操作"]').exists()).toBe(false);
  });

  it('自分のコメントには、編集と削除を出す', () => {
    const wrapper = mountItem({ currentUserId: author.id });
    expect(menuLabels(wrapper)).toEqual(['コメントを編集', 'コメントを削除']);
  });

  it('他人のコメントでも、テナントオーナーには削除だけを出す', () => {
    const wrapper = mountItem({ currentUserId: 'u-other', isTenantOwner: true });
    expect(menuLabels(wrapper)).toEqual(['コメントを削除']);
  });

  it('自分のコメントのテナントオーナーには、編集と削除を出す', () => {
    const wrapper = mountItem({ currentUserId: author.id, isTenantOwner: true });
    expect(menuLabels(wrapper)).toEqual(['コメントを編集', 'コメントを削除']);
  });

  it('ログイン中の利用者が分からない間は、自分のコメントにも出さない', () => {
    const wrapper = mountItem({ currentUserId: null });
    expect(menuLabels(wrapper)).toEqual([]);
    expect(wrapper.find('[aria-label="コメントの操作"]').exists()).toBe(false);
  });
});
