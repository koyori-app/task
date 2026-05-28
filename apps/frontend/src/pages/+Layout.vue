<!-- https://vike.dev/Layout -->

<script setup lang="ts">
import AppSidebarSkeleton from '@/components/sidebar/AppSidebarSkeleton.vue';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import { Separator } from '@/components/ui/separator';
import { SidebarInset, SidebarProvider, SidebarTrigger } from '@/components/ui/sidebar';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, defineAsyncComponent } from 'vue';

const pageContext = usePageContext();
const isAuthPage = computed(() => ['/signin', '/signup'].includes(pageContext.urlPathname));

const AppSidebar = defineAsyncComponent(() => import('@/components/sidebar/AppSidebar.vue'));
</script>

<template>
  <slot v-if="isAuthPage" />
  <SidebarProvider v-else>
    <Suspense>
      <AppSidebar />
      <template #fallback>
        <AppSidebarSkeleton />
      </template>
    </Suspense>
    <SidebarInset>
      <header
        class="flex h-16 shrink-0 items-center gap-2 transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-12"
      >
        <div class="flex items-center gap-2 px-4">
          <SidebarTrigger class="-ml-1" />
          <Separator orientation="vertical" class="mr-2 data-[orientation=vertical]:h-4" />
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem class="hidden md:block">
                <BreadcrumbLink href="#"> ToDo </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator class="hidden md:block" />
              <BreadcrumbItem>
                <BreadcrumbPage>いい感じにする</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
        </div>
      </header>
      <div class="flex flex-1 flex-col gap-4 p-4 pt-0">
        <slot />
      </div>
    </SidebarInset>
  </SidebarProvider>
</template>
