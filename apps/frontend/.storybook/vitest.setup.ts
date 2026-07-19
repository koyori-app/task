import { setProjectAnnotations } from '@storybook/vue3-vite';
import { beforeAll } from 'vitest';
import * as previewAnnotations from './preview';

const project = setProjectAnnotations([previewAnnotations]);

beforeAll(project.beforeAll);
