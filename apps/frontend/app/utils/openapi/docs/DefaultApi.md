# DefaultApi

All URIs are relative to *http://localhost*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**createPersonalToken**](DefaultApi.md#createpersonaltoken) | **POST** /v1/personal_tokens | パーソナルアクセストークンを発行 |
| [**getLabels**](DefaultApi.md#getlabels) | **GET** /v1/labels | ラベル一覧 |
| [**getPersonalToken**](DefaultApi.md#getpersonaltoken) | **GET** /v1/personal_tokens/{id} | 指定したトークンを参照 |
| [**login**](DefaultApi.md#loginoperation) | **POST** /v1/auth/login | ログイン |
| [**logout**](DefaultApi.md#logout) | **POST** /v1/auth/logout | ログアウト |
| [**me**](DefaultApi.md#me) | **GET** /v1/auth/me | ログイン中ユーザー情報 |
| [**register**](DefaultApi.md#registeroperation) | **POST** /v1/auth/register | 新規登録 |
| [**resendVerificationEmail**](DefaultApi.md#resendverificationemail) | **POST** /v1/auth/resend-verification-email | 認証メールの再送 |
| [**revokeAllPersonalTokens**](DefaultApi.md#revokeallpersonaltokens) | **DELETE** /v1/personal_tokens | すべての個人用トークンを取り消し |
| [**revokePersonalToken**](DefaultApi.md#revokepersonaltoken) | **DELETE** /v1/personal_tokens/{id} | 指定したトークンを取り消し |
| [**verifyEmail**](DefaultApi.md#verifyemailoperation) | **POST** /v1/auth/verify-email | メールアドレスの確認 |



## createPersonalToken

> CreatePersonalTokenResponse createPersonalToken(body)

パーソナルアクセストークンを発行

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { CreatePersonalTokenRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // object
    body: Object,
  } satisfies CreatePersonalTokenRequest;

  try {
    const data = await api.createPersonalToken(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **body** | `object` |  | |

### Return type

[**CreatePersonalTokenResponse**](CreatePersonalTokenResponse.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 発行したトークンの情報 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getLabels

> Array&lt;CrateEntitiesLabelsModel&gt; getLabels()

ラベル一覧

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { GetLabelsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  try {
    const data = await api.getLabels();
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters

This endpoint does not need any parameter.

### Return type

[**Array&lt;CrateEntitiesLabelsModel&gt;**](CrateEntitiesLabelsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | すべてのラベル |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getPersonalToken

> PersonalTokenResponse getPersonalToken(id)

指定したトークンを参照

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { GetPersonalTokenRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | トークンの識別子
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies GetPersonalTokenRequest;

  try {
    const data = await api.getPersonalToken(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **id** | `string` | トークンの識別子 | [Defaults to `undefined`] |

### Return type

[**PersonalTokenResponse**](PersonalTokenResponse.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | トークンの状態 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## login

> login(loginRequest)

ログイン

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { LoginOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // LoginRequest
    loginRequest: ...,
  } satisfies LoginOperationRequest;

  try {
    const data = await api.login(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **loginRequest** | [LoginRequest](LoginRequest.md) |  | |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **204** | ログインに成功しました（本文なし） |  -  |
| **401** | メールアドレスまたはパスワードが正しくありません |  -  |
| **403** | メールアドレスの確認が済んでいないためログインできません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## logout

> logout()

ログアウト

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { LogoutRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  try {
    const data = await api.logout();
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters

This endpoint does not need any parameter.

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **204** | ログアウトしました（本文なし） |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## me

> CrateEntitiesUsersModel me()

ログイン中ユーザー情報

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { MeRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  try {
    const data = await api.me();
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters

This endpoint does not need any parameter.

### Return type

[**CrateEntitiesUsersModel**](CrateEntitiesUsersModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 現在のアカウント情報 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## register

> string register(registerRequest)

新規登録

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { RegisterOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // RegisterRequest
    registerRequest: ...,
  } satisfies RegisterOperationRequest;

  try {
    const data = await api.register(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **registerRequest** | [RegisterRequest](RegisterRequest.md) |  | |

### Return type

**string**

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `text/plain`, `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **201** | アカウントが作成されました。続けて送信されたメールで認証してください。 |  -  |
| **409** | このメールアドレスはすでに登録されています |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## resendVerificationEmail

> string resendVerificationEmail(resendVerificationRequest)

認証メールの再送

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { ResendVerificationEmailRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // ResendVerificationRequest
    resendVerificationRequest: ...,
  } satisfies ResendVerificationEmailRequest;

  try {
    const data = await api.resendVerificationEmail(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **resendVerificationRequest** | [ResendVerificationRequest](ResendVerificationRequest.md) |  | |

### Return type

**string**

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `text/plain`, `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 認証メールを送信しました |  -  |
| **404** | 入力されたメールアドレスのアカウントが見つかりませんでした |  -  |
| **409** | このアカウントではメール認証はもう完了しています |  -  |
| **429** | しばらくしてから再度お試しください |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## revokeAllPersonalTokens

> Array&lt;PersonalTokenResponse&gt; revokeAllPersonalTokens()

すべての個人用トークンを取り消し

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { RevokeAllPersonalTokensRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  try {
    const data = await api.revokeAllPersonalTokens();
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters

This endpoint does not need any parameter.

### Return type

[**Array&lt;PersonalTokenResponse&gt;**](PersonalTokenResponse.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 現在アクティブなトークンの一覧（空になり得ます） |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## revokePersonalToken

> PersonalTokenResponse revokePersonalToken(id)

指定したトークンを取り消し

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { RevokePersonalTokenRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | トークンの識別子
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies RevokePersonalTokenRequest;

  try {
    const data = await api.revokePersonalToken(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **id** | `string` | トークンの識別子 | [Defaults to `undefined`] |

### Return type

[**PersonalTokenResponse**](PersonalTokenResponse.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 取り消し後の状態 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## verifyEmail

> string verifyEmail(verifyEmailRequest)

メールアドレスの確認

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { VerifyEmailOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // VerifyEmailRequest
    verifyEmailRequest: ...,
  } satisfies VerifyEmailOperationRequest;

  try {
    const data = await api.verifyEmail(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **verifyEmailRequest** | [VerifyEmailRequest](VerifyEmailRequest.md) |  | |

### Return type

**string**

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `text/plain`, `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | メールアドレスの確認が完了しました |  -  |
| **400** | 認証用リンクが無効か、または有効期限切れです |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

