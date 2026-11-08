<script setup lang="ts">
import {
  PhFiles,
  PhGear,
  PhGitPullRequest,
  PhListChecks,
  PhPlus,
  PhTag,
} from '@phosphor-icons/vue';
import { computed } from 'vue';
import type { components } from '@/generated/api';

import { Accordion, Button, SidebarLink, Skeleton } from '@koyori-app/ui-vue';

export type ProjectNavItem = components['schemas']['ProjectResponse'];

const props = defineProps<{
  tenantSlug: string;
  projects: components['schemas']['ProjectResponse'][];
  /** 現在の URL パス。active 強調と初期展開の判定に使う */
  currentPath?: string;
  loading?: boolean;
  error?: boolean;
}>();

const emit = defineEmits<{
  retry: [];
  create: [];
}>();

const sortedProjects = computed(() =>
  [...props.projects].sort((a, b) => {
    if (a.is_personal === b.is_personal) return a.name.localeCompare(b.name);
    return a.is_personal ? -1 : 1;
  }),
);

function projectBaseUrl(project: ProjectNavItem) {
  return `/${props.tenantSlug}/projects/${project.key}`;
}

/** プロジェクト配下のページを開いているか（初期展開・親の active 判定） */
function isProjectActive(project: ProjectNavItem) {
  return !!props.currentPath && props.currentPath.startsWith(`${projectBaseUrl(project)}/`);
}

interface ProjectChild {
  label: string;
  href: string;
  icon: typeof PhListChecks;
}

function projectChildren(project: ProjectNavItem): ProjectChild[] {
  const base = projectBaseUrl(project);
  const children: ProjectChild[] = [
    { label: 'タスク', href: `${base}/tasks`, icon: PhListChecks },
    { label: 'ドキュメント', href: `${base}/docs`, icon: PhFiles },
    { label: 'ラベル', href: `${base}/labels`, icon: PhTag },
  ];
  // 個人プロジェクト（個人 Inbox）はシステム管理のため設定を出さない。
  // レビューも、設定へ入れない＝GitHub 連携を張る手段が無いので出さない
  if (!project.is_personal) {
    children.push({ label: 'レビュー', href: `${base}/reviews`, icon: PhGitPullRequest });
    children.push({ label: '設定', href: `${base}/settings`, icon: PhGear });
  }
  return children;
}

/** アイコンの無いプロジェクトに丸で出す頭文字（サロゲートペアを割らない） */
function projectInitial(project: ProjectNavItem) {
  return ([...project.name.trim()][0] ?? '?').toUpperCase();
}

function isChildActive(child: ProjectChild) {
  // 完全一致 or セグメント境界付き前方一致（tasks と tasks-archive のような
  // 接頭辞衝突を防ぎつつ、配下ページ /…/tasks/T-1 でも「タスク」を active に保つ）
  return (
    !!props.currentPath &&
    (props.currentPath === child.href || props.currentPath.startsWith(`${child.href}/`))
  );
}
</script>

<template>
  <div class="flex flex-col gap-(--koyori-space-xs)">
    <div
      class="flex items-center justify-between gap-2 px-3 in-data-[sidebar-rail=true]:justify-center in-data-[sidebar-rail=true]:px-0"
    >
      <!-- レールでは見出しの文字が幅に収まらないので、読み上げにだけ残す -->
      <h2 class="text-xs font-medium text-muted-foreground in-data-[sidebar-rail=true]:sr-only">
        Projects
      </h2>
      <Button
        v-if="tenantSlug"
        variant="ghost"
        ariaLabel="プロジェクトを作成"
        @click="emit('create')"
      >
        <template #icon><PhPlus class="size-4" /></template>
      </Button>
    </div>
    <div v-if="loading" class="flex flex-col gap-(--koyori-space-xs)">
      <p class="sr-only">プロジェクトを読み込み中…</p>
      <Skeleton v-for="n in 3" :key="n" />
    </div>
    <!-- レールでは文言とボタンが幅に収まらないので、失敗・0 件の案内は広げたときに出す -->
    <div v-else-if="error" class="flex flex-col gap-2 px-3 py-2 in-data-[sidebar-rail=true]:hidden">
      <p class="text-xs leading-snug text-destructive">プロジェクト一覧を取得できませんでした</p>
      <Button variant="tertiary" label="再試行" @click="emit('retry')" />
    </div>
    <div v-else-if="sortedProjects.length === 0" class="p-2 in-data-[sidebar-rail=true]:hidden">
      <div class="rounded-lg border border-dashed px-3 py-3.5 text-center">
        <p class="mb-2.5 text-xs leading-snug text-muted-foreground">
          プロジェクトはまだありません。
        </p>
        <Button v-if="tenantSlug" class="w-full" label="プロジェクトを作成" @click="emit('create')">
          <template #icon><PhPlus class="size-3.5" /></template>
        </Button>
      </div>
    </div>
    <template v-else>
      <Accordion
        v-for="project in sortedProjects"
        :id="project.id"
        :key="project.id"
        :label="project.name"
        :default-open="isProjectActive(project)"
      >
        <template #icon>
          <img
            v-if="project.icon_url"
            :src="project.icon_url"
            alt=""
            class="size-4 shrink-0 rounded-sm object-cover"
          />
          <span v-else-if="project.icon_emoji" class="text-base leading-none">{{
            project.icon_emoji
          }}</span>
          <span
            v-else
            class="flex size-4 shrink-0 items-center justify-center rounded-full bg-accent text-[10px] font-semibold leading-none"
            >{{ projectInitial(project) }}</span
          >
        </template>
        <div v-if="tenantSlug" class="flex flex-col gap-(--koyori-space-xs)">
          <SidebarLink
            v-for="child in projectChildren(project)"
            :key="child.href"
            :label="child.label"
            :href="child.href"
            :current="isChildActive(child)"
          >
            <template #icon><component :is="child.icon" class="size-4" /></template>
          </SidebarLink>
        </div>
      </Accordion>
    </template>
  </div>
</template>
