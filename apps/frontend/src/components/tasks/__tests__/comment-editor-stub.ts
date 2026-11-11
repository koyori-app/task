import { defineComponent, h } from 'vue';

/**
 * 単体テスト用の CommentEditor の代役。
 *
 * 実物は CodeMirror で、happy-dom では起動できない（MarkdownEditor と同じ事情）。呼び出し側の
 * 単体テストが見たいのは「下書きが編集器へ渡り、変更が返ってくる」「候補と対応表が渡る」「@ ボタンが
 * 候補を開く」という配線なので、同じ v-model の面を持つ textarea に差し替える。
 * 札・候補・IME などの実物の挙動は、実ブラウザで動く story（stories/components/CommentEditor.stories.ts）が見る。
 *
 * 使い方（import より前に置く。vi.mock は巻き上げられる）:
 *   vi.mock('@/components/tasks/CommentEditor.vue', async () => ({
 *     default: (await import('@/components/tasks/__tests__/comment-editor-stub')).default,
 *   }));
 */
export const openMentionPickerSpy = { calls: 0 };

export default defineComponent({
  name: 'CommentEditorStub',
  props: {
    modelValue: { type: String, default: '' },
    candidates: { type: Array, default: () => [] },
    mentions: { type: Object, default: () => ({}) },
    disabled: { type: Boolean, default: false },
    placeholder: { type: String, default: '' },
    ariaLabel: { type: String, default: undefined },
  },
  emits: ['update:modelValue'],
  setup(props, { emit, expose }) {
    expose({
      focus: () => {},
      openMentionPicker: () => {
        openMentionPickerSpy.calls += 1;
      },
    });
    return () =>
      h('textarea', {
        value: props.modelValue,
        disabled: props.disabled,
        placeholder: props.placeholder,
        'aria-label': props.ariaLabel,
        'data-candidates': String(props.candidates.length),
        onInput: (event: Event) =>
          emit('update:modelValue', (event.target as HTMLTextAreaElement).value),
      });
  },
});
