export function arkMessage(msg: string): string {
  if (msg.includes('at least length 3')) return '3文字以上で入力してください。';
  if (msg.includes('email address')) return 'メールアドレスの形式が正しくありません。';
  if (msg.includes('at least length 8')) return '8文字以上で入力してください。';
  return msg;
}
