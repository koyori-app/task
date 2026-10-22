import { getCurrentInstance, onMounted, ref } from 'vue';

/**
 * reka-ui の浮き出し（Tooltip）を Teleport する先。
 *
 * showModal した `<dialog>` の外は inert になり、top layer の下に描かれる。body へ出すと
 * ダイアログの中から開いた浮き出しが見えず押せないので、置かれた場所が `<dialog>` の中なら
 * その dialog へ出す。それ以外は従来どおり body。
 */
export function useDialogPortalTarget() {
  const target = ref<string | HTMLElement>('body');
  const instance = getCurrentInstance();
  onMounted(() => {
    // 根が Teleport でも、$el は元の位置に残る目印（コメント等）を指す
    const node = instance?.proxy?.$el as Node | null | undefined;
    const dialog = node?.parentElement?.closest('dialog');
    if (dialog) target.value = dialog;
  });
  return target;
}
