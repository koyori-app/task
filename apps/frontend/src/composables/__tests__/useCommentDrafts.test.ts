import { beforeEach, describe, expect, it } from 'vitest';

import {
  beginCommentDraftSend,
  getCommentDraft,
  isCommentDraftSending,
  resetCommentDrafts,
  setCommentDraft,
} from '../useCommentDrafts';

const USER = 'user-1';
const TASK = 't/p/ENG-1';

describe('useCommentDrafts の送信中の状態', () => {
  beforeEach(() => {
    resetCommentDrafts();
    setCommentDraft(USER, TASK, null, '送る本文');
  });

  it('始めると今の宛先だけが送信中になる', () => {
    beginCommentDraftSend(USER, TASK, null);
    expect(isCommentDraftSending(USER, TASK, null)).toBe(true);
    expect(isCommentDraftSending(USER, TASK, 'thread-1')).toBe(false);
    expect(isCommentDraftSending(USER, 't/p/ENG-2', null)).toBe(false);
  });

  it('成功で終えると、1 回の呼び出しの直後に送信中が解けて下書きが消えている', () => {
    const finish = beginCommentDraftSend(USER, TASK, null);
    finish(true);
    expect(isCommentDraftSending(USER, TASK, null)).toBe(false);
    expect(getCommentDraft(USER, TASK, null)).toBe('');
  });

  it('失敗で終えると、送信中が解けて下書きは残る', () => {
    const finish = beginCommentDraftSend(USER, TASK, null);
    finish(false);
    expect(isCommentDraftSending(USER, TASK, null)).toBe(false);
    expect(getCommentDraft(USER, TASK, null)).toBe('送る本文');
  });

  it('送信中に下書きが変わっていたら、成功でも消さない', () => {
    const finish = beginCommentDraftSend(USER, TASK, null);
    setCommentDraft(USER, TASK, null, '別の下書き');
    finish(true);
    expect(getCommentDraft(USER, TASK, null)).toBe('別の下書き');
  });

  it('リセットの後に同じ本文で始めた送信の下書きは、古い送信が成功で終わっても消さない', () => {
    const oldFinish = beginCommentDraftSend(USER, TASK, null);
    resetCommentDrafts();
    setCommentDraft(USER, TASK, null, '送る本文');
    const newFinish = beginCommentDraftSend(USER, TASK, null);

    oldFinish(true);
    expect(getCommentDraft(USER, TASK, null)).toBe('送る本文');
    expect(isCommentDraftSending(USER, TASK, null)).toBe(true);

    // 新しい送信が失敗すれば、下書きは残る
    newFinish(false);
    expect(getCommentDraft(USER, TASK, null)).toBe('送る本文');
  });

  it('リセットの後に同じ宛先で始めた送信の送信中は、古い送信を終えても解けない', () => {
    const oldFinish = beginCommentDraftSend(USER, TASK, null);
    resetCommentDrafts();
    expect(isCommentDraftSending(USER, TASK, null)).toBe(false);

    setCommentDraft(USER, TASK, null, '新しい本文');
    const newFinish = beginCommentDraftSend(USER, TASK, null);
    oldFinish(false);
    expect(isCommentDraftSending(USER, TASK, null)).toBe(true);

    newFinish(true);
    expect(isCommentDraftSending(USER, TASK, null)).toBe(false);
    expect(getCommentDraft(USER, TASK, null)).toBe('');
  });
});
