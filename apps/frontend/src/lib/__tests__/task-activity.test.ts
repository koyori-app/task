import { describe, expect, it } from 'vitest';

import { activityText, type ActivityItem } from '@/lib/task-activity';

function text(event_type: string, payload: unknown) {
  return activityText({ id: 'x', created_at: '', event_type, payload } as ActivityItem);
}

// ダッシュボードの「最近のアクティビティ」が文のまま使う
describe('activityText', () => {
  it.each([
    [
      'relation_added',
      { type: 'blocks', target_seq_id: 55 },
      'タスク #55 をブロックする関係を追加しました',
    ],
    [
      'relation_added',
      { type: 'blocked_by', target_seq_id: 55 },
      'タスク #55 にブロックされる関係を追加しました',
    ],
    [
      'relation_removed',
      { type: 'blocks', target_seq_id: 55 },
      'タスク #55 をブロックする関係を解除しました',
    ],
    [
      'relation_removed',
      { type: 'blocked_by', target_seq_id: 55 },
      'タスク #55 にブロックされる関係を解除しました',
    ],
    ['relation_added', {}, '関連タスクを追加しました'],
    ['relation_removed', {}, '関連タスクを外しました'],
  ])('関係の変更 %s %j を向きと相手つきで表示する', (event_type, payload, expected) => {
    expect(text(event_type, payload)).toBe(expected);
  });

  it.each([
    ['critical_fire', '緊急'],
    ['critical', '重大'],
    ['high', '高'],
    ['medium', '中'],
    ['low', '低'],
    ['trivial', '些細'],
    // 表にない値は隠さずそのまま出す
    ['urgent', 'urgent'],
  ])('優先度の変更 %s を日本語で表示する', (to, label) => {
    expect(text('priority_changed', { from: 'low', to })).toBe(`優先度を ${label} に変更しました`);
  });

  it.each([
    ['label_added', { label_id: 'label-1', name: 'バグ' }, 'ラベル「バグ」を追加しました'],
    ['label_removed', { label_id: 'label-1', name: 'バグ' }, 'ラベル「バグ」を外しました'],
    ['label_added', {}, 'ラベルを追加しました'],
    ['label_removed', {}, 'ラベルを外しました'],
  ])('ラベルの変更 %s %j を表示する', (event_type, payload, expected) => {
    expect(text(event_type, payload)).toBe(expected);
  });
});
