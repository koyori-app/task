
# CrateEntitiesProjectMembersModel


## Properties

Name | Type
------------ | -------------
`id` | string
`projectId` | string
`role` | [ProjectRole](ProjectRole.md)
`userId` | string

## Example

```typescript
import type { CrateEntitiesProjectMembersModel } from ''

// TODO: Update the object below with actual values
const example = {
  "id": null,
  "projectId": null,
  "role": null,
  "userId": null,
} satisfies CrateEntitiesProjectMembersModel

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as CrateEntitiesProjectMembersModel
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


