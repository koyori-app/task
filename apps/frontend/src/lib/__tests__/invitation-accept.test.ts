import { describe, expect, it } from 'vitest';
import { invitationAcceptReturnPath } from '../invitation-accept';

describe('invitationAcceptReturnPath', () => {
  it('承諾画面のパスだけを戻り先にする', () => {
    expect(invitationAcceptReturnPath('/invitations/accept?token=abc')).toBe(
      '/invitations/accept?token=abc',
    );
    expect(invitationAcceptReturnPath('https://evil.example/invitations/accept?')).toBeNull();
    expect(invitationAcceptReturnPath('/settings')).toBeNull();
    expect(invitationAcceptReturnPath(null)).toBeNull();
  });
});
