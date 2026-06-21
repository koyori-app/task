import { onMounted, ref } from 'vue';

export function useHydrated() {
  const isHydrated = ref(false);

  onMounted(() => {
    isHydrated.value = true;
  });

  return isHydrated;
}
