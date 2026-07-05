# Shared Layer Boundary

## Scope

`src/shared` is the first stable import surface for code that is intentionally cross-feature. It exposes small barrels only; PR-2 does not move existing implementation files from `src/components`, `src/lib`, or `src/types`.

## Public Entries

| Entry | Current source | Rule |
| --- | --- | --- |
| `src/shared/ui` | `src/components/ui` | Re-export reusable UI primitives. Keep feature-specific composition out of this layer. |
| `src/shared/lib` | `src/lib/utils`, `src/lib/phase1Capabilities` | Re-export generic helpers and Phase 1 capability gates. Avoid domain workflows here. |
| `src/shared/hooks` | `src/lib/hooks` | Re-export hooks that are reusable across feature boundaries. App or assistant event hooks stay outside shared. |
| `src/shared/types` | `src/types` | Re-export shared app data contracts as type-only exports. |

## Review Checklist

- [x] Shared barrels point to existing implementation files.
- [x] No external software integration is introduced through `src/shared`.
- [x] Domain-owned UI remains in `src/components/features` until each feature migration is planned.
- [x] New shared exports are small enough to typecheck without app shell changes.
