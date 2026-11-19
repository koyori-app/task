import { describe, expect, it } from 'vitest';
import {
  assertionToJSON,
  base64urlToBuffer,
  bufferToBase64url,
  toRequestOptions,
} from '../webauthn';

const bytes = (...values: number[]) => new Uint8Array(values).buffer;

describe('base64url', () => {
  it('ArrayBuffer と行き来し、詰め物と + / を使わない', () => {
    const buffer = bytes(0xfb, 0xff, 0xbf, 0x00, 0x01);
    const encoded = bufferToBase64url(buffer);
    expect(encoded).toBe('-_-_AAE');
    expect(new Uint8Array(base64urlToBuffer(encoded))).toEqual(new Uint8Array(buffer));
  });

  it('詰め物のある形も読む', () => {
    expect(new Uint8Array(base64urlToBuffer('AQ=='))).toEqual(new Uint8Array([1]));
  });
});

describe('toRequestOptions', () => {
  it('チャレンジと許す鍵の id を ArrayBuffer にする', () => {
    const options = toRequestOptions({
      publicKey: {
        challenge: 'AQID',
        timeout: 60000,
        rpId: 'task.example',
        userVerification: 'preferred',
        allowCredentials: [{ type: 'public-key', id: 'BAUG', transports: ['internal'] }],
      },
    });
    expect(new Uint8Array(options.challenge as ArrayBuffer)).toEqual(new Uint8Array([1, 2, 3]));
    expect(options.rpId).toBe('task.example');
    expect(options.timeout).toBe(60000);
    expect(options.userVerification).toBe('preferred');
    const [allowed] = options.allowCredentials ?? [];
    expect(new Uint8Array(allowed!.id as ArrayBuffer)).toEqual(new Uint8Array([4, 5, 6]));
    expect(allowed!.transports).toEqual(['internal']);
  });
});

describe('assertionToJSON', () => {
  it('署名の応答を backend の形（base64url）にする', () => {
    const credential = {
      id: 'BAUG',
      rawId: bytes(4, 5, 6),
      type: 'public-key',
      response: {
        authenticatorData: bytes(1),
        clientDataJSON: bytes(2),
        signature: bytes(3),
        userHandle: null,
      },
    } as unknown as PublicKeyCredential;
    expect(assertionToJSON(credential)).toEqual({
      id: 'BAUG',
      rawId: 'BAUG',
      type: 'public-key',
      response: {
        authenticatorData: 'AQ',
        clientDataJSON: 'Ag',
        signature: 'Aw',
        userHandle: null,
      },
      extensions: {},
    });
  });
});
