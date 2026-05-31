<template>
  <a :class="{ active: isActive }">
    <slot />
  </a>
</template>

<script lang="ts" setup>
import { usePageContext } from "vike-vue/usePageContext";
import { computed, useAttrs } from "vue";

const pageContext = usePageContext();
const { href } = useAttrs();
const isActive = computed(() => {
  const { urlPathname } = pageContext;
  const hrefStr = href as string;
  return hrefStr === "/" ? urlPathname === hrefStr : urlPathname.startsWith(hrefStr);
});
</script>

<style scoped>
a {
  padding: 2px 10px;
  margin-left: -10px;
}
a.active {
  background-color: #eee;
}
</style>
