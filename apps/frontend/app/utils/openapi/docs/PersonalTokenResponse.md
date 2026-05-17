
# PersonalTokenResponse

PAT のメタデータ（平文トークン・ハッシュは含まない）

## Properties

Name | Type
------------ | -------------
`expiresAt` | Date
`id` | string
`lastUsedAt` | Date
`name` | string
`revoked` | boolean
`scopes` | [Array&lt;Scope&gt;](Scope.md)
`tokenLastFour` | string
`userId` | string

## Example

```typescript
import type { PersonalTokenResponse } from ''

// TODO: Update the object below with actual values
const example = {
  "expiresAt": null,
  "id": null,
  "lastUsedAt": null,
  "name": null,
  "revoked": null,
  "scopes": null,
  "tokenLastFour": null,
  "userId": null,
} satisfies PersonalTokenResponse

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as PersonalTokenResponse
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


