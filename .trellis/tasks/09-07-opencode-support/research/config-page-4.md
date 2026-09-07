[Skip to content](#start-of-content)   
 

## Navigation Menu

[Sign in](/login?return_to=https%3A%2F%2Fgithub.com%2Fanomalyco%2Fopencode%2Fblob%2Fdev%2Fpackages%2Fopencode%2Fsrc%2Fauth%2Findex.ts)

Appearance settings

[Sign in](/login?return_to=https%3A%2F%2Fgithub.com%2Fanomalyco%2Fopencode%2Fblob%2Fdev%2Fpackages%2Fopencode%2Fsrc%2Fauth%2Findex.ts)

[Sign up](/signup?ref_cta=Sign+up&ref_loc=header+logged+out&ref_page=%2F%3Cuser-name%3E%2F%3Crepo-name%3E%2Fblob%2Fshow&source=header-repo&source_repo=anomalyco%2Fopencode)

Appearance settings

You signed in with another tab or window. Reload to refresh your session. You signed out in another tab or window. Reload to refresh your session. You switched accounts on another tab or window. Reload to refresh your session. Dismiss alert

{{ message }}

### Uh oh!

There was an error while loading. Please reload this page.

[anomalyco](/anomalyco)   /  **[opencode](/anomalyco/opencode)**  Public

* [Notifications](/login?return_to=%2Fanomalyco%2Fopencode)  You must be signed in to change notification settings
* [Fork 26.8k](/login?return_to=%2Fanomalyco%2Fopencode)
* [Star  205k](/login?return_to=%2Fanomalyco%2Fopencode)

## Expand file tree

/

# index.ts

Copy path

More file actions

More file actions

## Latest commit

## History

[History](/anomalyco/opencode/commits/dev/packages/opencode/src/auth/index.ts)

History

97 lines (78 loc) · 3.37 KB

/

# index.ts

Copy path

## File metadata and controls

97 lines (78 loc) · 3.37 KB

[Raw](https://github.com/anomalyco/opencode/raw/refs/heads/dev/packages/opencode/src/auth/index.ts)

Copy raw file

Download raw file

Open symbols panel

Edit and raw actions

1

2

3

4

5

6

7

8

9

10

11

12

13

14

15

16

17

18

19

20

21

22

23

24

25

26

27

28

29

30

31

32

33

34

35

36

37

38

39

40

41

42

43

44

45

46

47

48

49

50

51

52

53

54

55

56

57

58

59

60

61

62

63

64

65

66

67

68

69

70

71

72

73

74

75

76

77

78

79

80

81

82

83

84

85

86

87

88

89

90

91

92

93

94

95

96

97

import { LayerNode } from "@opencode-ai/core/effect/layer-node"

import path from "path"

import { Effect, Layer, Record, Result, Schema, Context } from "effect"

import { NonNegativeInt } from "@opencode-ai/core/schema"

import { Global } from "@opencode-ai/core/global"

import { FSUtil } from "@opencode-ai/core/fs-util"

export const OAUTH\_DUMMY\_KEY = "opencode-oauth-dummy-key"

const file = path.join(Global.Path.data, "auth.json")

const fail = (message: string) => (cause: unknown) => new AuthError({ message, cause })

export class Oauth extends Schema.Class<Oauth>("OAuth")({

type: Schema.Literal("oauth"),

refresh: Schema.String,

access: Schema.String,

expires: NonNegativeInt,

accountId: Schema.optional(Schema.String),

enterpriseUrl: Schema.optional(Schema.String),

}) {}

export class Api extends Schema.Class<Api>("ApiAuth")({

type: Schema.Literal("api"),

key: Schema.String,

metadata: Schema.optional(Schema.Record(Schema.String, Schema.String)),

}) {}

export class WellKnown extends Schema.Class<WellKnown>("WellKnownAuth")({

type: Schema.Literal("wellknown"),

key: Schema.String,

token: Schema.String,

}) {}

export const Info = Schema.Union([Oauth, Api, WellKnown]).annotate({ discriminator: "type", identifier: "Auth" })

export type Info = Schema.Schema.Type<typeof Info>

export class AuthError extends Schema.TaggedErrorClass<AuthError>()("AuthError", {

message: Schema.String,

cause: Schema.optional(Schema.Defect()),

}) {}

export interface Interface {

readonly get: (providerID: string) => Effect.Effect<Info | undefined, AuthError>

readonly all: () => Effect.Effect<Record<string, Info>, AuthError>

readonly set: (key: string, info: Info) => Effect.Effect<void, AuthError>

readonly remove: (key: string) => Effect.Effect<void, AuthError>

export class Service extends Context.Service<Service, Interface>()("@opencode/Auth") {}

const layer = Layer.effect(

Service,

Effect.gen(function\* () {

const fsys = yield\* FSUtil.Service

const decode = Schema.decodeUnknownOption(Info)

const all = Effect.fn("Auth.all")(function\* () {

if (process.env.OPENCODE\_AUTH\_CONTENT) {

try {

return JSON.parse(process.env.OPENCODE\_AUTH\_CONTENT)

} catch (err) {}

}

const data = (yield\* fsys.readJson(file).pipe(Effect.orElseSucceed(() => ({})))) as Record<string, unknown>

return Record.filterMap(data, (value) => Result.fromOption(decode(value), () => undefined))

})

const get = Effect.fn("Auth.get")(function\* (providerID: string) {

return (yield\* all())[providerID]

})

const set = Effect.fn("Auth.set")(function\* (key: string, info: Info) {

const norm = key.replace(/\/+$/, "")

const data = yield\* all()

if (norm !== key) delete data[key]

delete data[norm + "/"]

yield\* fsys

.writeJson(file, { ...data, [norm]: info }, 0o600)

.pipe(Effect.mapError(fail("Failed to write auth data")))

})

const remove = Effect.fn("Auth.remove")(function\* (key: string) {

const norm = key.replace(/\/+$/, "")

const data = yield\* all()

delete data[key]

delete data[norm]

yield\* fsys.writeJson(file, data, 0o600).pipe(Effect.mapError(fail("Failed to write auth data")))

})

return Service.of({ get, all, set, remove })

}),

export const node = LayerNode.make({ service: Service, layer: layer, deps: [FSUtil.node] })

export \* as Auth from "."

You can’t perform that action at this time.

 
