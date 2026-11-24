import { describe, it, expect } from 'vitest';
import {
  suggestionContext,
  keyCandidates,
  valueCandidates,
  applySuggestion,
  normalizeQueryValue,
} from '../suggest';
import { toggleFlag } from '../edit';
import { describeTaskQueryError, taskQueryErrorsOf } from '../errors';
describe('normalizeQueryValue', () => {
  it('drops @ from usernames but keeps it on @me, which only the backend reads as myself', () => {
    expect(normalizeQueryValue('author', '@ALICE')).toBe('alice');
    expect(normalizeQueryValue('assignee', 'alice')).toBe('alice');
    expect(normalizeQueryValue('author', '@ME')).toBe('@me');
    expect(normalizeQueryValue('author', 'me')).toBe('me');
    expect(normalizeQueryValue('label', '@Bug')).toBe('@bug');
  });
});
describe('query completion', () => {
  it('distinguishes words, keys and the active comma-separated value', () => {
    expect(suggestionContext('word', 4)).toBeNull();
    expect(suggestionContext('-la', 3)).toMatchObject({
      kind: 'keys',
      prefix: 'la',
      negated: true,
    });
    expect(suggestionContext('label:bug,f', 11)).toMatchObject({
      kind: 'values',
      key: 'label',
      chosen: ['bug'],
      prefix: 'f',
    });
    expect(suggestionContext('status:"In', 10)).toMatchObject({ kind: 'values', prefix: 'In' });
    expect(keyCandidates('la')[0]?.value).toBe('label');
  });
  it('completes keys and partial values without committing, adding a comma after the value', () => {
    const context = suggestionContext('label:b', 7)!;
    const candidates = valueCandidates('label', 'b', {
      labels: [
        { id: '1', name: 'debug' },
        { id: '2', name: 'bug' },
      ],
    });
    expect(candidates[0]?.value).toBe('bug');
    expect(applySuggestion('label:b', context, candidates[0]!)).toEqual({
      draft: 'label:bug,',
      caret: 10,
      commit: false,
    });
    expect(applySuggestion('la', suggestionContext('la', 2)!, keyCandidates('la')[0]!)).toEqual({
      draft: 'label:',
      caret: 6,
      commit: false,
    });
  });
  const apply = (draft: string, caret: number, candidate: Parameters<typeof applySuggestion>[2]) =>
    applySuggestion(draft, suggestionContext(draft, caret)!, candidate);
  it('commits exact matches, single-value keys and the none row right away', () => {
    expect(apply('label:BUG', 9, { value: 'bug', label: 'bug' })).toEqual({
      draft: 'label:bug',
      caret: 9,
      commit: true,
    });
    expect(apply('sort:', 5, { value: 'created', label: '作成日' })).toEqual({
      draft: 'sort:created',
      caret: 12,
      commit: true,
    });
    expect(
      apply('label:bug,', 10, { value: 'label', label: 'ラベルなし', key: 'no' }),
    ).toMatchObject({ draft: 'no:label', commit: true });
  });
  it('keeps chosen values, negation, trailing words and quoting when picking', () => {
    expect(apply('label:bug,b', 11, { value: 'bugfix', label: 'bugfix' })).toMatchObject({
      draft: 'label:bug,bugfix,',
      commit: false,
    });
    expect(apply('-label:b', 8, { value: 'bug', label: 'bug' })).toMatchObject({
      draft: '-label:bug,',
      commit: false,
    });
    expect(apply('label:b 通知', 7, { value: 'bug', label: 'bug' })).toEqual({
      draft: 'label:bug, 通知',
      caret: 10,
      commit: false,
    });
    const quoted = apply('status:', 7, { value: 'In Progress', label: 'In Progress' });
    expect(quoted).toEqual({
      draft: 'status:"In Progress",',
      caret: 'status:"In Progress",'.length,
      commit: false,
    });
  });
  it('hides chosen values and the none row once a value is chosen', () => {
    const data = {
      labels: [
        { id: '1', name: 'bug' },
        { id: '2', name: 'feature' },
      ],
      members: [{ id: '3', username: 'alice' }],
    };
    expect(valueCandidates('label', '', data, ['BUG']).map((c) => c.label)).toEqual(['feature']);
    expect(valueCandidates('label', '', data).map((c) => c.label)).toEqual([
      'bug',
      'feature',
      'ラベルなし',
    ]);
    const assignees = valueCandidates('assignee', '', data, ['@alice']).map((c) => c.value);
    expect(assignees).not.toContain('alice');
    expect(assignees).toContain('@me');
    expect(assignees).not.toContain('assignee');
  });
  it('serializes single-value flags as separate AND terms', () => {
    expect(toggleFlag('is:archived', 'is', 'subtask', true)).toBe('is:archived is:subtask');
    expect(toggleFlag('is:archived is:subtask', 'is', 'archived', false)).toBe('is:subtask');
  });
  it('distinguishes query errors from failures with retry', () => {
    expect(taskQueryErrorsOf({ message: 'server-error' })).toBeNull();
    const error = {
      code: 'unknown-value' as const,
      key: 'status',
      value: '無い',
      term: 'status:無い',
    };
    expect(taskQueryErrorsOf({ message: 'invalid-task-query', errors: [error] })).toEqual([error]);
    expect(describeTaskQueryError(error)).toBe('「無い」というステータスはありません');
  });
});

it('prioritizes exact values and matches usernames with @', () => {
  expect(valueCandidates('priority', 'CRITICAL', {})[0]?.value).toBe('critical');
  expect(
    valueCandidates('assignee', '@ALICE', { members: [{ id: '1', username: 'alice' }] })[0]?.value,
  ).toBe('alice');
  expect(
    valueCandidates('author', '@ALICE', { members: [{ id: '1', username: 'alice' }] })[0]?.value,
  ).toBe('alice');
});
it('keeps members starting with "me" while @me is typed, with @me first', () => {
  const values = valueCandidates('assignee', '@me', {
    members: [{ id: '1', username: 'meg' }],
  }).map((c) => c.value);
  expect(values.slice(0, 2)).toEqual(['@me', 'meg']);
});
