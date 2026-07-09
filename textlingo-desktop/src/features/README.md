# PR-2 Feature Boundary Review Checklist

## Scope

PR-2 establishes the App Shell directory boundary for `src/features` and `src/shared`. It leaves existing implementation components in `src/components/features` and exposes feature entry metadata for later incremental migration.

## Feature Entries

| Feature | Stable entry | Current implementation boundary |
| --- | --- | --- |
| Materials | `src/features/materials` | Material list, manual article creation, new material dialog, drop import overlay |
| Reader | `src/features/reader` | Article reader shell, explanation panel, bookmark sidebar |
| Notes | `src/features/notes` | Favorite vocabulary and grammar note surfaces |
| Local Learning | `src/features/local-learning` | Reader learning candidates, candidate inbox, word packs, recitation panel, pack selection |
| Assistant | `src/features/assistant` | Agent panel, chat assistant, sidebar shell, mind map, logs |
| Settings | `src/features/settings` | Settings dialog, onboarding, quick switcher, update checker |
| Media | `src/features/media` | Local audio/video/subtitle import, playback, export |
| Books | `src/features/books` | Book import and EPUB/TXT/PDF readers |

## Directory Rules

| Directory | Owns | PR-2 rule |
| --- | --- | --- |
| `src/features/<feature>` | Feature entry metadata and future feature-local exports | Keep shell-only entries until migration work starts. |
| `src/shared` | Cross-feature UI, hooks, lib helpers, and shared types | Re-export existing reusable implementation through small barrels. |
| `src/components/features` | Legacy feature implementations | Keep current components here during PR-2. Move them only in focused follow-up PRs. |
| `src/components/ui` | Existing UI primitives | Re-export through `src/shared/ui`; implementation stays in place for this PR. |

## Review Checklist

- [x] Each Phase 1 feature has a stable `index.ts` entry.
- [x] `src/features/index.ts` exposes a single registry for later app-shell routing or review tooling.
- [x] `src/shared` exposes minimal `ui`, `lib`, `hooks`, and `types` barrels.
- [x] Legacy feature components remain in `src/components/features`.
- [x] PR-2 does not connect Anki, Zotero, MinerU, LanguageTool, MCP, browser plugins, or other external software.
- [x] PR-2 does not split Reader or Settings internals.
- [x] Feature metadata documents current legacy ownership before migration starts.

## Follow-Up Migration Notes

1. Migrate one feature at a time after tests identify current behavior.
2. Move component implementation together with feature-local tests.
3. Keep shared exports generic; feature-owned helpers should live under the corresponding feature directory.
4. PR-5 activates Local Learning through the reader candidate inbox while implementation remains under `src/components/features`.
5. Update this checklist when a legacy component leaves `src/components/features`.
