<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhArrowClockwise, PhEnvelopeSimple, PhPaperPlaneTilt, PhX } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';

import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select';
import { apiClient, useMeQuery } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';
import { useAuthStore } from '@/stores/auth';

type TenantResponse = Pick<
  components['schemas']['TenantListItemResponse'],
  'id' | 'name' | 'owner_id'
>;
type TenantMemberResponse = components['schemas']['TenantMemberResponse'];
type TenantRole = components['schemas']['TenantRole'];
type TenantInvitationResponse = components['schemas']['TenantInvitationResponse'];

const MEMBERS_PATH = '/v1/tenants/{tenant_id}/members' as const;
const MEMBER_PATH = '/v1/tenants/{tenant_id}/members/{user_id}' as const;
const INVITATIONS_PATH = '/v1/tenants/{tenant_id}/invitations' as const;
const INVITATION_PATH = '/v1/tenants/{tenant_id}/invitations/{invitation_id}' as const;
const INVITATION_RESEND_PATH =
  '/v1/tenants/{tenant_id}/invitations/{invitation_id}/resend' as const;

/**
 * ロールを見ているのは今のところテナントメンバー管理 API だけで、Viewer と Member に
 * 書き込みの差は無い（`apps/backend/docs/tenant-project-authz.md`）。読み取り専用が
 * 全経路で効くまでは、権限が絞られたと誤解させない書き方にしておく。
 */
const ROLES: { value: TenantRole; label: string; description: string }[] = [
  { value: 'Admin', label: 'Admin', description: 'メンバーの追加・変更・削除ができる' },
  { value: 'Member', label: 'Member', description: 'タスクの作成と編集ができる' },
  {
    value: 'Viewer',
    label: 'Viewer',
    description: '今は Member と同じ（読み取り専用は未実装）',
  },
];

const props = defineProps<{ tenant: TenantResponse }>();

const queryClient = useQueryClient();
const authStore = useAuthStore();

const membersQuery = apiClient.useQuery(
  'get',
  MEMBERS_PATH,
  {
    params: { path: { tenant_id: props.tenant.id } },
  },
  {
    staleTime: 60_000,
    retry: false,
  },
);
const meQuery = useMeQuery();
const currentUser = computed(() => meQuery.data.value ?? authStore.user);

const updateMutation = apiClient.useMutation('put', MEMBER_PATH);
const removeMutation = apiClient.useMutation('delete', MEMBER_PATH);

/**
 * オーナーは認可上 tenant_members に行を持たない。現行 API は一覧用の synthetic row
 * を返すが、旧レスポンスや更新途中でも owner が欠けないよう本人情報から補完する。
 */
const members = computed<TenantMemberResponse[]>(() => {
  const currentMembers = [...(membersQuery.data.value ?? [])];
  const me = currentUser.value;
  if (me && me.id === props.tenant.owner_id && !currentMembers.some((m) => m.user_id === me.id)) {
    currentMembers.unshift({
      id: me.id,
      tenant_id: props.tenant.id,
      user_id: me.id,
      role: 'Admin',
      user: { id: me.id, username: me.username, avatar_url: me.avatar_url ?? null },
    });
  }
  return currentMembers;
});
const memberCount = computed(() => members.value.length);

const canManageMembers = computed(() => {
  const userId = currentUser.value?.id;
  if (!userId) return false;
  if (userId === props.tenant.owner_id) return true;
  return members.value.some((member) => member.user_id === userId && member.role === 'Admin');
});

async function invalidateMembers() {
  await queryClient.invalidateQueries({ queryKey: ['get', MEMBERS_PATH] });
}

// 招待の一覧は管理者にしか開かない（API も 403 を返す）
const invitationsQuery = apiClient.useQuery(
  'get',
  INVITATIONS_PATH,
  {
    params: { path: { tenant_id: props.tenant.id } },
  },
  {
    enabled: canManageMembers,
    retry: false,
  },
);
const invitations = computed<TenantInvitationResponse[]>(() => invitationsQuery.data.value ?? []);

const createInvitationMutation = apiClient.useMutation('post', INVITATIONS_PATH);
const resendInvitationMutation = apiClient.useMutation('post', INVITATION_RESEND_PATH);
const deleteInvitationMutation = apiClient.useMutation('delete', INVITATION_PATH);

async function invalidateInvitations() {
  await queryClient.invalidateQueries({ queryKey: ['get', INVITATIONS_PATH] });
}

function errorStatus(e: unknown) {
  return (e as { response?: { status?: number } }).response?.status;
}

/** オーナーは外せず、ロールも変えられない。行の見た目もそこで分ける。 */
function isOwner(member: TenantMemberResponse) {
  return member.user_id === props.tenant.owner_id;
}

function isSelf(member: TenantMemberResponse) {
  return member.user_id === currentUser.value?.id;
}

function roleDescription(role: TenantRole) {
  return ROLES.find((entry) => entry.value === role)?.description ?? '';
}

/** アバターの頭文字。表示名が無いときはユーザー名の先頭 2 文字を使う。 */
function initials(member: TenantMemberResponse) {
  return member.user.username.slice(0, 2).toUpperCase();
}

/** 頭文字から色を決める。同じ人はいつも同じ色になる。 */
const AVATAR_COLORS = ['#0f766e', '#7c3aed', '#b45309', '#be123c', '#1d4ed8', '#4d7c0f'];
function avatarColor(member: TenantMemberResponse) {
  const sum = [...member.user.username].reduce((acc, char) => acc + char.charCodeAt(0), 0);
  return AVATAR_COLORS[sum % AVATAR_COLORS.length];
}

/**
 * 同じメンバーへの PUT が並ぶと、後から完了した古い要求が最後の選択を上書きする。
 * Admin の付与・剥奪が絡むので、変更が終わる（再取得まで含む）まで全操作を止める。
 */
const busy = ref(false);

// --- 招待 ---

/** 送る前の粗い形の確認。最終的な判定は API（validator の email）に任せる。 */
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const inviteEmail = ref('');
const inviteRole = ref<TenantRole>('Member');
const inviting = ref(false);
const inviteError = ref<string | null>(null);
const inviteNotice = ref<string | null>(null);

const inviteEmailValid = computed(() => EMAIL_PATTERN.test(inviteEmail.value.trim()));

/** 送信と再送で共通の失敗理由。 */
function sendFailure(e: unknown, fallback: string) {
  switch (errorStatus(e)) {
    case 400:
      return 'メールアドレスの形が正しくありません。';
    case 409:
      return 'このアドレスの人は既にメンバーです。';
    case 429:
      return '同じアドレスへは 1 分ほど空けてから送ってください。';
    default:
      return fallback;
  }
}

async function onInvite() {
  if (!canManageMembers.value || inviting.value || !inviteEmailValid.value) return;
  const email = inviteEmail.value.trim();
  inviteError.value = null;
  inviteNotice.value = null;
  inviting.value = true;
  try {
    await createInvitationMutation.mutateAsync({
      params: { path: { tenant_id: props.tenant.id } },
      body: { email, role: inviteRole.value },
    });
    inviteEmail.value = '';
    inviteNotice.value = `${email} に招待を送りました。`;
    await invalidateInvitations();
  } catch (e) {
    inviteError.value = sendFailure(e, '招待を送れませんでした。');
  } finally {
    inviting.value = false;
  }
}

// --- 保留中の招待 ---

const invitationError = ref<string | null>(null);
const invitationNotice = ref<string | null>(null);

function isExpired(invitation: TenantInvitationResponse) {
  return new Date(invitation.expires_at) <= new Date();
}

function formatDate(value: string) {
  return new Date(value).toLocaleDateString('ja-JP');
}

async function onResendInvitation(invitation: TenantInvitationResponse) {
  if (!canManageMembers.value || busy.value) return;
  invitationError.value = null;
  invitationNotice.value = null;
  busy.value = true;
  try {
    await resendInvitationMutation.mutateAsync({
      params: { path: { tenant_id: props.tenant.id, invitation_id: invitation.id } },
    });
    invitationNotice.value = `${invitation.email} に招待を送り直しました。`;
    await invalidateInvitations();
  } catch (e) {
    invitationError.value = sendFailure(e, '招待を送り直せませんでした。');
  } finally {
    busy.value = false;
  }
}

async function onRevokeInvitation(invitation: TenantInvitationResponse) {
  if (!canManageMembers.value || busy.value) return;
  invitationError.value = null;
  invitationNotice.value = null;
  busy.value = true;
  try {
    await deleteInvitationMutation.mutateAsync({
      params: { path: { tenant_id: props.tenant.id, invitation_id: invitation.id } },
    });
    await invalidateInvitations();
  } catch {
    invitationError.value = '招待を取り消せませんでした。';
    await invalidateInvitations();
  } finally {
    busy.value = false;
  }
}

// --- ロール変更 ---

const roleError = ref<string | null>(null);

async function onRoleChange(member: TenantMemberResponse, role: TenantRole) {
  if (!canManageMembers.value || isOwner(member) || busy.value) return;
  if (role === member.role) return;
  roleError.value = null;
  busy.value = true;
  try {
    await updateMutation.mutateAsync({
      params: { path: { tenant_id: props.tenant.id, user_id: member.user_id } },
      body: { role },
    });
    await invalidateMembers();
  } catch {
    roleError.value = 'ロールを変更できませんでした。';
    // 表示は membersQuery のデータに束縛しているので、再取得で元のロールへ戻る
    await invalidateMembers();
  } finally {
    busy.value = false;
  }
}

// --- 削除 ---

const removeError = ref<string | null>(null);

/**
 * 自分自身は外せない。除名した本人はテナントへの口を失うのに、この画面と
 * テナント一覧・ストアはそのまま残り、再取得が 403 になって取り残される。
 * 退出の導線（一覧とストアを取り直して別テナントへ移す）ができるまでは塞ぐ。
 */
function canRemove(member: TenantMemberResponse) {
  return canManageMembers.value && !isOwner(member) && !isSelf(member);
}

async function onRemove(member: TenantMemberResponse) {
  if (!canRemove(member) || busy.value) return;
  removeError.value = null;
  busy.value = true;
  try {
    await removeMutation.mutateAsync({
      params: { path: { tenant_id: props.tenant.id, user_id: member.user_id } },
    });
    await invalidateMembers();
  } catch {
    removeError.value = 'メンバーを外せませんでした。';
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="min-h-0 flex-1 overflow-auto">
    <div class="mx-auto max-w-[760px] px-6 pb-14 pt-8">
      <div class="mb-6">
        <h1 class="m-0 mb-1 text-2xl font-bold tracking-tight">メンバー</h1>
        <p class="m-0 text-sm text-muted-foreground">
          <strong class="font-medium text-foreground">{{ tenant.name }}</strong>
          に入れる人を管理します。メンバーはこのテナントのすべてのプロジェクトを見られます。
        </p>
      </div>

      <!-- 招待 -->
      <section v-if="canManageMembers" class="mb-7 rounded-[10px] border p-4">
        <h2 class="mb-3 text-sm font-semibold">人を招待する</h2>
        <form class="flex flex-wrap items-stretch gap-2" @submit.prevent="onInvite">
          <div class="relative min-w-[220px] flex-1">
            <PhEnvelopeSimple
              class="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
            />
            <input
              v-model="inviteEmail"
              type="email"
              required
              :disabled="inviting"
              aria-label="招待するメールアドレス"
              placeholder="name@example.com"
              class="h-9 w-full rounded-md border bg-background pl-8 pr-3 text-sm shadow-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
            />
          </div>
          <Select v-model="inviteRole" :disabled="inviting">
            <SelectTrigger aria-label="招待するロール" class="h-9 w-[130px]">
              <span class="truncate">{{ inviteRole }}</span>
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="role in ROLES" :key="role.value" :value="role.value">
                {{ role.label }}
              </SelectItem>
            </SelectContent>
          </Select>
          <Button type="submit" class="gap-2" :disabled="inviting || !inviteEmailValid">
            <PhPaperPlaneTilt class="size-4" />
            {{ inviting ? '送信中…' : '招待' }}
          </Button>
        </form>
        <p v-if="inviteError" role="alert" class="mt-2.5 text-sm text-destructive">
          {{ inviteError }}
        </p>
        <p v-else-if="inviteNotice" role="status" class="mt-2.5 text-sm">{{ inviteNotice }}</p>
        <p class="mt-2.5 text-xs text-muted-foreground">
          招待メールのリンクは 7
          日間有効です。参加できるのは、招待したアドレスでログインした人だけです。
        </p>
      </section>

      <!-- 保留中の招待 -->
      <section
        v-if="canManageMembers && (invitations.length > 0 || invitationsQuery.isError.value)"
        class="mb-7"
      >
        <div class="mb-2.5 flex items-baseline gap-2">
          <h2 class="text-sm font-semibold">保留中の招待</h2>
          <span class="text-xs text-muted-foreground">{{ invitations.length }}</span>
        </div>
        <p v-if="invitationsQuery.isError.value" role="alert" class="text-sm text-destructive">
          招待を読み込めませんでした
        </p>
        <p v-if="invitationError" role="alert" class="mb-2 text-sm text-destructive">
          {{ invitationError }}
        </p>
        <p v-else-if="invitationNotice" role="status" class="mb-2 text-sm">
          {{ invitationNotice }}
        </p>
        <ul v-if="invitations.length > 0" class="rounded-[10px] border">
          <li
            v-for="invitation in invitations"
            :key="invitation.id"
            class="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
          >
            <span
              class="flex size-[34px] shrink-0 items-center justify-center rounded-lg border border-dashed text-muted-foreground"
              aria-hidden="true"
            >
              <PhEnvelopeSimple class="size-4" />
            </span>
            <span class="min-w-0 flex-1 overflow-hidden leading-snug">
              <span class="block truncate text-sm font-medium">{{ invitation.email }}</span>
              <span class="block truncate text-xs text-muted-foreground">
                {{ invitation.role }} ·
                <span v-if="isExpired(invitation)" class="text-destructive">期限切れ</span>
                <template v-else>{{ formatDate(invitation.expires_at) }} まで有効</template>
              </span>
            </span>
            <Button
              variant="ghost"
              size="sm"
              :disabled="busy"
              class="h-7 shrink-0 gap-1.5 text-muted-foreground"
              :aria-label="`${invitation.email}へ招待を送り直す`"
              @click="onResendInvitation(invitation)"
            >
              <PhArrowClockwise class="size-3.5" />
              再送
            </Button>
            <Button
              variant="ghost"
              size="icon"
              :disabled="busy"
              class="size-7 shrink-0 text-muted-foreground"
              :aria-label="`${invitation.email}への招待を取り消す`"
              @click="onRevokeInvitation(invitation)"
            >
              <PhX class="size-3.5" />
            </Button>
          </li>
        </ul>
      </section>

      <!-- メンバー -->
      <section>
        <div class="mb-2.5 flex items-baseline gap-2">
          <h2 class="text-sm font-semibold">メンバー</h2>
          <span class="text-xs text-muted-foreground">{{ memberCount }}</span>
        </div>

        <p v-if="membersQuery.isPending.value" class="text-sm text-muted-foreground">
          メンバーを読み込み中…
        </p>
        <p v-else-if="membersQuery.isError.value" role="alert" class="text-sm text-destructive">
          メンバーを読み込めませんでした
        </p>

        <template v-else>
          <p v-if="!canManageMembers" class="mb-2 text-sm text-muted-foreground" role="status">
            メンバーの招待・ロール変更・除外ができるのは、テナントオーナーと Admin だけです。
          </p>

          <p v-if="roleError" role="alert" class="mb-2 text-sm text-destructive">{{ roleError }}</p>
          <p v-if="removeError" role="alert" class="mb-2 text-sm text-destructive">
            {{ removeError }}
          </p>

          <ul class="rounded-[10px] border">
            <li
              v-for="member in members"
              :key="member.id"
              class="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
            >
              <span
                class="flex size-[34px] shrink-0 items-center justify-center rounded-lg text-xs font-semibold text-white"
                :style="{ background: avatarColor(member) }"
                aria-hidden="true"
              >
                {{ initials(member) }}
              </span>
              <span class="min-w-0 flex-1 overflow-hidden leading-snug">
                <span class="block truncate text-sm font-medium">
                  {{ member.user.username }}
                  <span v-if="isSelf(member)" class="text-xs font-normal text-muted-foreground">
                    (あなた)
                  </span>
                </span>
                <!--
                  参照デザインはここにメールを出すが、`UserSummary` は
                  id / username / avatar_url しか返さない。UUID を出しても読めないので、
                  API がメールを持つまでは説明にロールの意味を出す。
                -->
                <span class="block truncate text-xs text-muted-foreground">
                  {{ roleDescription(member.role) }}
                </span>
              </span>

              <span
                v-if="isOwner(member)"
                class="shrink-0 px-2.5 text-[13px] text-muted-foreground"
              >
                オーナー
              </span>

              <template v-else>
                <Select
                  :model-value="member.role"
                  :disabled="!canManageMembers || busy"
                  @update:model-value="onRoleChange(member, $event as TenantRole)"
                >
                  <SelectTrigger
                    :aria-label="`${member.user.username}のロール`"
                    class="h-8 w-[110px] shrink-0"
                  >
                    <!--
                      `SelectValue` は選んだ項目の中身をそのまま写すため、説明まで
                      引き金に出てしまう（「Adminメン…」のような潰れた表示になる）。
                      引き金にはロール名だけを置く。
                    -->
                    <span class="truncate text-[13px]">{{ member.role }}</span>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem v-for="role in ROLES" :key="role.value" :value="role.value">
                      <span class="block text-sm font-medium">{{ role.label }}</span>
                      <span class="block text-xs text-muted-foreground">
                        {{ role.description }}
                      </span>
                    </SelectItem>
                  </SelectContent>
                </Select>
                <Button
                  variant="ghost"
                  size="icon"
                  :disabled="!canRemove(member) || busy"
                  class="size-7 shrink-0 text-muted-foreground"
                  :aria-label="`${member.user.username}を外す`"
                  @click="onRemove(member)"
                >
                  <PhX class="size-3.5" />
                </Button>
              </template>
            </li>
          </ul>

          <p v-if="members.length === 0" class="py-6 text-center text-sm text-muted-foreground">
            メンバーがいません
          </p>
        </template>
      </section>
    </div>
  </div>
</template>
