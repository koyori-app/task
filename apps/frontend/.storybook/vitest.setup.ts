import { setProjectAnnotations } from '@storybook/vue3-vite';
import { page } from 'vitest/browser';
import { beforeAll, beforeEach } from 'vitest';
import * as previewAnnotations from './preview';

const project = setProjectAnnotations([previewAnnotations]);

beforeAll(project.beforeAll);

beforeEach(async () => {
  await page.viewport(1440, 900);
});
