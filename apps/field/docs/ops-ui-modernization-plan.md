# `/ops` UI modernization and coverage plan

## Scope

- Modernize the shared shell, page headers, buttons, table surfaces, status pills, dropdowns, and responsive layout without changing routes, data contracts, business rules, or existing selectors.
- Prioritize `/ops` and Tower screens while keeping shared controls safe for officer and other modules.
- Use the existing `base-nova` shadcn configuration and `@base-ui/react`; no dependency installation is required.
- On wide screens, let the workspace and data surfaces use the available width instead of leaving narrow centered islands.
- Keep inline create forms collapsed by default and expose them through accessible page actions; once opened, their fields use responsive full-width grids.

## Coverage gate

Every source file changed in this pass must have existing coverage extended or a focused test added in this session.

| Changed area | Coverage |
| --- | --- |
| `src/components/ui/*` | `src/components/ui/primitives.test.tsx` |
| `PageHeader`, `TableSurface`, `controls` | `PageHeader.test.tsx`, `TableSurface.test.tsx`, `controls.test.tsx` |
| status and table primitives | `StatusPill.test.tsx`, `DataTable.test.tsx`, `controls.test.tsx` |
| shell, language, tour, sign-out | `RootLayout.test.tsx`, `LanguageSwitch.test.tsx`, `TourButton.test.tsx`, `SignOutButton.test.tsx` |
| ops list and forms | existing screen tests plus `OpsRequestsScreen.test.tsx` |
| ops nested detail and review | `DemandDetailScreen.test.tsx`, `OpsRequestReviewScreen.test.tsx` |
| Tower overview and drill-downs | `TowerScreen.test.tsx`, `TowerDrillScreens.test.tsx` |
| officer people and registration | `PeopleScreen.test.tsx`, `RegisterScreen.test.tsx` |
| global styling | existing styles/token/contrast/depth/motion tests |
| wide workspace sizing | `RootLayout.test.tsx`, `TableSurface.test.tsx`, `TowerDrillScreens.test.tsx`, `OfficerRecordScreens.test.tsx` |
| progressive ops create forms | `DemandListScreen.test.tsx`, `BuyersScreen.test.tsx`, `responsive.spec.ts`, `demand.spec.ts`, `opportunity.spec.ts`, `journey.spec.ts` |
| ops/Tower/officer detail width | existing detail screen tests plus `responsive.spec.ts` geometry checks |
| mobile page and shell header composition | `PageHeader.test.tsx`, `RootLayout.test.tsx`, `responsive.spec.ts` |
| translated close action | `bundles.test.ts` key-contract coverage |

## Acceptance checks

- Custom Select controls support pointer and keyboard interaction and preserve test IDs.
- Nested pages expose a logical parent Back link and breadcrumbs with current-page semantics.
- Table surfaces are white, with filters/actions included in the same surface and horizontal scrolling limited to the table region.
- Status chips use solid semantic fills with readable contrast; no striped backgrounds anywhere.
- Responsive checks cover 320, 375, 768, 1024, and 1440px without document-level horizontal overflow.
- At 1440px, demand/buyer create panels, request detail, Tower drills, and officer detail occupy the available workspace; create panels remain hidden until requested.
- At every target width, table overflow stays inside the table wrapper and the create panel remains inside the viewport.
- At phone widths, PageHeader navigation/actions and global account actions use a non-cramped stacked/grid layout with usable touch targets.
- Run `pnpm typecheck`, `pnpm lint`, `pnpm test`, and `pnpm e2e` before handoff.
