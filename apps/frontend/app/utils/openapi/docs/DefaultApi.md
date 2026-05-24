# DefaultApi

All URIs are relative to *http://localhost*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**addMember**](DefaultApi.md#addmemberoperation) | **POST** /v1/tenants/{tenant_id}/projects/{project_id}/members | プロジェクトメンバーを追加 |
| [**createPersonalToken**](DefaultApi.md#createpersonaltoken) | **POST** /v1/personal_tokens | パーソナルアクセストークンを発行 |
| [**createProject**](DefaultApi.md#createprojectoperation) | **POST** /v1/tenants/{tenant_id}/projects | プロジェクトを作成 |
| [**createTenant**](DefaultApi.md#createtenantoperation) | **POST** /v1/tenants | テナントを作成 |
| [**deleteProject**](DefaultApi.md#deleteproject) | **DELETE** /v1/tenants/{tenant_id}/projects/{id} | プロジェクトを削除 |
| [**deleteTenant**](DefaultApi.md#deletetenant) | **DELETE** /v1/tenants/{id} | テナントを削除 |
| [**getLabels**](DefaultApi.md#getlabels) | **GET** /v1/labels | ラベル一覧 |
| [**getPersonalToken**](DefaultApi.md#getpersonaltoken) | **GET** /v1/personal_tokens/{id} | 指定したトークンを参照 |
| [**getProject**](DefaultApi.md#getproject) | **GET** /v1/tenants/{tenant_id}/projects/{id} | プロジェクトを取得 |
| [**getTenant**](DefaultApi.md#gettenant) | **GET** /v1/tenants/{id} | テナントを取得 |
| [**listMembers**](DefaultApi.md#listmembers) | **GET** /v1/tenants/{tenant_id}/projects/{project_id}/members | プロジェクトメンバー一覧 |
| [**listProjects**](DefaultApi.md#listprojects) | **GET** /v1/tenants/{tenant_id}/projects | プロジェクト一覧 |
| [**listTenants**](DefaultApi.md#listtenants) | **GET** /v1/tenants | 自分のテナント一覧 |
| [**login**](DefaultApi.md#loginoperation) | **POST** /v1/auth/login | ログイン |
| [**logout**](DefaultApi.md#logout) | **POST** /v1/auth/logout | ログアウト |
| [**me**](DefaultApi.md#me) | **GET** /v1/auth/me | ログイン中ユーザー情報 |
| [**register**](DefaultApi.md#registeroperation) | **POST** /v1/auth/register | 新規登録 |
| [**removeMember**](DefaultApi.md#removemember) | **DELETE** /v1/tenants/{tenant_id}/projects/{project_id}/members/{user_id} | プロジェクトメンバーを削除 |
| [**resendVerificationEmail**](DefaultApi.md#resendverificationemail) | **POST** /v1/auth/resend-verification-email | 認証メールの再送 |
| [**revokeAllPersonalTokens**](DefaultApi.md#revokeallpersonaltokens) | **DELETE** /v1/personal_tokens | すべての個人用トークンを取り消し |
| [**revokePersonalToken**](DefaultApi.md#revokepersonaltoken) | **DELETE** /v1/personal_tokens/{id} | 指定したトークンを取り消し |
| [**updateMember**](DefaultApi.md#updatememberoperation) | **PUT** /v1/tenants/{tenant_id}/projects/{project_id}/members/{user_id} | プロジェクトメンバーの権限を変更 |
| [**updateProject**](DefaultApi.md#updateprojectoperation) | **PUT** /v1/tenants/{tenant_id}/projects/{id} | プロジェクトを更新 |
| [**updateTenant**](DefaultApi.md#updatetenantoperation) | **PUT** /v1/tenants/{id} | テナントを更新 |
| [**verifyEmail**](DefaultApi.md#verifyemailoperation) | **POST** /v1/auth/verify-email | メールアドレスの確認 |



## addMember

> CrateEntitiesProjectMembersModel addMember(tenantId, projectId, addMemberRequest)

プロジェクトメンバーを追加

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { AddMemberOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    projectId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // AddMemberRequest
    addMemberRequest: ...,
  } satisfies AddMemberOperationRequest;

  try {
    const data = await api.addMember(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **projectId** | `string` | プロジェクトID | [Defaults to `undefined`] |
| **addMemberRequest** | [AddMemberRequest](AddMemberRequest.md) |  | |

### Return type

[**CrateEntitiesProjectMembersModel**](CrateEntitiesProjectMembersModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **201** | 追加されたメンバー |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


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


## createProject

> CrateEntitiesProjectsModel createProject(tenantId, createProjectRequest)

プロジェクトを作成

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { CreateProjectOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // CreateProjectRequest
    createProjectRequest: ...,
  } satisfies CreateProjectOperationRequest;

  try {
    const data = await api.createProject(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **createProjectRequest** | [CreateProjectRequest](CreateProjectRequest.md) |  | |

### Return type

[**CrateEntitiesProjectsModel**](CrateEntitiesProjectsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **201** | 作成されたプロジェクト |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## createTenant

> CrateEntitiesTenantsModel createTenant(createTenantRequest)

テナントを作成

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { CreateTenantOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // CreateTenantRequest
    createTenantRequest: ...,
  } satisfies CreateTenantOperationRequest;

  try {
    const data = await api.createTenant(body);
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
| **createTenantRequest** | [CreateTenantRequest](CreateTenantRequest.md) |  | |

### Return type

[**CrateEntitiesTenantsModel**](CrateEntitiesTenantsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **201** | 作成されたテナント |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## deleteProject

> deleteProject(tenantId, id)

プロジェクトを削除

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { DeleteProjectRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies DeleteProjectRequest;

  try {
    const data = await api.deleteProject(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **id** | `string` | プロジェクトID | [Defaults to `undefined`] |

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
| **204** | 削除しました |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## deleteTenant

> deleteTenant(id)

テナントを削除

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { DeleteTenantRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies DeleteTenantRequest;

  try {
    const data = await api.deleteTenant(body);
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
| **id** | `string` | テナントID | [Defaults to `undefined`] |

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
| **204** | 削除しました |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
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


## getProject

> CrateEntitiesProjectsModel getProject(tenantId, id)

プロジェクトを取得

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { GetProjectRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies GetProjectRequest;

  try {
    const data = await api.getProject(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **id** | `string` | プロジェクトID | [Defaults to `undefined`] |

### Return type

[**CrateEntitiesProjectsModel**](CrateEntitiesProjectsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | プロジェクト情報 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getTenant

> CrateEntitiesTenantsModel getTenant(id)

テナントを取得

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { GetTenantRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies GetTenantRequest;

  try {
    const data = await api.getTenant(body);
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
| **id** | `string` | テナントID | [Defaults to `undefined`] |

### Return type

[**CrateEntitiesTenantsModel**](CrateEntitiesTenantsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | テナント情報 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listMembers

> Array&lt;CrateEntitiesProjectMembersModel&gt; listMembers(tenantId, projectId)

プロジェクトメンバー一覧

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { ListMembersRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    projectId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies ListMembersRequest;

  try {
    const data = await api.listMembers(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **projectId** | `string` | プロジェクトID | [Defaults to `undefined`] |

### Return type

[**Array&lt;CrateEntitiesProjectMembersModel&gt;**](CrateEntitiesProjectMembersModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | メンバー一覧 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listProjects

> Array&lt;CrateEntitiesProjectsModel&gt; listProjects(tenantId)

プロジェクト一覧

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { ListProjectsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies ListProjectsRequest;

  try {
    const data = await api.listProjects(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |

### Return type

[**Array&lt;CrateEntitiesProjectsModel&gt;**](CrateEntitiesProjectsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | プロジェクト一覧 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listTenants

> Array&lt;CrateEntitiesTenantsModel&gt; listTenants()

自分のテナント一覧

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { ListTenantsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  try {
    const data = await api.listTenants();
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

[**Array&lt;CrateEntitiesTenantsModel&gt;**](CrateEntitiesTenantsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | テナント一覧 |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
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
| **503** | 認証メールの送信準備に失敗しました。アカウントは作成済みのため、認証メールの再送をお試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## removeMember

> removeMember(tenantId, projectId, userId)

プロジェクトメンバーを削除

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { RemoveMemberRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    projectId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | ユーザーID
    userId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
  } satisfies RemoveMemberRequest;

  try {
    const data = await api.removeMember(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **projectId** | `string` | プロジェクトID | [Defaults to `undefined`] |
| **userId** | `string` | ユーザーID | [Defaults to `undefined`] |

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
| **204** | 削除しました |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
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
| **503** | 認証メールの送信準備に失敗しました。しばらくしてから再送をお試しください |  -  |

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


## updateMember

> CrateEntitiesProjectMembersModel updateMember(tenantId, projectId, userId, updateMemberRequest)

プロジェクトメンバーの権限を変更

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { UpdateMemberOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    projectId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | ユーザーID
    userId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // UpdateMemberRequest
    updateMemberRequest: ...,
  } satisfies UpdateMemberOperationRequest;

  try {
    const data = await api.updateMember(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **projectId** | `string` | プロジェクトID | [Defaults to `undefined`] |
| **userId** | `string` | ユーザーID | [Defaults to `undefined`] |
| **updateMemberRequest** | [UpdateMemberRequest](UpdateMemberRequest.md) |  | |

### Return type

[**CrateEntitiesProjectMembersModel**](CrateEntitiesProjectMembersModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 更新後のメンバー |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## updateProject

> CrateEntitiesProjectsModel updateProject(tenantId, id, updateProjectRequest)

プロジェクトを更新

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { UpdateProjectOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    tenantId: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // string | プロジェクトID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // UpdateProjectRequest
    updateProjectRequest: ...,
  } satisfies UpdateProjectOperationRequest;

  try {
    const data = await api.updateProject(body);
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
| **tenantId** | `string` | テナントID | [Defaults to `undefined`] |
| **id** | `string` | プロジェクトID | [Defaults to `undefined`] |
| **updateProjectRequest** | [UpdateProjectRequest](UpdateProjectRequest.md) |  | |

### Return type

[**CrateEntitiesProjectsModel**](CrateEntitiesProjectsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 更新後のプロジェクト |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
| **500** | サーバー側で問題が発生しました。時間をおいて再度お試しください |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## updateTenant

> CrateEntitiesTenantsModel updateTenant(id, updateTenantRequest)

テナントを更新

### Example

```ts
import {
  Configuration,
  DefaultApi,
} from '';
import type { UpdateTenantOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new DefaultApi();

  const body = {
    // string | テナントID
    id: 38400000-8cf0-11bd-b23e-10b96e4ef00d,
    // UpdateTenantRequest
    updateTenantRequest: ...,
  } satisfies UpdateTenantOperationRequest;

  try {
    const data = await api.updateTenant(body);
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
| **id** | `string` | テナントID | [Defaults to `undefined`] |
| **updateTenantRequest** | [UpdateTenantRequest](UpdateTenantRequest.md) |  | |

### Return type

[**CrateEntitiesTenantsModel**](CrateEntitiesTenantsModel.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | 更新後のテナント |  -  |
| **401** | ログインまたはセッションが必要です |  -  |
| **403** | この操作は許可されていません |  -  |
| **404** | リソースが見つかりません |  -  |
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

