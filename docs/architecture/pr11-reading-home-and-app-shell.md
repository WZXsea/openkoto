# PR-11 Reading Home and App Shell

## Status

Implementation branch: `wzx/pr11-reading-home-shell`

Status: completed

Release: `0.9.0` prerelease, tag `wzx-v0.9.0-pr11-reading-home-shell`

## Product boundary

PR-11 separates the user-facing reading entry from the material maintenance workbench. The application starts on a quiet reading home, while search, filters, batch actions, tags, import jobs, and duplicate resolution remain available in a dedicated material library.

The phase-one boundary remains unchanged: no Anki, Zotero, MinerU, LanguageTool, external MCP, URL router, or standalone Assistant destination is introduced.

## Navigation contract

- `home` is the cold-start destination.
- `materials` owns the full material workbench.
- `learning` owns canonical learning items and exposes the saved/pack compatibility surface.
- `annotations` is presented as user-facing notes.
- `reader` and `ktv-export` remain immersive and hide the global app shell.
- Reader return state preserves the originating home, material, learning, note, or saved surface.
- Active screen state is session-local and is not persisted across launches.

## Home composition

The main column contains one primary reading action, up to four recent materials excluding the primary item, and explicit import/library shortcuts. The primary action selects the most recently opened in-progress material, falls back to the most recent material, and finally falls back to first-import guidance for an empty library.

The secondary column contains a twelve-week activity heatmap and a compact current-day summary. Activity failure is isolated to this card and never blocks material loading or reading navigation.

## Activity heatmap contract

`GET /learning-review/activity-heatmap` accepts `start_date`, `end_date`, and `timezone_offset_minutes`. The default inclusive range ends on the user's current local date and spans 84 days; requests are limited to 366 days.

The response is:

```json
{
  "start_date": "2026-04-22",
  "end_date": "2026-07-14",
  "days": [
    {
      "date": "2026-07-14",
      "read_materials": 2,
      "learning_actions": 3,
      "activity_score": 5
    }
  ]
}
```

`read_materials` counts distinct materials with `read` events for the local date. `learning_actions` counts `create`, `accept`, `reject`, `archive`, `restore`, `organize`, `local_preview`, and `merge`; `migrate` is excluded. `activity_score` is their sum. Missing dates are represented with zero values. Existing user and occurrence indexes are reused, and no historical events are synthesized before PR-10.

## Compatibility rules

- Material workbench filters and view state remain in memory while the app is open.
- Drag-and-drop and direct material-open events attach the correct reader return surface.
- Existing favorite data and commands remain supported; the global navigation presents them through the learning surface.
- Existing themes and user-selected UI fonts remain authoritative. New UI uses semantic theme tokens rather than fixed palette values.
- No database migration is required.

## Verification

- Backend: date validation, default and maximum ranges, timezone boundaries, empty dates, event classification, and user isolation.
- Desktop: Tauri DTO and command contract tests.
- Frontend: home fallbacks, activity degradation, material-library regression, navigation state, and responsive layout.
- Workflow: cold start, home-to-reader return, materials-to-reader return, notes/learning source return, and import flow.
- Package: version consistency, bundled runtime health, packaged smoke, and ARM64 artifact verification.

Final results:

- `bash script/verify_pr11_reading_home.sh --full` passed Backend, Desktop, frontend, production build, and PR-11 Playwright workflows.
- `bash script/verify_pr11_reading_home.sh --run-pr10` passed the PR-10, PR-9, PR-8, and PR-7 regression chain.
- Frontend completed 54 test files and 211 tests; targeted visual workflow covered 1440px, 1280px, 700px, light/dark modes, user font variables, Reader immersion, and material-filter return state.
- The ARM64 application and bundled Backend, PostgreSQL, and Node executables were verified; deep `codesign` and DMG checksum validation passed with the documented ad-hoc prerelease signature.
- The local application was upgraded from 0.8.0 to 0.9.0 after a complete rollback backup. Backend health, 84-day activity API, migration version, and existing material/learning/activity counts were preserved.
