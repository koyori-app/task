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
import { useAuthSession } from '@/composables/useAuthSession';
import { clientOnly } from 'vike-vue/clientOnly';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, defineAsyncComponent, defineComponent } from 'vue';

const TanStackDevtools = import.meta.env.DEV
  ? clientOnly(() => import('@/components/devtools/TanStackDevtoolsClient.vue'))
  : defineComponent({ name: 'TanStackDevtoolsStub', render: () => null });

const showTanStackDevtools = import.meta.env.DEV;

const pageContext = usePageContext();
const isAuthPage = computed(() => ['/signin', '/signup'].includes(pageContext.urlPathname));

const { meQuery } = useAuthSession({
  guard: computed(() => !isAuthPage.value),
});

const AppSidebar = defineAsyncComponent(() => import('@/components/sidebar/AppSidebar.vue'));
</script>

<template>
  <TanStackDevtools v-if="showTanStackDevtools" />
  <slot v-if="isAuthPage" />
  <div
    v-else-if="meQuery.isPending.value"
    class="flex min-h-svh items-center justify-center text-muted-foreground text-sm"
  >
    読み込み中…
  </div>
  <SidebarProvider v-else-if="meQuery.isSuccess.value">
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
