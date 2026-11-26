# UI Redesign Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat five-tab client UI with a dark three-pane shell built on React 19 + Tailwind 4 + animate-ui, with advanced pages in a gear sheet and CSS-only portrait reflow.

**Architecture:** A new `AppShell` owns layout and navigation; the left rail filters the center server list, the right pane keeps Connect always visible, and a sheet holds Subscriptions / Service profiles / Settings. Existing Tauri commands, Zustand session state and all `data-testid` hooks stay intact; only UI files change.

**Tech Stack:** pnpm + Nx, Tauri v2, React 19.2, TypeScript strict, Vite, Tailwind 4.1 via `@tailwindcss/vite`, `@animate-ui/react` + `motion`, Zustand 5, Radix, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-26-ui-redesign-shell-design.md`

## Global Constraints

- Node >=20; use Vite ^7.1.0 + `@vitejs/plugin-react` ^5 only when `node -v` is >=20.19, otherwise keep Vite ^5.4.11 + plugin-react ^4.3.4.
- React/react-dom 19.2.0; Tailwind 4.1 with `@tailwindcss/vite`; no `tailwind.config.js` or `postcss.config.js`.
- Dark tokens only; green accent `--primary: oklch(0.72 0.17 150)`; no `sky-*` / `slate-*` / yellow anywhere in rendered UI.
- Tauri window opens 900x640. CSS-only reflow down to ~360px portrait; no Android/iOS targets, plugins or native code.
- Preserve test hooks exactly: `mglaroute` heading, `server-row`, `server-power`, `connection-status`, `active-server`, and `div.overflow-x-auto > table.table-fixed` with `td.break-words`.
- Preserve all command semantics and busy codes: `connection_busy`, `port_in_use`, `probe_busy`, `mutation_busy`. No Rust, contract schema or SQLite changes.
- Persist only non-secret UI preferences (favorites, filter, sheet tab). Never persist or log subscription URLs, links, credentials or raw config.
- No fabricated data: no flags, no traffic counters, no "dedicated IP / downloading / streaming" categories, no fake anti-DPI capability.
- Touch targets >=44px; no hover-only controls; `:focus-visible` uses `--ring`.
- Verification commands: `pnpm test` (root) and `pnpm build` with `NX_DAEMON='false'`; both must exit 0.

## Review Focus

- **Untested servers under "Reachable only":** `latencyMs === null` or `reachable === false` must be excluded, not shown as healthy — pinned by `server-filter.test.ts` case "excludes untested and failed profiles".
- **`connection_busy` / `port_in_use` rejections:** must produce the existing notices and leave state untouched rather than entering `faulted` — pinned by the two migrated cases in `connection-panel.test.tsx`.
- **Persisted UI state containing secrets:** favorites/filter must never capture `link`, `rawConfig` or credentials from a loaded profile — pinned by `ui.store.test.ts` case "persists only non-secret keys".
- **Very long remarks/addresses in a narrow pane:** must wrap inside the scrollable table without widening the page — pinned by `server-table.test.tsx` case "keeps long names in scrollable area" plus the manual 400px resize check.
- **Portrait bottom navigation:** `Connect`/`Servers` must scroll to their zones and only `More` opens the sheet; active state must follow the visible zone — pinned by `bottom-nav.test.tsx` and the manual resize check.

---

### Task 1: Lift frontend stack and define theme foundation

**Files:**
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/vite.config.ts`
- Modify: `apps/desktop/tsconfig.json`
- Modify: `apps/desktop/src/index.css`
- Create: `apps/desktop/components.json`
- Create: `apps/desktop/src/lib/utils.ts`
- Delete: `apps/desktop/tailwind.config.js`, `apps/desktop/postcss.config.js`
- Modify: `pnpm-lock.yaml` (via install)

**Interfaces:**
- Consumes: existing `vite.config.ts` Vitest config (`environment: "jsdom"`, `include: ["src/**/*.test.{ts,tsx}"]`).
- Produces: `cn(...inputs: ClassValue[]): string` from `src/lib/utils.ts`; CSS tokens `--background`, `--card`, `--muted`, `--muted-foreground`, `--primary`, `--primary-foreground`, `--border`, `--ring`, `--destructive` in `src/index.css`; `@/*` alias in TS and Vite.

- [ ] **Step 1: Check Node version and pick Vite major**

Run `node -v`. If >=20.19 set Vite `^7.1.0` and `@vitejs/plugin-react` `^5.0.0`; otherwise keep Vite `^5.4.11` and plugin-react `^4.3.4`.

- [ ] **Step 2: Update dependencies and configs**

In `apps/desktop/package.json` set react/react-dom `19.2.0`, `@types/react`/`@types/react-dom` `^19`, add `@animate-ui/react` (latest), `motion` `^12.23.26`, `class-variance-authority` `^0.7.1`, `clsx` `^2.1.1`, `tailwind-merge` `^3.4.0`, `@radix-ui/react-slot` `^1.2.4`, `tailwindcss` `^4.1.17`, `@tailwindcss/vite` `^4.1.17`; drop `autoprefixer` and `postcss`. Add `@tailwindcss/vite()` to `vite.config.ts` plugins, keep `react()` and the `test` block. In `tsconfig.json` add `"paths": { "@/*": ["src/*"] }` alongside inherited paths. Add `components.json` with `"rsc": false`, `"css": "src/index.css"`, aliases `@/components`, `@/lib/utils`, `@/components/ui`, `@/lib`, `@/hooks`, and registries `{ "@animate-ui": "https://animate-ui.com/r/{name}.json" }`.

- [ ] **Step 3: Replace stylesheet and delete legacy configs**

Write `src/index.css` exactly as the token block in spec section 2 (`@import "tailwindcss";`, `@theme inline` mapping, `:root` dark values including `--primary: oklch(0.72 0.17 150)` and `--ring: var(--primary)`). Delete `tailwind.config.js` and `postcss.config.js`. Create `src/lib/utils.ts` exporting `cn` via `twMerge(clsx(inputs))`.

- [ ] **Step 4: Install and verify**

Run `pnpm install` at workspace root, then `pnpm test` and `NX_DAEMON='false' pnpm build`. Expected: both exit 0. Search under `apps/desktop/src` for `sky-` and `slate-`; expected: no hits.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/package.json apps/desktop/vite.config.ts apps/desktop/tsconfig.json apps/desktop/src/index.css apps/desktop/src/lib/utils.ts apps/desktop/components.json pnpm-lock.yaml
git rm apps/desktop/tailwind.config.js apps/desktop/postcss.config.js
git commit -m "chore(ui): lift frontend to React 19 and Tailwind 4 tokens"
```

### Task 2: Token-based UI primitives and animate-ui provider

**Files:**
- Modify: `apps/desktop/src/components/ui/{button,card,dialog,input,switch,table}.tsx`
- Modify: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/components/ui/primitives.test.tsx`

**Interfaces:**
- Consumes: `cn` from Task 1; CSS tokens from Task 1.
- Produces: same exported component names and prop types as before, now token-styled; `AnimateUIProvider tempo={0.9}` wrapping `<App />` in `main.tsx`.

- [ ] **Step 1: Write failing test**

Create `primitives.test.tsx` asserting: default `Button` className contains `bg-primary` and no `bg-sky-`; `Card` className contains `bg-card`; `cn("px-2", "px-4")` returns `"px-4"`.

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- primitives.test.tsx`. Expected: FAIL because primitives still use `sky-*`/`slate-*`.

- [ ] **Step 3: Restyle primitives and add provider**

Replace literal palette classes in all six files with tokens: `bg-primary`, `text-primary-foreground`, `bg-card`, `border-border`, `bg-muted`, `text-muted-foreground`, `bg-background`, `ring-ring`. Keep every `data-testid`, `variant` prop and DOM structure. Wrap `<App />` in `<AnimateUIProvider tempo={0.9}>` imported from `@animate-ui/react/provider`.

- [ ] **Step 4: Animate wrappers with the official CLI**

Run `npx @animate-ui/cli@latest animate all --dry-run`, review the diff, then run without `--dry-run`. Accept only primitive-import/style changes; revert any change to props, business logic or `data-testid`s.

- [ ] **Step 5: Run tests to verify they pass**

Run `pnpm --filter @mglaroute/desktop test -- primitives.test.tsx` and then `pnpm test`. Expected: PASS, all suites green.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/components/ui apps/desktop/src/main.tsx
git commit -m "feat(ui): token primitives and animate-ui provider"
```

### Task 3: UI-only persisted store

**Files:**
- Create: `apps/desktop/src/stores/ui.store.ts`
- Create: `apps/desktop/src/stores/ui.store.test.ts`

**Interfaces:**
- Consumes: zustand 5 `persist`.
- Produces: `ServerFilter = { kind: "all" | "favorites" | "reachable" | "subscription"; subscriptionId?: string }`; `SheetTab = "subscriptions" | "serviceProfiles" | "settings"`; `useUiStore` with fields `favorites: string[]`, `filter: ServerFilter`, `sheetTab: SheetTab` and actions `toggleFavorite(id: string): void`, `setFilter(filter: ServerFilter): void`, `setSheetTab(tab: SheetTab): void`, backed by `persist(..., { name: "mglaroute-ui", partialize })`.

- [ ] **Step 1: Write failing tests**

Cover: defaults (`favorites: []`, `filter.kind === "all"`, `sheetTab === "subscriptions"`); `toggleFavorite` adds then removes; `setFilter` replaces the previous filter; `setSheetTab` accepts each of the three values. Add case "persists only non-secret keys": after setting a favorite and filter, `JSON.parse(localStorage.getItem("mglaroute-ui"))` has only `favorites`, `filter`, `sheetTab` under `state`, and its string form contains no `vless://` or `rawConfig`.

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- ui.store.test.ts`. Expected: FAIL, module absent.

- [ ] **Step 3: Implement the store**

Create `ui.store.ts` with the exact types and actions above. `persist` middleware wraps the store; `partialize` returns only the three UI fields.

- [ ] **Step 4: Run tests to verify they pass**

Run `pnpm --filter @mglaroute/desktop test -- ui.store.test.ts`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/stores/ui.store.ts apps/desktop/src/stores/ui.store.test.ts
git commit -m "feat(ui): persist favorites, filter and sheet tab"
```

### Task 4: Center pane — filtering, search and server rows

**Files:**
- Create: `apps/desktop/src/features/server-list/server-filter.ts`
- Create: `apps/desktop/src/features/server-list/server-filter.test.ts`
- Create: `apps/desktop/src/features/server-list/ServerDetailsDialog.tsx`
- Create: `apps/desktop/src/features/server-list/server-details-dialog.test.tsx`
- Modify: `apps/desktop/src/features/server-table/ServerTable.tsx`
- Modify: `apps/desktop/src/features/server-table/server-table.test.tsx`
- Modify: `apps/desktop/src/pages/Servers.tsx`
- Modify: `apps/desktop/src/pages/Servers.test.tsx`

**Interfaces:**
- Consumes: `ServerFilter` and `useUiStore` from Task 3; `cn` from Task 1; existing `Profile` type and `useAppStore` probe fields.
- Produces: `filterProfiles(profiles: Profile[], filter: ServerFilter, favorites: string[]): Profile[]` (no sorting); `ServerTable` gains optional props `favorites?: string[]`, `onToggleFavorite?: (id: string) => void`, `onOpenDetails?: (profile: Profile) => void`; `ServerDetailsDialog` props `{ profile: Profile | null; connected: boolean; onClose: () => void; onSetActive: (id: string) => void }`.

- [ ] **Step 1: Write failing filter tests**

In `server-filter.test.ts` assert: `all` returns the input unchanged; `favorites` returns only ids in the array; `subscription` returns only matching `subscriptionId`; `reachable` returns only `latencyMs !== null && latencyMs > 0 && reachable === true`; a profile with `latencyMs: null, reachable: false` is excluded; unknown subscription id returns `[]`.

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- server-filter.test.ts`. Expected: FAIL.

- [ ] **Step 3: Implement `filterProfiles`**

Write the pure function in `server-filter.ts`. Do not sort; `ServerTable` keeps its existing delay/failed sort.

- [ ] **Step 4: Run filter tests**

Run `pnpm --filter @mglaroute/desktop test -- server-filter.test.ts`. Expected: PASS.

- [ ] **Step 5: Write failing ServerTable tests**

Add cases: star button `data-testid="server-favorite"` toggles via callback without selecting; `data-testid="server-details"` calls `onOpenDetails` with the profile; Protocol and Address header/cell carry `hidden sm:table-cell`; existing sort, `opacity-60`, `overflow-x-auto`, `table-fixed`, `td.break-words` and row-click-does-not-select assertions stay unchanged.

- [ ] **Step 6: Run them to verify they fail**

Run `pnpm --filter @mglaroute/desktop test -- server-table.test.tsx`. Expected: FAIL on the three new cases.

- [ ] **Step 7: Extend `ServerTable`**

Add the two optional props and two buttons inside the existing row. Keep one `<table>` only — no second DOM tree for mobile. Add `hidden sm:table-cell` to both `<TableHeaderCell>` and matching `<TableCell>` for Protocol and Address. Star button aria-label: `Add favorite: {name}` / `Remove favorite: {name}`; chevron aria-label: `Server details: {name}`.

- [ ] **Step 8: Write failing center-pane tests**

In `Servers.test.tsx` assert: typing into a search box labelled `Search servers` narrows rows by remarks; setting `useUiStore.getState().setFilter({ kind: "favorites" })` with one favorite shows only that row; `Run test` / `Stop test` and `tested / total` + finished timestamp still render from the probe store.

- [ ] **Step 9: Run them to verify they fail**

Run `pnpm --filter @mglaroute/desktop test -- Servers.test.tsx`. Expected: FAIL.

- [ ] **Step 10: Wire `Servers.tsx`**

Read `filter` and `favorites` from `useUiStore`, compute `filterProfiles(profiles, filter, favorites)`, then apply local case-insensitive `search` over `remarks`, `address` and `protocol`. Read `toggleFavorite` with a `useUiStore((s) => s.toggleFavorite)` selector and pass `favorites`, `onToggleFavorite: toggleFavorite` and `onOpenDetails` (opens `ServerDetailsDialog`) into `ServerTable`. Keep all existing invoke calls, busy notices and `setProbeStarted` / `setProbeSettled` logic byte-for-byte in behaviour.

- [ ] **Step 11: Write failing details-dialog test**

Assert it renders name, protocol, address, port, latency; `Set active` calls `onSetActive(profile.id)`; `connected` renders a `Connected` badge; `onClose` fires on the close button.

- [ ] **Step 12: Implement `ServerDetailsDialog`**

Use the existing local Radix Dialog wrapper; no new dependency. Keep `aria-label` on the close button.

- [ ] **Step 13: Run all task tests**

Run `pnpm --filter @mglaroute/desktop test -- server-filter.test.ts server-table.test.tsx Servers.test.tsx server-details-dialog.test.tsx`. Expected: PASS.

- [ ] **Step 14: Commit**

```bash
git add apps/desktop/src/features/server-list apps/desktop/src/features/server-table apps/desktop/src/pages/Servers.tsx apps/desktop/src/pages/Servers.test.tsx
git commit -m "feat(ui): filterable server pane with favorites and details"
```

### Task 5: AppShell skeleton, left rail and Dashboard retirement

**Files:**
- Create: `apps/desktop/src/app-shell/AppShell.tsx`
- Create: `apps/desktop/src/app-shell/ServerRail.tsx`
- Create: `apps/desktop/src/app-shell/app-shell.test.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.test.tsx`
- Delete: `apps/desktop/src/pages/Dashboard.tsx`, `apps/desktop/src/pages/Dashboard.test.tsx`

**Interfaces:**
- Consumes: `useUiStore` (Task 3), `Servers` (Task 4), `cn` (Task 1), `useAppStore` / `useConnectionStore`, `invoke`, `AntiDpiStatusSchema`, `SubscriptionSchema`, `ProfileSchema`.
- Produces: `AppShell` with zones `[data-testid="rail-zone"]`, `[data-testid="center-zone"]`, `[data-testid="right-zone"]`; `ServerRail({ onOpenSettings }: { onOpenSettings: () => void })`; right zone and gear placeholders that Tasks 6 and 7 replace.

- [ ] **Step 1: Write failing shell tests**

In `app-shell.test.tsx`, mock `../lib/invoke` and `@tauri-apps/api/event`; assert: `mglaroute` heading exists; four filter buttons `All servers`, `Favorites`, `Reachable only` and a `By subscription` group label render; clicking `Favorites` sets `useUiStore.getState().filter.kind === "favorites"`; hydration invokes `subscription_list` and `profile_list` and the rail shows their counts; anti-DPI badge reads `unsupported` when `anti_dpi_status` resolves with `{ status: "unsupported" }`; clicking the gear button toggles the gear placeholder.

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- app-shell.test.tsx`. Expected: FAIL, module absent.

- [ ] **Step 3: Implement `ServerRail`**

Renders the `h1` `mglaroute`, filter buttons wired to `setFilter`, dynamic subscription buttons using `useAppStore.subscriptions` with `{ kind: "subscription", subscriptionId }`, footer counters from `useAppStore`, and the anti-DPI status fetched once with `invoke("anti_dpi_status", AntiDpiStatusSchema)` (silent catch sets `unsupported`). Gear button has `aria-label="Open settings"` and calls the prop.

- [ ] **Step 4: Implement `AppShell` and thin `App`**

`AppShell` calls `useBackendEvents()`, hydrates `subscription_list` / `profile_list` on mount exactly as the old `Dashboard` did (silent catch), renders the three zones plus a notice toast with `role="status"` and dismiss button. Center zone renders `<Servers />`. Right zone renders a placeholder `<div data-testid="right-placeholder" />`. Gear button state lives here; when open render `<div data-testid="gear-placeholder" />`. `App.tsx` becomes `return <AppShell />;`.

- [ ] **Step 5: Delete Dashboard and migrate its assertions**

Delete both Dashboard files. Ensure count, last-cycle and anti-DPI assertions live in `app-shell.test.tsx` / `Servers.test.tsx`, and the active-server hook assertion moves to Task 6. Update `App.test.tsx` to assert the heading and all three zones.

- [ ] **Step 6: Run tests**

Run `pnpm --filter @mglaroute/desktop test -- app-shell.test.tsx App.test.tsx`. Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/app-shell apps/desktop/src/App.tsx apps/desktop/src/App.test.tsx
git rm apps/desktop/src/pages/Dashboard.tsx apps/desktop/src/pages/Dashboard.test.tsx
git commit -m "feat(ui): three-pane shell with filter rail"
```

### Task 6: Right connection pane

**Files:**
- Modify: `apps/desktop/src/features/connection-panel/ConnectionPanel.tsx`
- Modify: `apps/desktop/src/features/connection-panel/connection-panel.test.tsx`
- Modify: `apps/desktop/src/app-shell/AppShell.tsx`

**Interfaces:**
- Consumes: `ConnectionState`, `useConnectionStore`, `useConnectionEvents`, `useAppStore` (profiles, selectedProfileId, setNotice), `invoke`, `commandErrorCode`, `cn`.
- Produces: `ConnectionPanel` rendered in `[data-testid="right-zone"]`; preserves `connection-status`, `active-server` and every existing label/notice.

- [ ] **Step 1: Add failing tests**

Migrate every existing `connection-panel.test.tsx` case unchanged (connect with no selection, selected id passed, `connection_busy`, `port_in_use`, faulted, disconnect). Add: when status is `connected`, the power button has `ring-primary`; `active-server` shows the selected profile's `remarks` and `address`; the timer shows `00:00` on connect and resets to `00:00` after a disconnect event (use Vitest fake timers).

- [ ] **Step 2: Run them to verify they fail**

Run `pnpm --filter @mglaroute/desktop test -- connection-panel.test.tsx`. Expected: FAIL on the three new cases.

- [ ] **Step 3: Restyle and extend `ConnectionPanel`**

Replace the header/Connect row with a large circular power button whose accessible name is still `Connect` / `Reconnect` / `Disconnect` (so migrated tests keep working); connected state adds `ring-2 ring-primary` and the restrained green glow from the spec. Add `data-testid="active-server"` with `data-state` (`connected` | `active` | `none`), name/address from `profiles.find(p => p.id === (connection.profileId ?? selectedProfileId))`, proxy port, protocol and latency lines, and the `mm:ss` timer driven by an interval started on `connected` and cleared on every other status. Keep every busy-code branch and notice string exactly as today. Replace the right-zone placeholder in `AppShell` with `<ConnectionPanel />` inside `[data-testid="right-zone"]`.

- [ ] **Step 4: Run tests**

Run `pnpm --filter @mglaroute/desktop test -- connection-panel.test.tsx app-shell.test.tsx`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/features/connection-panel apps/desktop/src/app-shell/AppShell.tsx
git commit -m "feat(ui): persistent green power pane"
```

### Task 7: Gear sheet with animate-ui tabs

**Files:**
- Create: `apps/desktop/src/app-shell/GearSheet.tsx`
- Create: `apps/desktop/src/app-shell/gear-sheet.test.tsx`
- Modify: `apps/desktop/src/app-shell/AppShell.tsx`
- Create (only if Motion needs it): `apps/desktop/src/test-setup.ts`
- Modify (only if needed): `apps/desktop/vite.config.ts`

**Interfaces:**
- Consumes: `useUiStore.sheetTab` / `setSheetTab` (Task 3), tabs from `@animate-ui/react/primitives/tabs`, Radix Dialog wrapper (Task 2), feature components `SubscriptionList`, `ServiceProfileEditor`, `SettingsForm`.
- Produces: `GearSheet({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void })`.

- [ ] **Step 1: Write failing test**

Mock the three feature components to simple markers. Render `GearSheet` with `open={true}`; assert three triggers named `Subscriptions`, `Service profiles`, `Settings`; clicking `Settings` sets `useUiStore.getState().sheetTab === "settings"` and shows only the settings marker; the content container has class `sm:w-[min(480px,100vw)]`; firing the close control calls `onOpenChange(false)`.

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- gear-sheet.test.tsx`. Expected: FAIL.

- [ ] **Step 3: Implement `GearSheet`**

Use the Radix Dialog wrapper as a right-side sheet: `fixed right-0 top-0 h-full w-full sm:w-[min(480px,100vw)] bg-card border-l border-border`. Inside, use controlled `Tabs` from `@animate-ui/react/primitives/tabs` with `TabsHighlight`, `TabsList`, `TabsHighlightItem`, `TabsTrigger`, `TabsContents`, `TabsContent`, value wired to `sheetTab` / `setSheetTab`. Each panel renders the corresponding feature component unchanged. Replace the gear placeholder in `AppShell` with `<GearSheet open={sheetOpen} onOpenChange={setSheetOpen} />`.

- [ ] **Step 4: Add jsdom mocks only if required**

If the test fails on missing browser APIs, create `src/test-setup.ts` with `window.matchMedia` and `globalThis.ResizeObserver` stubs and add `test.setupFiles: ["src/test-setup.ts"]` to `vite.config.ts`. Do not add other mocks.

- [ ] **Step 5: Run tests**

Run `pnpm --filter @mglaroute/desktop test -- gear-sheet.test.tsx app-shell.test.tsx`. Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/app-shell apps/desktop/src/test-setup.ts apps/desktop/vite.config.ts
git commit -m "feat(ui): advanced pages in animated gear sheet"
```

### Task 8: Portrait-ready responsive layout and final verification

**Files:**
- Modify: `apps/desktop/src/app-shell/AppShell.tsx`
- Create: `apps/desktop/src/app-shell/BottomNav.tsx`
- Create: `apps/desktop/src/app-shell/bottom-nav.test.tsx`
- Modify: `apps/desktop/src/index.css`
- Modify: `apps/desktop/src/features/server-table/ServerTable.tsx`

**Interfaces:**
- Consumes: zones from Task 5, `GearSheet` from Task 7, `ServerTable` from Task 4.
- Produces: `BottomNav({ active, onConnect, onServers, onMore }: { active: "connect" | "servers" | "more"; onConnect: () => void; onServers: () => void; onMore: () => void })`.

- [ ] **Step 1: Write failing bottom-nav test**

Assert three buttons `Connect`, `Servers`, `More`; each click calls its own callback; each button has at least `min-h-11`; `More` is the only one wired to the sheet opener (assert via callback wiring, not DOM).

- [ ] **Step 2: Run it to verify it fails**

Run `pnpm --filter @mglaroute/desktop test -- bottom-nav.test.tsx`. Expected: FAIL.

- [ ] **Step 3: Implement breakpoints and bottom nav**

Root shell: `h-[100dvh] min-h-0 w-full overflow-hidden lg:grid lg:grid-cols-[240px_minmax(0,1fr)_320px]` with child order: rail `order-2 lg:order-1 md:w-[72px] lg:w-auto`, center `order-3 lg:order-2 min-w-0`, right `order-1 lg:order-3 min-h-0`. Every scroll pane gets `min-h-0 min-w-0 overflow-y-auto overscroll-contain`. At `<640px` render `BottomNav` fixed at the bottom with `pb-[calc(72px+env(safe-area-inset-bottom))]` on the scroll area; `Connect` scrolls `[data-testid="right-zone"]` into view, `Servers` scrolls `[data-testid="center-zone"]`, `More` sets `sheetOpen(true)`. Track the active tab from the click; no scroll-spy dependency. Add `.pb-safe { padding-bottom: env(safe-area-inset-bottom); }` to `index.css`. Ensure star/power/chevron buttons are `min-h-11` below `sm`.

- [ ] **Step 4: Run tests**

Run `pnpm --filter @mglaroute/desktop test -- bottom-nav.test.tsx app-shell.test.tsx`. Expected: PASS.

- [ ] **Step 5: Full verification**

Run `pnpm test` and `NX_DAEMON='false' pnpm build` at workspace root. Expected: both exit 0. Then run the Tauri window manually at 900x640, 1024x700, 800x900 and 400x850: all three zones, search, favorites, power button, gear sheet and notice reachable; no horizontal page scroll; no yellow; no fabricated stats.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/app-shell apps/desktop/src/index.css apps/desktop/src/features/server-table/ServerTable.tsx
git commit -m "feat(ui): portrait-ready adaptive shell"
```

## Self-review notes

- Spec sections 1–7 map to Tasks 1–8: stack/tokens → Task 1–2, rail/list/pane/sheet → Task 4–7, Dashboard redistribution → Tasks 5–6, responsive → Task 8, verification → every task plus Task 8.
- Types are consistent: `ServerFilter` from Task 3 is the only filter shape; `filterProfiles` from Task 4 consumes it; component names and test hooks stay unchanged across tasks.
- Review Focus items are each pinned to a named test in Tasks 3, 4, 6 and 8.
- Deliberately left to the implementer: exact token lightness within contrast rules, animation tempo details inside `tempo={0.9}`, and whether the timer starts on click or on the `connected` event (prefer the event).
