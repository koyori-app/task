import { watchDebounced } from '@vueuse/core';
import { ref, watch, type Ref } from 'vue';

export type PasswordStrength = '' | 'low' | 'medium' | 'high';

/**
 * Expected behavior (zxcvbn-ts via server API):
 * - `12345678` → low
 * - `Password1` → low
 * - `P@ssw0rd` → low
 * - `sakura123` → low〜medium（jaPasswords により low 寄り）
 * - `Tr0ub4dor&3` → high
 */
export function usePasswordStrength(password: Ref<string>): {
  strength: Ref<PasswordStrength>;
} {
  const strength = ref<PasswordStrength>('') as Ref<PasswordStrength>;
  let seq = 0;

  watch(password, (value) => {
    if (!value) {
      seq++;
      strength.value = '';
    }
  });

  watchDebounced(
    password,
    async (value) => {
      if (!value) return;

      const id = ++seq;
      try {
        const response = await fetch('/internal/password-strength', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ password: value }),
        });

        if (!response.ok || id !== seq) return;

        const data = (await response.json()) as { strength: PasswordStrength };
        strength.value = data.strength;
      } catch {
        // network/parse error: keep previous strength
      }
    },
    { debounce: 300 },
  );

  return { strength };
}
