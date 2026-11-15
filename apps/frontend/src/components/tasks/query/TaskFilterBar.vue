<script setup lang="ts">
import { computed } from 'vue';
import { Dropdown, type DropdownItem } from '@koyori-app/ui-vue';
import { qualifierValues, setQualifierValues, toggleFlag } from '@/lib/task-query/edit';
import { tokenizeTaskQuery, type TaskQueryKey } from '@/lib/task-query/tokenize';
const props = defineProps<{
  modelValue: string;
  statuses: { id: string; name: string }[];
  labels: { id: string; name: string }[];
  members: { id: string; username: string }[];
  membersState?: { loading: boolean; error: boolean; onRetry: () => void };
}>();
const emit = defineEmits<{ 'update:modelValue': [value: string]; 'open:assignee': [] }>();
const keys = ['status', 'assignee', 'label', 'priority', 'due', 'is'] as const;
const names = {
  status: 'ステータス',
  assignee: '担当',
  label: 'ラベル',
  priority: '優先度',
  due: '期限',
  is: 'その他',
};
function selected(key: TaskQueryKey, value: string) {
  return tokenizeTaskQuery(props.modelValue).some(
    (t) => t.kind === 'qualifier' && t.key === key && !t.negated && t.values.includes(value),
  );
}
function relatedActive(key: (typeof keys)[number]) {
  return tokenizeTaskQuery(props.modelValue).some(
    (t) =>
      t.kind === 'qualifier' &&
      (t.key === key ||
        ((key === 'label' || key === 'assignee') && t.key === 'no' && t.values.includes(key)) ||
        (key === 'is' && ['author', 'in'].includes(t.key))),
  );
}
function caption(key: (typeof keys)[number]) {
  if ((key === 'label' || key === 'assignee') && selected('no', key))
    return `${names[key]}: ${key === 'label' ? 'ラベルなし' : '担当者なし'}`;
  const positive = qualifierValues(props.modelValue, key);
  const excluded = qualifierValues(props.modelValue, key, true);
  const values = positive.length ? positive : excluded;
  return values.length
    ? `${names[key]}: ${values.length > 1 ? `${values.length} 件` : values[0]}${!positive.length ? '以外' : ''}`
    : names[key];
}
function toggle(key: TaskQueryKey, value: string) {
  const values = qualifierValues(props.modelValue, key);
  emit(
    'update:modelValue',
    setQualifierValues(
      props.modelValue,
      key,
      values.includes(value) ? values.filter((v) => v !== value) : [...values, value],
    ),
  );
}
function options(key: (typeof keys)[number]): DropdownItem[] {
  const choices =
    key === 'status'
      ? props.statuses.map((v) => [v.name, v.name])
      : key === 'label'
        ? props.labels.map((v) => [v.name, v.name])
        : key === 'assignee'
          ? [['@me', '自分'], ...props.members.map((v) => [v.username, v.username])]
          : key === 'priority'
            ? ['critical_fire', 'critical', 'high', 'medium', 'low', 'trivial'].map((v) => [v, v])
            : key === 'due'
              ? [
                  ['', '指定なし'],
                  ['overdue', '期限超過'],
                  ['<1d', '今日'],
                  ['<7d', '7 日以内'],
                  ['<30d', '30 日以内'],
                ]
              : [
                  ['archived', 'アーカイブ済み'],
                  ['subtask', 'サブタスクだけ'],
                ];
  const items: DropdownItem[] = choices.map(([value, label]) => ({
    value: JSON.stringify([key, value!]),
    label: label!,
    kind: key === 'due' ? 'radio' : 'checkbox',
    checked: selected(key, value!),
  }));
  if (key === 'label' || key === 'assignee')
    items.push({
      value: JSON.stringify(['no', key]),
      label: key === 'label' ? 'ラベルなし' : '担当者なし',
      kind: 'checkbox',
      checked: selected('no', key),
    });
  if (key === 'is')
    for (const [value, label] of [
      ['author:@me', '自分が作成'],
      ['in:title', 'タイトルだけを探す'],
      ['in:comment', 'コメントだけを探す'],
    ]) {
      const [k, v] = value!.split(':');
      items.push({
        value: JSON.stringify([k, v]),
        label: label!,
        kind: 'checkbox',
        checked: selected(k as TaskQueryKey, v!),
      });
    }
  return items;
}
const menus = computed(() =>
  keys.map((key) => ({
    key,
    label: caption(key),
    items: options(key),
    active: relatedActive(key),
  })),
);
function select(_key: (typeof keys)[number], encoded: string) {
  const [key, value] = JSON.parse(encoded) as [TaskQueryKey, string];
  if (key === 'due')
    emit('update:modelValue', setQualifierValues(props.modelValue, key, value ? [value] : []));
  else if (['no', 'is', 'author', 'in'].includes(key))
    emit('update:modelValue', toggleFlag(props.modelValue, key, value, !selected(key, value)));
  else toggle(key, value);
}
</script>
<template>
  <div class="flex flex-wrap items-center gap-2">
    <Dropdown
      v-for="menu in menus"
      :key="menu.key"
      variant="tertiary"
      :class="menu.active ? '[&>button]:bg-muted' : ''"
      :label="menu.label"
      :items="menu.items"
      :on-select="(value) => select(menu.key, value)"
      @click="menu.key === 'assignee' && emit('open:assignee')"
    /><span v-if="membersState?.loading" class="text-xs text-muted-foreground">担当者を取得中</span
    ><button
      v-if="membersState?.error"
      type="button"
      class="text-xs underline"
      @click="membersState.onRetry"
    >
      担当者を再取得
    </button>
  </div>
</template>
