<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import AdminShell from '@/components/admin/AdminShell.vue';
import AdminUserDetail from '@/components/admin/AdminUserDetail.vue';

const pageContext = usePageContext();
</script>

<template>
  <AdminShell title="ユーザー詳細" description="権限・停止・認証方法を確認し、復旧操作を行います。">
    <AdminUserDetail :user-id="String(pageContext.routeParams.userId)" />
  </AdminShell>
</template>
