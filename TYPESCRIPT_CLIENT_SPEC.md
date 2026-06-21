# Valkyrin TypeScript Client: Architectural Specification

## 1. The Payload Architecture — Tripartite Decomposition

**IR Partitioning Rule:** For each entity node in the IR, split its fields into three disjoint buckets based on a "kind" discriminator:

- `scalars`: (kind = "scalar" | "enum") Plain object literal of primitive/enum types.
- `objects`: (kind = "object" AND NOT composite) Nested payload references (recursive).
- `composites`: (kind = "object" AND composite) Nested scalar-like object literal.

**Generated type skeleton:**

```typescript
type Payload_<ExtArgs> = {
  name: "EntityName"
  scalars: _GetPayloadResult<{ fieldA: string; fieldB: number }, ExtArgs['result']['entityName']>
  objects: { relationField: Payload_Related<ExtArgs> }
  composites: { nestedComposite: { subField: boolean } }
}

The scalars wrapper (_GetPayloadResult) is the extension seam. At the runtime library level:
TypeScript

type _GetPayloadResult<Base, R> = Omit<Base, _ExtensionKeys<R>> & _ExtensionObject<R>

This lets user-defined result extensions override scalar fields or add computed fields. Without extensions, R = {}, so Omit<Base, never> & {} collapses to Base.

Default (no-select) model type:
TypeScript

type EntityModel = _DefaultSelection<Payload_>

Which unwraps scalars & composites and applies any global/local omit via key-remapping:
TypeScript

type _DefaultSelection<P, Args, GlobalOmit> =
  Args extends { omit: infer Local }
    ? _ApplyOmit<_UnwrapPayload<P>, _Patch<Local, _ExtractGlobal<GlobalOmit, P['name']>>>
    : _ApplyOmit<_UnwrapPayload<P>, _ExtractGlobal<GlobalOmit, P['name']>>

The return type resolver dispatches by operation:
TypeScript

type _GetResult<P, Args, Op> = {
  findUnique:     _GetFindResult<P, Args> | null
  findMany:       _GetFindResult<P, Args>[]
  create:         _GetFindResult<P, Args>
}[Op]

Architectural rule: Never emit a "flat" model type directly. Always emit a structured payload and compute the flattened return type at the type level via a central generic resolver.
2. The Args Generation — Multi-Phase Builder Pattern

Architecture: For each (entity, operation) pair, the generator composes an args type by layering independent concerns:
TypeScript

type Args<ExtArgs> = {
  select?:  EntitySelect<ExtArgs> | null    // conditional: only if entity has fields
  include?: EntityInclude<ExtArgs> | null   // conditional: only if entity has relations
  omit?:    EntityOmit<ExtArgs> | null      // always added
  // ...schemaArgs (where, orderBy, data, etc.)
}

Schema arg type resolution rule (for each field in the IR):

    Look up the scalar type via a static mapping table (String → String, Int → number, etc.)

    If location = "enumTypes" AND namespace = "model" → reference $Enums.EnumName

    If location = "inputObjectTypes" AND isList = false → group with siblings via XOR

    If the type tree contains dynamic model references → add generic <T extends $ValkyrinModel = never> parameter.

XOR construction for mutually exclusive input objects:
TypeScript

type InputType = _XOR<{ optionA: string }, { optionB: number }> | string  | number

Note: XOR is an associative left-fold for mutually-exclusive union intersection.

Generic parameter inference: Pre-compute (via BFS on the input type graph at generation time) whether a type needs a $ValkyrinModel generic argument.
3. The Omit/Select Logic — Type-Level Projection Engine

This is composed of three independent type-level mechanisms:
3a. Select/Include Shape Types (generated per-entity)
TypeScript

// Generated: maps each field to boolean | nested-args
type EntitySelect<ExtArgs> = {
  fieldA?: boolean
  fieldB?: boolean
  relationField?: boolean | EntityFieldArgs<ExtArgs>  // only if relation
}

// Generated: maps each relation field only
type EntityInclude<ExtArgs> = {
  relationField?: boolean | EntityFieldArgs<ExtArgs>
}

// Generated (scalars + composites only):
type EntityOmit<ExtArgs> = {
  fieldA?: boolean
  fieldB?: boolean
}

3b. The Central Projection Engine
TypeScript

type _GetFindResult<P, A, GlobalOmit> =
  A extends any ? _DefaultSelection<P, A, GlobalOmit> :
  A extends { select: infer S } | { include: infer I }
    ? {
        [K in keyof (S & I) as (S & I)[K] extends false | undefined | null | Skip ? never : K]:
          // resolve nested type
      }
    : _DefaultSelection<P, A, GlobalOmit>

Key remapping is the filtering mechanism: [K in keyof (S & I) as (S & I)[K] extends false | undefined | null | Skip ? never : K]

    Keys set to false, null, or undefined are remapped to never → eliminated from the result.

    Keys set to true → resolved via scalar lookup.

    Keys set to a nested object → recurse with _GetFindResult<P, nestedArgs, GlobalOmit>.

3c. Omit Application

The omit field is independent of select/include. It applies in _DefaultSelection.
TypeScript

type _ApplyOmit<T, O> = {
  [K in keyof T as O[K] extends true ? never : K]: T[K]
}

When both select and include are present, the projected result is intersected with the default selection (minus omitted keys) to ensure that any fields not explicitly picked up by select/include still have the correct nullability.
TypeScript

& (A extends { include: any } ? _DefaultSelection<P, A & { omit: A['omit'] }, GlobalOmit> : unknown)
```
