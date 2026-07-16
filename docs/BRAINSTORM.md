# Brainstorm: Agent settings prototype

## Goal

Design a shared settings experience in `packages/app-shell` for the Ora Agent IDE, with appearance preferences and an "Atoms" area for managing agents and skills. The result must work in both desktop and web hosts and use `@ora/ui` components.

## Codebase findings

- `AppShell` owns shared product UI and is the correct owner for settings state and presentation.
- `UserProfile` already provides the natural settings entry point in the sidebar footer.
- `@ora/ui` already exports the required primitives: dialog, tabs, select, switch, input, button, badge, and scroll area.
- `ContractsClient` already exposes typed CRUD commands for both agents and skills; the Atoms UI can use real repository boundaries rather than prototype-only data shapes.
- Language switching already exists through `react-i18next`, but currently lives directly in the account menu.
- Theme state and provider/model preferences do not yet have contracts, so those controls should initially use local prototype state behind a small settings boundary.

## Current recommendation

Use a large IDE-style settings dialog rather than a separate route. The dialog has a narrow left navigation rail and a scrollable settings pane, remains usable on laptop-sized screens, and can later become a full page without changing feature ownership.

### Settings categories

1. **Appearance**
   - Theme: system, light, dark.
   - Language: Simplified Chinese or English.
   - Optional density control: comfortable or compact, because Agent IDE users scan long trees and logs.

2. **Atoms**
   - Agents and Skills tabs with counts, search, and empty/loading/error states.
   - Commands: create, edit, and delete using the existing contracts.
   - Each row shows name and description; destructive actions require confirmation.
   - Reserve import/export and enable/disable actions for a later iteration instead of inventing unsupported contracts now.

3. **Models & Services**
   - Default provider and model selectors.
   - Connection-status row and a manage-provider command.
   - Do not place raw API secrets in the shared frontend until a secure host-backed credential contract exists.

4. **Permissions & Execution**
   - Approval policy: ask every time, ask for risky actions, or trusted workspace.
   - Separate switches for terminal, filesystem writes, and network access.
   - Default command timeout selector.

5. **Data & Privacy**
   - Conversation-history retention.
   - Diagnostic/telemetry sharing switch.
   - Clear local conversation history command with destructive confirmation.

## Ownership boundaries

| Layer | Responsibility |
|---|---|
| `packages/app-shell/src/features/settings` | Settings dialog, category navigation, local prototype preferences, Atoms management UI |
| `packages/app-shell/src/app-shell.tsx` | Dialog open state and shared settings integration |
| `packages/app-shell/src/features/sidebar/user-profile.tsx` | Settings entry command only |
| `packages/contracts` | Existing Agent and Skill CRUD; future persisted settings contracts |
| `packages/ui` | Reusable visual primitives only |

## Interaction decisions

- Preferences apply immediately; there is no global Save button.
- CRUD forms use focused dialogs and delete confirmation, matching current workspace entity behavior.
- Language moves into Appearance; the account menu keeps Settings and Log out.
- The first prototype should expose all five categories, while only Appearance and Atoms need complete interactions.

## Implementation status

- The five-category large-dialog layout is implemented in `packages/app-shell`.
- Appearance preferences apply immediately and persist locally.
- Atoms uses the existing Agent and Skill CRUD contracts backed by `packages/mock-service` in the web prototype.
- Models, permissions, and privacy controls are interactive prototype state pending host-backed settings contracts.
