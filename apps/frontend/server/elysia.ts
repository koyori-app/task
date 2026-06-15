import { createTodoHandler } from '#/middlewares/create-todo-handler';
import { settingInjector } from '#/middlewares/setting-injector';
import { staticPlugin } from '@elysiajs/static';
import vike from '@vikejs/elysia';
import { zxcvbn, zxcvbnOptions } from '@zxcvbn-ts/core';
import * as commonPackage from '@zxcvbn-ts/language-common';
import * as jaPackage from '@zxcvbn-ts/language-ja';
import { Elysia, t } from 'elysia';

type PasswordStrength = '' | 'low' | 'medium' | 'high';

zxcvbnOptions.setOptions({
  dictionary: {
    ...commonPackage.dictionary,
    jaPasswords: jaPackage.dictionary.commonWords,
  },
  graphs: commonPackage.adjacencyGraphs,
});

/**
 * Converts a numeric password strength score to a categorical strength level.
 *
 * @param score - The password strength score, typically from zxcvbn
 * @returns 'low' for scores up to 1, 'medium' for scores up to 3, 'high' otherwise
 */
function scoreToStrength(score: number): PasswordStrength {
  if (score <= 1) return 'low';
  if (score <= 3) return 'medium';
  return 'high';
}

/**
 * Creates and configures the Elysia application server.
 *
 * @returns The configured Elysia application instance
 */
function getApp() {
  const app = new Elysia();

  app.use(staticPlugin({ assets: 'server/public', prefix: '/static-assets' }));

  app.post(
    '/internal/password-strength',
    ({ body }) => {
      const { password } = body;
      if (!password) return { strength: '' as const };

      return { strength: scoreToStrength(zxcvbn(password).score) };
    },
    {
      body: t.Object({
        password: t.String({ maxLength: 256 }),
      }),
    },
  );

  vike(app, [settingInjector, createTodoHandler]);

  return app;
}

export const app = getApp();
