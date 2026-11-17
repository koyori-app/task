// テストケースの一覧（docs/dev/features/account-settings.md から移した）
// - アバターの URL 入力欄を出さないこと、画面からは `avatar_url` を送らないこと
// - 「削除」はプレビューを頭文字に戻し、保存で `clear_avatar_url` を送ること（失敗時は未保存のまま残る、
//   後から画像を選ぶと取り消される、アバターが無ければ押せない）
// - ユーザー名の前後の空白を取り除くこと、ユーザー名長を送信前に止めること、失敗時（400 / それ以外）の表示
// - 保存後に編集を再開すると保存済み表示が消えること
// - 画像アップロードの成功で表示・認証ストアが更新されること、その後の保存で `avatar_url` を送り直さないこと
// - 2 MiB 超を送る前に止めること（ちょうどは送る）、413 / 400 / それ以外の表示

import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import ProfileForm from '../ProfileForm.vue';
import { useAuthStore } from '@/stores/auth';
import type { components } from '@/generated/api';

const user: components['schemas']['UserResponse'] = {
  id: '00000000-0000-0000-0000-000000000001',
  username: 'tester',
  email: 'tester@example.com',
  email_verified: true,
  is_admin: false,
  is_suspended: false,
  totp_enabled: false,
  has_password: true,
  bio: '最初の説明',
  avatar_url: 'https://example.com/a.png',
};

/** PATCH /v1/auth/me に届いたリクエストボディを記録する。 */
function stubPatchMe(status = 200) {
  const bodies: unknown[] = [];
  const fetchMock = vi.fn(async (req: Request) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    if (pathname.endsWith('/v1/auth/me') && req.method === 'PATCH') {
      bodies.push(await req.clone().json());
      return new Response(JSON.stringify(user), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    }
    return new Response(JSON.stringify({ message: 'not-found' }), { status: 404 });
  });
  vi.stubGlobal('fetch', fetchMock);
  return bodies;
}

/** POST /v1/auth/me/avatar を受け、送られた FormData を記録する。 */
function stubUploadAvatar(status = 200, response: unknown = user) {
  const uploads: FormData[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = input instanceof Request ? input.url : String(input);
    if (url.endsWith('/v1/auth/me/avatar') && init?.method === 'POST') {
      uploads.push(init.body as FormData);
      return new Response(JSON.stringify(response), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    }
    return new Response(JSON.stringify({ message: 'not-found' }), { status: 404 });
  });
  vi.stubGlobal('fetch', fetchMock);
  return uploads;
}

async function chooseFile(wrapper: ReturnType<typeof mountForm>, file: File) {
  const input = wrapper.find('[data-testid="avatar-file-input"]');
  Object.defineProperty(input.element, 'files', { value: [file], configurable: true });
  await input.trigger('change');
  await flushPromises();
}

function findButton(wrapper: ReturnType<typeof mountForm>, label: string) {
  const button = wrapper.findAll('button').find((b) => b.text() === label);
  if (!button) throw new Error(`button not found: ${label}`);
  return button;
}

function mountForm(profileUser = user) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ProfileForm, {
    props: { user: profileUser },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

enableAutoUnmount(afterEach);

beforeEach(() => {
  setActivePinia(createPinia());
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ProfileForm', () => {
  it('アバターの URL 入力欄を出さない', async () => {
    stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    expect(wrapper.find('input[type="url"]').exists()).toBe(false);
    expect(wrapper.find('#avatarUrl').exists()).toBe(false);
    expect(wrapper.find('#avatar-description').text()).not.toContain('URL');
  });

  it('何も変えずに保存すると avatar_url も clear_avatar_url も送らない', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toEqual({ username: 'tester', bio: '最初の説明' });
  });

  it('ユーザー名と自己紹介を変えて保存するとその 2 つだけを送る', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('#username').setValue('renamed');
    await wrapper.find('#bio').setValue('新しい説明');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies[0]).toEqual({ username: 'renamed', bio: '新しい説明' });
  });

  it('アバターの無い利用者では「削除」が無効', async () => {
    stubPatchMe();
    const wrapper = mountForm({ ...user, avatar_url: null });
    await flushPromises();

    expect(findButton(wrapper, '削除').attributes('disabled')).toBeDefined();
  });

  it('「削除」を押すとプレビューが頭文字になり、保存で clear_avatar_url を送る', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await findButton(wrapper, '削除').trigger('click');
    await flushPromises();

    expect(bodies).toHaveLength(0);
    expect(wrapper.find('img').exists()).toBe(false);

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({ clear_avatar_url: true });
    expect(bodies[0]).not.toHaveProperty('avatar_url');
    expect(findButton(wrapper, '削除').attributes('disabled')).toBeDefined();
  });

  it('「削除」の後に画像をアップロードすると削除が取り消される', async () => {
    const uploadedUrl = 'https://task.example.com/v1/avatars/avatar-new.png';
    stubUploadAvatar(200, { ...user, avatar_url: uploadedUrl });
    const wrapper = mountForm();
    await flushPromises();

    await findButton(wrapper, '削除').trigger('click');
    await chooseFile(wrapper, new File([new Uint8Array([0x89])], 'me.png'));

    expect(wrapper.find('img').attributes('src')).toBe(uploadedUrl);

    const bodies = stubPatchMe();
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).not.toHaveProperty('clear_avatar_url');
    expect(bodies[0]).not.toHaveProperty('avatar_url');
  });

  it('保存に失敗したら削除は未保存のまま残る', async () => {
    const failed = stubPatchMe(500);
    const wrapper = mountForm();
    await flushPromises();

    await findButton(wrapper, '削除').trigger('click');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(failed).toHaveLength(1);
    expect(wrapper.text()).toContain('保存できませんでした。');
    expect(wrapper.find('img').exists()).toBe(false);

    const bodies = stubPatchMe();
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({ clear_avatar_url: true });
  });

  it('既存の HTTP アバターを変更しなければ他の項目を保存できる', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm({ ...user, avatar_url: 'http://example.com/legacy.png' });
    await flushPromises();

    await wrapper.find('#bio').setValue('新しい説明');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.find('img').attributes('src')).toBe('http://example.com/legacy.png');
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({ bio: '新しい説明' });
    expect(bodies[0]).not.toHaveProperty('avatar_url');
    expect(bodies[0]).not.toHaveProperty('clear_avatar_url');
  });

  it('3 文字未満のユーザー名は送信せずエラーを出す', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('#username').setValue('ab');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(0);
    expect(wrapper.text()).toContain('3文字以上で入力してください。');
  });

  it('前後の空白は取り除いて送る', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('#username').setValue('  renamed  ');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({ username: 'renamed' });
  });

  it('空白だけのユーザー名は送信しない', async () => {
    const bodies = stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('#username').setValue('   ');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(bodies).toHaveLength(0);
    expect(wrapper.text()).toContain('3文字以上で入力してください。');
  });

  it('保存に失敗したらエラーを表示する', async () => {
    stubPatchMe(500);
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.text()).toContain('保存できませんでした。');

    await wrapper.find('#bio').setValue('再編集');
    await flushPromises();

    expect(wrapper.text()).not.toContain('保存できませんでした。');
  });

  it('400 のときは入力内容の確認を促す', async () => {
    stubPatchMe(400);
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.text()).toContain('入力内容を確認してください。');
  });

  it('保存後に編集を再開すると「保存しました。」が消える', async () => {
    stubPatchMe();
    const wrapper = mountForm();
    await flushPromises();

    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.text()).toContain('保存しました。');

    await wrapper.find('#bio').setValue('編集を再開する');
    await flushPromises();

    expect(wrapper.text()).not.toContain('保存しました。');
  });

  describe('アバター画像のアップロード', () => {
    const uploadedUrl =
      'https://task.example.com/v1/avatars/avatar-0f8fad5b-d9cb-469f-a165-70867728950e.png';

    it('成功するとプレビュー・認証ストアを新しい URL にする', async () => {
      const uploads = stubUploadAvatar(200, { ...user, avatar_url: uploadedUrl });
      const wrapper = mountForm();
      await flushPromises();

      const file = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'me.png', {
        type: 'image/png',
      });
      await chooseFile(wrapper, file);

      expect(uploads).toHaveLength(1);
      expect(uploads[0]!.get('file')).toBeInstanceOf(File);
      expect(wrapper.find('img').attributes('src')).toBe(uploadedUrl);
      expect(useAuthStore().user?.avatar_url).toBe(uploadedUrl);
      expect(wrapper.text()).toContain('保存しました。');
    });

    it('アップロード後の保存では avatar_url を送り直さない', async () => {
      stubUploadAvatar(200, { ...user, avatar_url: uploadedUrl });
      const wrapper = mountForm();
      await flushPromises();
      await chooseFile(wrapper, new File([new Uint8Array([0x89])], 'me.png'));

      const bodies = stubPatchMe();
      await wrapper.find('#bio').setValue('新しい説明');
      await wrapper.find('form').trigger('submit');
      await flushPromises();

      expect(bodies).toHaveLength(1);
      expect(bodies[0]).not.toHaveProperty('avatar_url');
      expect(bodies[0]).not.toHaveProperty('clear_avatar_url');
    });

    it('2 MiB を超える画像は送らずにエラーを出す', async () => {
      const uploads = stubUploadAvatar();
      const wrapper = mountForm();
      await flushPromises();

      await chooseFile(wrapper, new File([new Uint8Array(2 * 1024 * 1024 + 1)], 'big.png'));

      expect(uploads).toHaveLength(0);
      expect(wrapper.text()).toContain('画像は 2 MiB 以下にしてください。');
    });

    it('2 MiB ちょうどは送る', async () => {
      const uploads = stubUploadAvatar();
      const wrapper = mountForm();
      await flushPromises();

      await chooseFile(wrapper, new File([new Uint8Array(2 * 1024 * 1024)], 'exact.png'));

      expect(uploads).toHaveLength(1);
    });

    it.each([
      [413, '画像は 2 MiB 以下にしてください。'],
      [400, 'PNG・JPEG・GIF・WebP の画像を選んでください。'],
      [500, 'アップロードできませんでした。'],
    ])('%i のときはエラーを出し、プレビューは変えない', async (status, message) => {
      stubUploadAvatar(status, { message: 'error' });
      const wrapper = mountForm();
      await flushPromises();

      await chooseFile(wrapper, new File([new Uint8Array([0x89])], 'me.png'));

      expect(wrapper.text()).toContain(message);
      expect(wrapper.find('img').attributes('src')).toBe(user.avatar_url);
      expect(useAuthStore().user).toBeNull();
    });
  });
});
