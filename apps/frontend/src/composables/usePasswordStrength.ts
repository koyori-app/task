import { watchDebounced } from '@vueuse/core';
import { ref, watch, type Ref } from 'vue';

export type PasswordStrength = '' | 'low' | 'medium' | 'high';

/**
 * Maintains a reactive password strength value based on server-side validation.
 *
 * Clears the strength immediately when the password is empty. For non-empty
 * passwords, performs a debounced server-side strength check.
 *
 * @param password - A reactive reference to the password string
 * @returns An object containing the reactive `strength` value
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
      // TODO: add client-side maxLength guard once backend enforces a limit (e.g. 256)

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
