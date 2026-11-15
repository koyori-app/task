<script setup lang="ts">
import AdminShell from '@/components/admin/AdminShell.vue';
import AdminUserList from '@/components/admin/AdminUserList.vue';
</script>

<template>
  <AdminShell title="管理" description="ユーザーの作成・停止と、認証事故の復旧を行います。">
    <AdminUserList />
  </AdminShell>
</template>
