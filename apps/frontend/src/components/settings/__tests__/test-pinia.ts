import { createPinia, type Pinia } from 'pinia';
import { createPersistedState } from 'pinia-plugin-persistedstate';

/** 本番の onCreatePinia と同じく、localStorage に残す pinia を作る。 */
export function persistedPinia(): Pinia {
  const pinia = createPinia();
  pinia.use(createPersistedState({ storage: localStorage }));
  return pinia;
}

/** 環境設定・実験的機能を localStorage に先に置く（store を作る前に呼ぶ）。 */
export function seedSettings(seed: {
  preferences?: Record<string, boolean>;
  experiments?: Record<string, boolean>;
}) {
  if (seed.preferences) {
    localStorage.setItem('preferences', JSON.stringify({ values: seed.preferences }));
  }
  if (seed.experiments) {
    localStorage.setItem('experiments', JSON.stringify({ values: seed.experiments }));
  }
}
