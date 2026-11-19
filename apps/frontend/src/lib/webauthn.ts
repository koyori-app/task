/**
 * WebAuthn の JSON と、ブラウザの API が取る ArrayBuffer を詰め替える。
 *
 * backend（webauthn-rs）はチャレンジと資格情報を base64url の JSON でやり取りする。
 * `navigator.credentials.get` は ArrayBuffer を取り、ArrayBuffer を返すので、その間を埋める。
 * `PublicKeyCredential.parseRequestOptionsFromJSON` は古いブラウザに無いので使わない。
 */

export function base64urlToBuffer(value: string): ArrayBuffer {
  const base64 = value.replace(/-/g, '+').replace(/_/g, '/');
  const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

export function bufferToBase64url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

type CredentialDescriptorJSON = { type: string; id: string; transports?: string[] };

/** `POST /v1/auth/reauth/passkey/start` が返す形（webauthn-rs の RequestChallengeResponse）。 */
export type RequestChallengeJSON = {
  publicKey: {
    challenge: string;
    timeout?: number;
    rpId?: string;
    allowCredentials?: CredentialDescriptorJSON[];
    userVerification?: UserVerificationRequirement;
  };
};

export function toRequestOptions(json: RequestChallengeJSON): PublicKeyCredentialRequestOptions {
  const { publicKey } = json;
  return {
    challenge: base64urlToBuffer(publicKey.challenge),
    timeout: publicKey.timeout,
    rpId: publicKey.rpId,
    userVerification: publicKey.userVerification,
    allowCredentials: publicKey.allowCredentials?.map((credential) => ({
      type: credential.type as PublicKeyCredentialType,
      id: base64urlToBuffer(credential.id),
      transports: credential.transports as AuthenticatorTransport[] | undefined,
    })),
  };
}

/** `POST /v1/auth/reauth/passkey/finish` の `credential` に渡す形。 */
export function assertionToJSON(credential: PublicKeyCredential) {
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    id: credential.id,
    rawId: bufferToBase64url(credential.rawId),
    type: credential.type,
    response: {
      authenticatorData: bufferToBase64url(response.authenticatorData),
      clientDataJSON: bufferToBase64url(response.clientDataJSON),
      signature: bufferToBase64url(response.signature),
      userHandle: response.userHandle ? bufferToBase64url(response.userHandle) : null,
    },
    extensions: {},
  };
}

/** このブラウザでパスキーを使えるか。 */
export function isWebAuthnAvailable(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.PublicKeyCredential !== 'undefined' &&
    typeof navigator.credentials?.get === 'function'
  );
}

/** ブラウザの認証器でパスキーを選ばせ、backend に渡す形にして返す。取り消すと例外を投げる。 */
export async function getPasskeyAssertion(json: RequestChallengeJSON) {
  const credential = await navigator.credentials.get({ publicKey: toRequestOptions(json) });
  if (!credential) throw new DOMException('no credential', 'NotAllowedError');
  return assertionToJSON(credential as PublicKeyCredential);
}
