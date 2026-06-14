import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface AuthUser {
  id: string;
  email: string;
  username: string;
  email_verified: boolean;
  is_admin: boolean;
  is_suspended: boolean;
  totp_enabled: boolean;
}

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
