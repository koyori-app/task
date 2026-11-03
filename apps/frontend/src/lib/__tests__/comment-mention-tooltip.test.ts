// @vitest-environment happy-dom
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';

import {
  MENTION_AUTOCOMPLETE_Z_INDEX,
  mentionExtensions,
  resolveMentionAutocompleteTooltipParent,
} from '../comment-mention-extensions';

const ALICE = '11111111-1111-4111-8111-111111111111';

describe('resolveMentionAutocompleteTooltipParent', () => {
  it('dialog の外では body', () => {
    const host = document.createElement('div');
    document.body.append(host);
    expect(resolveMentionAutocompleteTooltipParent(host)).toBe(document.body);
    host.remove();
  });

  it('dialog の中では dialog 要素', () => {
    const dialog = document.createElement('dialog');
    const host = document.createElement('div');
    dialog.append(host);
    document.body.append(dialog);
    expect(resolveMentionAutocompleteTooltipParent(host)).toBe(dialog);
    dialog.remove();
  });
});

describe('mention autocomplete tooltip layer', () => {
  it('EditorView 起動時に tooltipParent へ CodeMirror の tooltip 容器が載る', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const tooltipParent = document.body;
    const before = tooltipParent.childElementCount;

    const view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: [
          ...mentionExtensions({
            tooltipParent,
            candidates: () => [{ id: ALICE, username: 'alice' }],
            names: {},
            disabled: () => false,
          }),
          EditorView.lineWrapping,
        ],
      }),
      parent: host,
    });

    expect(tooltipParent.childElementCount).toBeGreaterThan(before);
    const container = tooltipParent.lastElementChild as HTMLElement;
    expect(container.style.position).toBe('relative');

    view.destroy();
    host.remove();
  });

  it('候補 popup の theme に決めた z-index が載る', () => {
    document.documentElement.style.setProperty('--koyori-z-menu', '10');
    const host = document.createElement('div');
    document.body.append(host);
    const tooltipParent = document.body;

    const view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: [
          ...mentionExtensions({
            tooltipParent,
            candidates: () => [{ id: ALICE, username: 'alice' }],
            names: {},
            disabled: () => false,
          }),
          EditorView.lineWrapping,
        ],
      }),
      parent: host,
    });

    const container = tooltipParent.lastElementChild as HTMLElement;
    const popup = document.createElement('div');
    popup.className = 'cm-tooltip cm-tooltip-autocomplete';
    container.append(popup);
    expect(getComputedStyle(popup).zIndex).toBe('10');
    expect(MENTION_AUTOCOMPLETE_Z_INDEX).toBe('var(--koyori-z-menu)');

    view.destroy();
    host.remove();
  });
});
