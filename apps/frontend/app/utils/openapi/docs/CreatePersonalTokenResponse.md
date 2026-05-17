
# CreatePersonalTokenResponse

PAT 作成時のレスポンス（平文トークンはこの応答でのみ返却）

## Properties

Name | Type
------------ | -------------
`expiresAt` | Date
`id` | string
`lastUsedAt` | Date
`name` | string
`revoked` | boolean
`scopes` | [Array&lt;Scope&gt;](Scope.md)
`token` | string
`tokenLastFour` | string
`userId` | string

## Example

```typescript
import type { CreatePersonalTokenResponse } from ''

// TODO: Update the object below with actual values
const example = {
  "expiresAt": null,
  "id": null,
  "lastUsedAt": null,
  "name": null,
  "revoked": null,
  "scopes": null,
  "token": null,
  "tokenLastFour": null,
  "userId": null,
} satisfies CreatePersonalTokenResponse

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as CreatePersonalTokenResponse
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


