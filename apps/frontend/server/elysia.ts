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

function scoreToStrength(score: number): PasswordStrength {
  if (score <= 1) return 'low';
  if (score <= 3) return 'medium';
  return 'high';
}

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
        password: t.String(),
      }),
    },
  );

  vike(app, [settingInjector, createTodoHandler]);

  return app;
}

export const app = getApp();
