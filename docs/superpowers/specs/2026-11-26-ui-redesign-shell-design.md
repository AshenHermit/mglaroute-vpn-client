# UI redesign: three-pane shell + animate-ui + portrait-ready layout

Date: 2026-11-26
Status: approved
Scope: UI only. No Rust, no Tauri command, no contract schema changes.

## Intent and why

The desktop client works but reads like a settings form: five flat tabs across the
top, a light `slate/sky` palette, a wide server table, and the connect action on
a different page from the server list. The operator wants the client to feel like
the VPN app in the supplied reference: a persistent connection surface, a dense
server list, and a narrow navigation rail, restyled with the animate-ui look used
by the Wired homepage (`frontend/src/app/(browser)/(homepage)/HomepageClient.tsx`)
and with green (not yellow) as the single accent.

The redesign must also prepare the UI for future Android/iOS targets **without**
adding mobile targets now: the current Tauri window must simply reflow cleanly
down to a phone-width portrait viewport when resized.

Reference material:
- Supplied screenshot: three zones — left "Select Server" rail, center searchable
  server list with stars, right connection panel with a large power ring and
  connection facts. Yellow glow is replaced by green.
- Wired homepage: centered `max-w-3xl` column, card, and animate-ui `Tabs` with a
  gliding highlight and sliding panels, driven by Tailwind 4 CSS variables.

## Goals

1. Replace the flat five-tab header with a persistent three-zone shell.
2. Make Connect a first-class, always-visible action beside the server list.
3. Move Subscriptions, Service profiles and Settings behind a gear-triggered
   sheet, using animate-ui tabs inside it.
4. Lift the frontend stack to React 19 + Tailwind 4 and use the official
   `@animate-ui/react` primitives, matching the Wired aesthetic.
5. Introduce a single dark token palette with a green accent, applied to every
   local UI wrapper and page.
6. Make the shell responsive down to ~360px wide portrait inside the existing
   Tauri window using CSS only; no mobile platform code, plugins or targets.
7. Keep all existing behaviour, busy-code handling and test hooks intact.

## Non-goals

- No Android/iOS Tauri targets, no mobile plugins, no native navigation bars.
- No new backend commands, no contract schema changes, no storage migration.
- No fabricated feature data: no country flags, no traffic counters, no
  "dedicated IP" or "for streaming" categories — `Profile` (contracts
  `schemas.ts`) carries `remarks`, `protocol`, `address`, `latencyMs`,
  `reachable`, `sortIndex`, `subscriptionId`; `ConnectionState` carries only
  `status`, `profileId`, `proxyPort`.
- No functional anti-DPI or TUN: the badge stays `unsupported` as today.
- No rewrite of subscription import, probing, auto-sort or reconnect logic.

## Anti-goals

- A light theme with a green button sprinkled on it — that is not the reference.
- A separate "Connect" page — the whole point is a persistent connection pane.
- Duplicating the Wired `components/animate-ui` folder into this repo — the
  current official package supplies the same primitives with less drift.
- Hover-only controls, fixed-width panes, or a layout that only works at
  900×640 (the current window size).

## Constraints

- Stack decision by operator: pnpm workspace + Nx, Tauri v2, Vite, React,
  TypeScript strict, Zustand 5, local shadcn-style Radix wrappers, Tailwind.
- `apps/desktop/src-tauri/tauri.conf.json` opens one 900×640 window with no
  `minWidth`/`minHeight`; tests must still pass at that size and below.
- UI-only preferences may be persisted; durable data stays in SQLite through
  Tauri commands. Never persist or log subscription URLs, credentials or raw
  config.
- Existing tests assert concrete DOM hooks: `mglaroute` heading (`App.test.tsx`),
  `server-row`, `server-power`, `connection-status`, `active-server`, a
  `div.overflow-x-auto` with a `table.table-fixed` and `td.break-words`
  (`server-table.test.tsx`). Preserve them.
- Node is declared `>=20`; pick a Vite version compatible with the installed
  Node (Vite 7 needs Node ≥20.19; otherwise keep Vite 5.4 and use
  `@tailwindcss/vite`, which supports it).

## Approaches considered

### A — Lift the stack, use official animate-ui (chosen)

React 19.2, Tailwind 4.1 via `@tailwindcss/vite`, `@animate-ui/react` + `motion`,
`npx shadcn init` with `components.json`, and CSS-variable tokens in
`apps/desktop/src/index.css`. The official `@animate-ui/react/primitives/tabs`
provides the same gliding highlight and sliding panels as the Wired homepage,
plus keyboard navigation and reduced-motion handling. `npx @animate-ui/cli@latest
animate all` can animate the existing local Radix wrappers instead of rewriting
them.

Tradeoff: larger one-time migration (class renaming, Vite config, test setup for
Motion in jsdom), but no vendored duplicate and the stack the operator asked to
raise.

### B — Vendor Wired's animate-ui folder, keep React 18 / Tailwind 3

Copy `frontend/src/components/animate-ui` into the client, add `motion`, `cva`,
`clsx`, `tailwind-merge`, `tailwindcss-animate`, and translate tokens to Tailwind
3 `@layer base`. Lower migration risk, but duplicates ~25 primitive files, pins
an older API, and contradicts the explicit "raise React/Tailwind" instruction.

### C — Use `@animate-ui/react` primitives without shadcn init, keep Tailwind 3

Fewer config changes, but animate-ui styling assumes Tailwind 4 utilities
(`size-*`, `@theme` tokens, `@custom-variant`) and the result drifts from Wired.
Rejected.

## Decided design

### 1. Stack and tooling

- `react`/`react-dom` → 19.2, `@types/react`/`@types/react-dom` → 19.
- Tailwind 3 → 4.1: delete `tailwind.config.js` and `postcss.config.js`, add
  `@tailwindcss/vite` plugin in `vite.config.ts`, `@import "tailwindcss";` in
  `index.css`. Keep Vite 5.4 only if Node < 20.19; otherwise move to Vite 7.
- Add `@animate-ui/react`, `motion`, `class-variance-authority`, `clsx`,
  `tailwind-merge`, `@radix-ui/react-slot` (needed by animate-ui wrappers).
- Add `components.json` with `registries["@animate-ui"] =
  "https://animate-ui.com/r/{name}.json"` and an `@/*` path alias in both
  `tsconfig.json` and `vite.config.ts`. Do **not** use the existing
  `@mglaroute/ui` alias: it points at `packages/ui/src/index.ts`, which does not
  exist.
- Wrap the app in `AnimateUIProvider tempo={0.9}` (snappy desktop feel).
- Run `npx @animate-ui/cli@latest animate all`, then review the diff; do not
  blindly accept changes to business logic or `data-testid`s.

### 2. Theme tokens

Dark base, one accent. Values are proposals; exact lightness is left to the
builder as long as WCAG AA text contrast holds on the card background.

```css
@import "tailwindcss";
@theme inline {
  --color-background: var(--background);
  --color-card: var(--card);
  --color-muted: var(--muted);
  --color-muted-foreground: var(--muted-foreground);
  --color-primary: var(--primary);
  --color-primary-foreground: var(--primary-foreground);
  --color-border: var(--border);
  --color-ring: var(--ring);
  --color-destructive: var(--destructive);
}
:root {
  --background: oklch(0.16 0.015 250);  /* shell */
  --card: oklch(0.21 0.015 250);        /* panes */
  --muted: oklch(0.27 0.015 250);       /* hover / inactive */
  --muted-foreground: oklch(0.72 0.02 250);
  --primary: oklch(0.72 0.17 150);      /* green accent */
  --primary-foreground: oklch(0.16 0.02 250);
  --border: oklch(1 0 0 / 10%);
  --ring: var(--primary);
  --destructive: oklch(0.70 0.19 22);
}
```

Green is used for: active nav item, connected status, the power ring, focus
rings, and the default button. Yellow is not used anywhere. The reference glow
becomes a restrained `box-shadow: 0 0 0 8px oklch(0.72 0.17 150 / 0.12)` on the
connected power button only.

Restyle `components/ui/{button,card,dialog,input,switch,table}.tsx` to consume
tokens instead of `slate-*`/`sky-*`, keeping the same exported APIs and props.

### 3. Shell and zones

New `apps/desktop/src/app-shell/AppShell.tsx` owns layout and navigation;
`App.tsx` becomes a thin composition root that calls `useBackendEvents()` and
renders `<AppShell />`.

Reference-derived structure:

- **Left rail** — brand `h1` "mglaroute" (keep the test-visible heading), filter
  list, footer counters (subscriptions / servers) and the real anti-DPI status
  badge (`unsupported` in this build). No invented "Connection features / Smart
  rules" links: those features do not exist in this client. Items: `All servers`,
  `Favorites`, `Reachable only`, then a dynamic `By subscription` group.
  Favorites and the active filter are UI-only Zustand `persist` state (IDs and
  filter key only — never links or secrets). All entries filter the center list;
  none navigate away. The gear button opens the sheet.
- **Center pane** — sticky header with search input, result count and `Run test`
  / `Stop test`; below it the server list. Reuse `ServerTable`: one `<table>`
  with `table-fixed`, `overflow-x-auto`, per-row `data-testid="server-row"`,
  power button, latency, state. Add a star toggle (`server-favorite`) and a
  detail chevron that opens a small sheet with server facts and "Set active".
  On narrow screens hide `Protocol`/`Address` columns (`hidden sm:table-cell`)
  rather than rendering a second DOM tree — this keeps existing tests valid.
- **Right pane** — persistent connection panel: large circular power button
  (green ring only when connected, neutral otherwise), status text
  (`connection-status`), active server name/address, proxy port, protocol,
  latency, and a client-side connection timer reset on every status change.
  `active-server` hook stays. Reserve an empty labelled slot for future traffic
  stats; render nothing fabricated. Anti-DPI badge shows `unsupported`.
- **Gear sheet** — Radix Dialog as a right-side sheet with animate-ui motion;
  contains `<Tabs defaultValue="subscriptions">` with three panels:
  `Subscriptions`, `Service profiles`, `Settings`, each hosting the existing
  feature component unchanged. Width `w-full sm:w-[min(480px,100vw)]`.
- **Notice** — restyle the existing toast to a bottom-centered token-based
  surface with a small Motion enter/exit; keep `role="status"` and dismiss
  button.

### 4. Responsive behaviour (CSS only, no mobile targets)

Root shell uses `h-[100dvh] min-h-0 w-full overflow-hidden`; every pane gets
`min-h-0 min-w-0 overflow-y-auto overscroll-contain`.

| Width | Layout |
| --- | --- |
| ≥1024px (`lg`) | Three columns: `grid-cols-[240px_minmax(0,1fr)_320px]`; rails fixed, center scrolls. |
| 640–1023px (`md`) | Two columns: icon-only rail (72px) + center. Connection pane becomes a compact hero card above the list; gear still opens the sheet. |
| <640px | Single column, vertical order: connection hero → horizontally scrollable filter chips → search + server list. Bottom fixed tab bar: `Connect` scrolls to the hero, `Servers` scrolls to the list, `More` opens the gear sheet; active tab follows the scrolled zone. `pb-[calc(72px+env(safe-area-inset-bottom))]` on the scroll area. |

All interactive targets ≥44px on touch layouts; no hover-only affordances;
`:focus-visible` rings from `--ring`; `touch-manipulation` on rows/buttons.
The server table remains horizontally scrollable at very narrow widths.

### 5. State and behaviour boundaries

- No contract or Rust changes. Connect/disconnect, Run test/Stop test, active
  server selection and all busy-code notices keep their current semantics.
- New UI-only persisted slice: `favorites: string[]`, `serverFilter` of shape
  `{ kind: "all" | "favorites" | "reachable" | "subscription"; subscriptionId?: string }`
  (one exclusive filter — no combinations in v1), and `sheetTab: string`. Keep
  `selectedProfileId`, connection state, probe state and notices session-only as
  today. `reachable` means the inverse of `ServerTable.isFailed`:
  `latencyMs !== null && latencyMs > 0 && reachable === true`; untested profiles
  drop out of that view only.
- Favorites never modify the profile table or API payloads.

### 7. Dashboard retirement

`Dashboard.tsx` is not a sheet tab and is removed as a page. Its real data is
redistributed, not dropped:

| Old Dashboard content | New home |
| --- | --- |
| subscription / server counts | left-rail footer counters |
| `Last test cycle`, `tested / total`, `finishedAt` | center pane header under `Run test` |
| `anti_dpi_status` fetch + badge | left-rail footer badge (same fetch, same `unsupported` result) |
| `active-server`, `ConnectionPanel` | right connection pane (`active-server` hook preserved) |

`Dashboard.test.tsx` is migrated to the new zone components or retired only where
the assertion is purely about the removed page shell. No data or command call is
lost.

### 6. Verification and migration checks

1. `pnpm test` and `pnpm build` (`NX_DAEMON='false'`) pass after each phase.
2. Motion in jsdom may need `matchMedia`/`ResizeObserver` mocks in the Vitest
   setup; add only what the suite actually needs.
3. Manual pass in the Tauri window: resize 900×640 → ~1024×700 → 800×900 →
   400×850, verify all three zones, sheet, search, favorites, power button and
   notice are reachable; no horizontal page scroll.
4. Confirm no yellow accent remains and no fabricated stats are rendered.

## What is left to the builder

- Exact token values and the final green shade, as long as contrast holds.
- File placement of small presentational helpers under `app-shell/` and
  `features/server-list/`, and which animate-ui wrappers the CLI actually swaps.
- Animation durations within the `tempo={0.9}` envelope; keep motion restrained.
- Whether the connection timer starts on click or on the `connected` event
  (prefer the event; no backend change).
