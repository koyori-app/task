<script setup lang="ts">
import { PhEye, PhEyeSlash } from '@phosphor-icons/vue';
import { ref } from 'vue';
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@/components/originui/input-group';

const model = defineModel<string>({ default: '' });

defineProps<{
  id?: string;
  name?: string;
  placeholder?: string;
  autocomplete?: string;
}>();

const emit = defineEmits<{
  blur: [event: FocusEvent];
  focus: [event: FocusEvent];
}>();

const isVisible = ref(false);

function toggleVisibility() {
  isVisible.value = !isVisible.value;
}
</script>

<template>
  <InputGroup>
    <InputGroupInput
      :id="id"
      :name="name"
      v-model="model"
      :type="isVisible ? 'text' : 'password'"
      :placeholder="placeholder"
      :autocomplete="autocomplete"
      @blur="emit('blur', $event)"
      @focus="emit('focus', $event)"
    />
    <InputGroupAddon align="inline-end">
      <InputGroupButton
        type="button"
        size="icon-xs"
        :aria-label="isVisible ? 'パスワードを隠す' : 'パスワードを表示する'"
        :aria-pressed="isVisible"
        @click="toggleVisibility"
      >
        <PhEye v-if="isVisible" :size="16" />
        <PhEyeSlash v-else :size="16" />
      </InputGroupButton>
    </InputGroupAddon>
  </InputGroup>
</template>
