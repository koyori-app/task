<script setup lang="ts">
import { PhPlus } from '@phosphor-icons/vue';
import { navigate } from 'vike/client/router';
import { Button } from '@koyori-app/ui-vue';

const props = defineProps<{
  tenantSlug: string;
  /** オーナーまたはテナント Admin（API の require_tenant_admin と同じ境） */
  canCreateProject: boolean;
}>();

function create() {
  void navigate(`/${props.tenantSlug}/projects/new`);
}
</script>

<template>
  <section
    aria-labelledby="dashboard-first-project-heading"
    class="mb-8 rounded-lg border border-dashed p-5 sm:p-6"
  >
    <h2 id="dashboard-first-project-heading" class="text-base font-semibold">
      最初のプロジェクトを作成しましょう
    </h2>
    <p class="mt-2 text-sm text-muted-foreground">
      タスクはプロジェクトの中に作ります。このテナントにはまだプロジェクトがないため、タスクを作成できません。
    </p>
    <Button v-if="canCreateProject" class="mt-4" label="最初のプロジェクトを作成" @click="create">
      <template #icon><PhPlus class="size-4" /></template>
    </Button>
    <p v-else class="mt-4 text-sm text-muted-foreground">
      プロジェクトを作成できるのはテナントのオーナーと管理者です。管理者に最初のプロジェクトの作成を依頼してください。
    </p>
  </section>
</template>
