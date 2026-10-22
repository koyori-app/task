<script setup lang="ts">
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { PhPlus } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import UserAvatar from '@/components/UserAvatar.vue';
import UserSelector from '@/components/users/UserSelector.vue';
import { Button, Dialog, Field, Picker } from '@koyori-app/ui-vue';
import { LIST_PROJECTS_PATH, apiClient, useMeQuery } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectMemberResponse = components['schemas']['ProjectMemberResponse'];
type TenantMemberResponse = components['schemas']['TenantMemberResponse'];
type ProjectRole = components['schemas']['ProjectRole'];

const MEMBERS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/members' as const;
const MEMBER_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/members/{user_id}' as const;
const TENANT_MEMBERS_PATH = '/v1/tenants/{tenant_id}/members' as const;

const ROLES: { value: ProjectRole; label: string; description: string }[] = [
  { value: 'Admin', label: '管理者', description: 'メンバーの管理を含むすべての操作' },
  { value: 'Member', label: 'メンバー', description: 'プロジェクトに参加して作業する' },
  { value: 'Viewer', label: '閲覧者', description: 'プロジェクトを閲覧する' },
];

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const queryClient = useQueryClient();

// 変更系は呼び出し時に props を読むので、読み取りも props に追従させる。
// 素のオブジェクトで組むと setup 時の値で固定され、一覧と操作先がずれる
// （親側の `:key` でも防いでいるが、こちらだけでも成立させる）。
const pathParams = computed(() => ({
  params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
}));
const membersQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', MEMBERS_PATH, pathParams.value),
    retry: false,
  })),
);
const tenantMembersQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', TENANT_MEMBERS_PATH, {
      params: { path: { tenant_id: props.tenantId } },
    }),
    retry: false,
  })),
);

// options を computed で組むと data の型が推論から落ちるので、ここで戻す
// （落としたままだと下流の map / filter が暗黙 any になり、vue-tsc が落ちる）
const members = computed<ProjectMemberResponse[]>(() => membersQuery.data.value ?? []);
const tenantMembers = computed<TenantMemberResponse[]>(() => tenantMembersQuery.data.value ?? []);

/** 一覧・追加・変更・削除はプロジェクト管理者（またはテナントオーナー・入れるテナント管理者）専用。 */
const isForbidden = computed(
  () =>
    (membersQuery.error.value as { response?: { status?: number } } | null)?.response?.status ===
    403,
);

/**
 * 追加候補 = テナントメンバーのうち、まだプロジェクトメンバーでない人。
 *
 * 取得に失敗したときも空になるので、「候補がいません」と出す前に
 * `tenantMembersQuery.isError` を見る（失敗を「テナントに人が居ない」という
 * 別の事実にすり替えない）。
 */
const candidates = computed(() => {
  const existing = new Set(members.value.map((m) => m.user_id));
  return tenantMembers.value.filter((tm) => !existing.has(tm.user_id));
});

/** UserSelector に渡す形（テナントメンバーの行から利用者を引く）。 */
const candidateUsers = computed(() =>
  candidates.value.map((candidate) => ({
    id: candidate.user_id,
    username: candidate.user.username,
    avatar_url: candidate.user.avatar_url ?? null,
  })),
);

const addMutation = apiClient.useMutation('post', MEMBERS_PATH);
const updateMutation = apiClient.useMutation('put', MEMBER_PATH);
const removeMutation = apiClient.useMutation('delete', MEMBER_PATH);

const meQuery = useMeQuery();

/** 対象が自分か。自分を外すとこの画面を開けなくなるので、削除の前に伝える。 */
function isSelf(member: ProjectMemberResponse) {
  return member.user_id === meQuery.data.value?.id;
}

/**
 * メンバーの増減はプロジェクトの見え方も変える。
 *
 * - 自分を外せば、そのプロジェクトは一覧から消える
 * - 最後の 1 人を外せば `project_members` が 0 件になり、テナントメンバー全員に開く
 *
 * どちらもプロジェクト一覧の中身が変わるので、メンバーだけでなく一覧も無効化する
 * （しないと、サイドバーや設定の他セクションが古い権限のまま操作できる顔で残る）。
 */
async function invalidateMembers() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', MEMBERS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', LIST_PROJECTS_PATH] }),
  ]);
}

function errorStatus(e: unknown): number | undefined {
  return (e as { response?: { status?: number } }).response?.status;
}

// --- 追加 ---

const selectedUserId = ref<string | null>(null);
const selectedRole = ref<ProjectRole>('Member');
const addError = ref<string | null>(null);

async function onAdd() {
  const userId = selectedUserId.value;
  if (!userId) return;
  addError.value = null;
  try {
    await addMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body: { user_id: userId, role: selectedRole.value },
    });
    await invalidateMembers();
    selectedUserId.value = null;
    selectedRole.value = 'Member';
  } catch (e) {
    const status = errorStatus(e);
    addError.value =
      status === 409
        ? 'この利用者は既にメンバーです。'
        : status === 400
          ? 'テナントメンバーでない利用者は追加できません。'
          : 'メンバーを追加できませんでした。';
  }
}

// --- ロール変更 ---

const roleError = ref<string | null>(null);

async function onRoleChange(member: ProjectMemberResponse, role: ProjectRole) {
  if (role === member.role) return;
  roleError.value = null;
  try {
    await updateMutation.mutateAsync({
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          user_id: member.user_id,
        },
      },
      body: { role },
    });
    await invalidateMembers();
  } catch (e) {
    roleError.value =
      errorStatus(e) === 409
        ? `最後の管理者「${member.user.username}」は降格できません。`
        : 'ロールを変更できませんでした。';
    // 表示は membersQuery のデータに束縛しているため、失敗時は再取得で元のロールに戻る
    await invalidateMembers();
  }
}

// --- 削除 ---

const removeTarget = ref<ProjectMemberResponse | null>(null);
const removeError = ref<string | null>(null);

function openRemove(member: ProjectMemberResponse) {
  removeError.value = null;
  removeTarget.value = member;
}

// 警告も説明に含める。説明（aria-describedby）に入れないと、開いたときに読み上げられない
const removeDescription = computed(() => {
  const target = removeTarget.value;
  if (!target) return '';
  return [
    `「${target.user.username}」をこのプロジェクトから外します。担当タスクの割り当ては残りますが、ウォッチしていたタスクの通知は解除されます。`,
    members.value.length === 1 &&
      'これが最後のメンバーです。外すとメンバー指定が無くなり、このプロジェクトはテナントメンバー全員が開けるようになります。',
    isSelf(target) && '自分をこのプロジェクトから外します。以後この設定画面は開けなくなります。',
  ]
    .filter(Boolean)
    .join('');
});

function onRemoveClose() {
  // 削除リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (removeMutation.isPending.value) return;
  removeTarget.value = null;
}

async function confirmRemove() {
  const target = removeTarget.value;
  if (!target) return;
  removeError.value = null;
  try {
    await removeMutation.mutateAsync({
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          user_id: target.user_id,
        },
      },
    });
    await invalidateMembers();
    removeTarget.value = null;
  } catch (e) {
    removeError.value =
      errorStatus(e) === 409
        ? '最後の管理者は削除できません。'
        : 'メンバーを削除できませんでした。';
  }
}
</script>

<template>
  <div>
    <div class="mb-5 border-b pb-4">
      <h2 class="text-xl font-semibold">メンバー</h2>
      <p class="mt-1 text-sm text-muted-foreground">
        メンバーを 1 人も指定しない間は、テナントメンバー全員がこのプロジェクトに入れます。
        指定すると、ここに載っている人だけに絞られます。
      </p>
    </div>

    <p v-if="membersQuery.isPending.value" class="text-sm text-muted-foreground">読み込み中…</p>

    <p v-else-if="isForbidden" class="text-sm text-muted-foreground">
      メンバーを管理できるのは、テナントオーナー・テナント管理者とこのプロジェクトの管理者だけです。
    </p>

    <p v-else-if="membersQuery.isError.value" role="alert" class="text-sm text-destructive">
      メンバーを読み込めませんでした
    </p>

    <template v-else>
      <!-- 追加フォーム -->
      <div class="mb-5 flex flex-wrap items-end gap-2">
        <Field id="member-candidate" label="テナントメンバーから追加" class="min-w-[220px] flex-1">
          <UserSelector
            label="テナントメンバーから追加"
            :users="candidateUsers"
            :model-value="selectedUserId ? [selectedUserId] : []"
            :multiple="false"
            :state="{
              loading: tenantMembersQuery.isPending.value,
              error: tenantMembersQuery.isError.value,
            }"
            search-label="テナントメンバーを検索"
            empty-text="追加できるテナントメンバーがいません。"
            no-match-text="一致するテナントメンバーがいません"
            @update:model-value="(ids) => (selectedUserId = ids[0] ?? null)"
          />
        </Field>
        <Field id="member-role" label="ロール" class="w-[140px]">
          <Picker
            label="ロール"
            :searchable="false"
            :items="ROLES"
            :selected-values="[selectedRole]"
            :on-selection-change="(values: string[]) => (selectedRole = values[0] as ProjectRole)"
          />
        </Field>
        <Button
          type="button"
          :disabled="!selectedUserId || addMutation.isPending.value"
          :label="addMutation.isPending.value ? '追加中…' : '追加'"
          @click="onAdd"
        >
          <template #icon><PhPlus class="size-4" /></template>
        </Button>
      </div>
      <p
        v-if="tenantMembersQuery.isError.value"
        role="alert"
        class="-mt-3 mb-5 text-sm text-destructive"
      >
        追加候補を読み込めませんでした
      </p>
      <p
        v-else-if="!candidates.length && !tenantMembersQuery.isPending.value"
        class="-mt-3 mb-5 text-xs text-muted-foreground"
      >
        追加できるテナントメンバーがいません。先にテナントへメンバーを追加してください。
      </p>
      <p v-if="addError" role="alert" class="-mt-3 mb-5 text-sm text-destructive">
        {{ addError }}
      </p>

      <p v-if="roleError" role="alert" class="mb-3 text-sm text-destructive">{{ roleError }}</p>

      <p v-if="members.length === 0" class="text-sm text-muted-foreground">
        メンバーはまだ指定されていません。
      </p>

      <ul v-else class="overflow-hidden rounded-lg border" data-testid="member-list">
        <li
          v-for="member in members"
          :key="member.id"
          class="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
        >
          <UserAvatar :user="member.user" size="xl" :initial-length="1" />
          <span class="min-w-0 flex-1 truncate text-sm font-medium">
            {{ member.user.username }}
          </span>
          <!-- 一覧の overflow-hidden に切られないよう、一覧はトップレイヤーに出す -->
          <Picker
            :label="`${member.user.username} のロール`"
            class="w-[130px]"
            top-layer
            :searchable="false"
            :disabled="updateMutation.isPending.value"
            :items="ROLES"
            :selected-values="[member.role]"
            :on-selection-change="
              (values: string[]) => onRoleChange(member, values[0] as ProjectRole)
            "
          />
          <Button
            type="button"
            variant="ghost"
            class="text-(--koyori-color-danger)"
            :ariaLabel="`メンバー「${member.user.username}」を削除`"
            label="削除"
            @click="openRemove(member)"
          />
        </li>
      </ul>
    </template>

    <Dialog
      :open="removeTarget !== null"
      title="メンバーを削除しますか？"
      :description="removeDescription"
      :on-close="onRemoveClose"
    >
      <p v-if="removeError" role="alert" class="text-sm text-destructive">{{ removeError }}</p>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="removeMutation.isPending.value"
          label="キャンセル"
          @click="removeTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="removeMutation.isPending.value"
          :label="removeMutation.isPending.value ? '削除中…' : '削除する'"
          @click="confirmRemove"
        />
      </template>
    </Dialog>
  </div>
</template>
