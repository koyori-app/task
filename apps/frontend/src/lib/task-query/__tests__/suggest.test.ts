import { describe, it, expect } from 'vitest';
import { suggestionContext, keyCandidates, valueCandidates, applySuggestion } from '../suggest';
import { toggleFlag } from '../edit';
import { describeTaskQueryError, taskQueryErrorsOf } from '../errors';
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
  it('completes keys without committing, values with committing', () => {
    const context = suggestionContext('label:b', 7)!;
    const candidates = valueCandidates('label', 'b', {
      labels: [
        { id: '1', name: 'debug' },
        { id: '2', name: 'bug' },
      ],
    });
    expect(candidates[0]?.value).toBe('bug');
    expect(applySuggestion('label:b', context, candidates[0]!)).toEqual({
      draft: 'label:bug',
      commit: true,
    });
    expect(applySuggestion('la', suggestionContext('la', 2)!, keyCandidates('la')[0]!)).toEqual({
      draft: 'label:',
      commit: false,
    });
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
