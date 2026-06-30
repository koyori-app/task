import { type } from 'arktype';
import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { components } from '@/generated/api';

export const authUserSchema = type({
  id: 'string',
  email: 'string',
  username: 'string',
  email_verified: 'boolean',
  is_admin: 'boolean',
  is_suspended: 'boolean',
  totp_enabled: 'boolean',
  'avatar_url?': 'string | null',
  'bio?': 'string | null',
});

export type AuthUser = typeof authUserSchema.infer;

type _BackendUser = components['schemas']['UserResponse'];
type _AuthUserConforms = AuthUser extends Pick<_BackendUser, keyof AuthUser> ? true : never;
const _conform: _AuthUserConforms = true;

export const useAuthStore = defineStore(
  'auth',
  () => {
    const user = ref<AuthUser | null>(null);

    function setUser(u: AuthUser) {
      user.value = u;
    }

    function clearUser() {
      user.value = null;
    }

    return { user, setUser, clearUser };
  },
  { persist: { pick: ['user.id', 'user.username', 'user.email'] } },
);
