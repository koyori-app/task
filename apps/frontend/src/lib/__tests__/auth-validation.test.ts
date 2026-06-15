import { describe, it, expect } from 'vitest';
import { arkMessage } from '../auth-validation';

describe('arkMessage', () => {
  it('3文字未満エラーを日本語に変換する', () => {
    expect(arkMessage('Expected string to be at least length 3')).toBe('3文字以上で入力してください。');
  });

  it('メールアドレス形式エラーを日本語に変換する', () => {
    expect(arkMessage('Expected string to be a valid email address')).toBe('メールアドレスの形式が正しくありません。');
  });

  it('8文字未満エラーを日本語に変換する', () => {
    expect(arkMessage('Expected string to be at least length 8')).toBe('8文字以上で入力してください。');
  });

  it('未知のメッセージはそのまま返す', () => {
    expect(arkMessage('some unknown error')).toBe('some unknown error');
  });
});
