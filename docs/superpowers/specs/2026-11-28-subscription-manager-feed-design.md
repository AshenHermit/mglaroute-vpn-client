# Subscription manager + mglaroute feed builder — design

Status: accepted

Date: 2026-11-28

## Intent

Subscriptions panel in the desktop client gets parity with the server admin
panel's feed builder and a usable edit flow. Today the panel has one big card
with an inline add form and a flat list of rows; there is no way to change a
subscription's name or URL after import. Operator wants:

1. Edit existing subscription — change name and URL.
2. Move "add subscription" into a modal opened by an "Add subscription" button.
3. Modal tabs: `URL` (current paste-a-URL flow) and `Mglaroute` (generate a feed
   URL for `https://api.mglaroute.ashen-hermit.space/api/feed`, mirroring
   `mglaroute-vpn-server/frontend/src/components/FeedLinkPanel.tsx`).
4. Editing a subscription whose URL points at that feed also shows its current
   feed settings in the same panel.
5. Subscription list becomes separate cards instead of one monolithic card.

Operator chose: a **public server endpoint** for enabled service names/ids, read
by the client through a **Tauri command** (not WebView `fetch`, no CORS changes).

## Goals

- Add and edit subscriptions entirely from one modal.
- Generated feed URL uses the same query parameters as `FeedLinkPanel`:
  `format`, `search`, `protocol`, `reachable`, `anycast`, `services`, `sort`, `limit`.
- Public endpoint exposes only `{ id, name }`; never test URLs, credentials or
  other admin-only fields.
- Each subscription renders as its own `Card`, with refresh/remove/edit/enable
  and per-row refetch interval preserved.

## Non-goals

- No Rust storage/DB schema changes: `subscription_update` already accepts
  `name` and `url`.
- No `format=json` feed consumption; client imports subscription text only.
- No credential login in client; public feed stays public.
- No server-side changes to the feed itself, scanner, or profiles.
- No new persisted client state for feed form values; modal state is session-only.

## Anti-goals

- Do not call `/api/online-services/enabled` from the client: JWT-protected and
  returns full entities including test URLs.
- Do not put subscription URLs, generated feed URLs or service data into logs.
- Do not allow a changed URL to silently re-import servers; existing profiles
  stay until the user clicks Refresh (existing command semantics).

## Constraints / findings

- Client: Tauri v2 + React 18 + TS strict; local shadcn-style `Dialog` and `Tabs`
  wrappers exist; there is no `Select` wrapper — use a styled native `<select>`.
- Client HTTP from Rust: `subscription/fetch.rs::build_client()` is the canonical
  no-proxy 10 s client; `serde_json` and `reqwest` are direct Cargo deps.
- Command names are asserted by a test in `commands/mod.rs`; any new command must
  be added to the `command_list!` macro and that assertion updated.
- `subscription_update` (verified in `commands/subscription.rs`) applies only
  `Some` fields; omitted `enabled` / `auto_refetch_minutes` stay unchanged. So
  edit-save can send only `{ id, name, url }` without corrupting row state.
- Server: NestJS, `FeedController` is `@Public()`, `OnlineServicesModule`
  already exports `OnlineServicesService`. Public feed writes a raw body with
  `@Res()`; envelope interceptor does not wrap it.
- Server repo has git (remote `origin` GitHub, `server` SSH). Main worktree has
  unrelated staged changes (`README.md`, `docs/assets/...gif`); do not disturb.
- Server worktree created outside repo at
  `D:\Users\hermit\vpn\worktrees-server\feed-services-endpoint`, branch
  `feature/public-feed-services`. Client worktree:
  `D:\Users\hermit\vpn\mglaroute-vpn-client\.worktrees\subscription-panel`,
  branch `feature/subscription-panel`.
- Baselines: client `pnpm test` 162 passed; server backend jest 108 passed.

## Approaches considered

| Approach | Trade-off | Decision |
|---|---|---|
| Public `GET /api/feed/services` + Tauri command using `build_client()` | Small Rust addition; no CORS or WebView-origin coupling; public shape is a curated `{id,name}[]` | **Chosen** |
| WebView `fetch` + extend server CORS with `tauri://localhost` | Less Rust code, but origin differs dev vs packaged; any local page could call it | Rejected |
| Add service names into `/api/feed` metadata | Changes public feed contract for all consumers | Rejected |
| Make `/api/online-services/enabled` public with a projection | Route sits next to `:id` and full-entity controller; easy to leak `testUrls` on future edits | Rejected |

## Design

### Server (`feature/public-feed-services`)

- `GET /api/feed/services` on `FeedController`, `@Public()` via controller scope,
  raw JSON array (`res.json`), Swagger-documented. Never wrapped in envelope,
  consistent with `GET /api/feed`.
- `FeedModule` imports `OnlineServicesModule`; `FeedService` gains
  `listPublicServices(): Promise<Array<{ id: string; name: string }>>` calling
  `onlineServicesService.getEnabled()` and projecting only `id` and `name`.
- Unit test: projection drops `testUrls`/`isEnabled`/`isDefault`; empty list
  returns `[]`; route stays public (controller-level `@Public()` unchanged).
- Docs: add endpoint to server `README.md` API list if it does not collide with
  the operator's staged README edits; otherwise note it for manual sync.

### Client Rust (`feature/subscription-panel`)

- New command `feed_services` in `commands/feed.rs` (new module), re-exported and
  added to `command_list!`.
- Uses `subscription::fetch::build_client()` + `fetch_subscription()` with the
  hardcoded endpoint `https://api.mglaroute.ashen-hermit.space/api/feed/services`,
  parses JSON into `Vec<FeedServicePayload>` (`serde` camelCase), maps any
  failure to `CommandError::Fetch` with no URL/error text. No mutation permit —
  read-only command.
- Test seam (builder call): split a testable pure helper
  `fetch_feed_services(client: &Client, url: &str) -> Result<Vec<FeedServicePayload>,
  CommandError>`; the command calls it with the hardcoded URL. Unit tests call the
  helper against a raw `std::net::TcpListener` fake server (no new Cargo dep).
- Test: local HTTP server serves a JSON body; helper returns parsed rows;
  non-2xx/malformed body maps to `Fetch`. Update command-surface assertion.

### Client contracts

- Add `FeedServiceSchema = z.object({ id: z.string(), name: z.string() })` and
  `FeedServiceListSchema = z.array(FeedServiceSchema)` to
  `packages/contracts/src/schemas.ts`; export both and the inferred type.

### Client UI

- `SubscriptionList.tsx`: page header with "Add subscription" button; below it a
  vertical stack of one `Card` per subscription. Card header: name, status badge,
  enable switch. Card body: URL text (truncated, full via `title`), edit/refresh/
  remove buttons, refetch interval label + input. Existing interval local state
  and error notices stay unchanged.
- New `SubscriptionDialog.tsx` in `features/subscription-list/`: one `Dialog`
  used for add and edit. Props: `open`, `mode` (`add` | `edit`), `subscription`,
  `onClose`, `onSaved`.
  - Name input; optional on add (server defaults to "Subscription"), editable on
    both modes.
  - `Tabs`: `URL` and `Mglaroute`.
    - `URL`: single URL input, same placeholder/validation as today.
    - `Mglaroute`: feed form with protocol, reachable, anycast, sort, limit,
      search and service chips; `format` fixed to `subscription`; base URL fixed
      to `https://api.mglaroute.ashen-hermit.space/api/feed`. Service chips come
      from `feed_services` invoked when the tab first opens; fetch failure is
      non-fatal (chips omitted, rest of form usable). `Generate link` builds the
      URL into the dialog's URL field; readonly preview + warning text mirrors
      FeedLinkPanel's warning.
  - Save calls `subscription_add` (add) or `subscription_update` with
    `{ id, name, url }` (edit), then closes and refreshes rows via `onSaved`.
  - Edit of a URL whose hostname is `api.mglaroute.ashen-hermit.space` and path
    is `/api/feed`: parse query params into the feed form and open on
    `Mglaroute` tab; unknown params are ignored. Other URLs open on `URL` tab.
- No new global store fields.

### Tests

- Rewrite `subscription-list.test.tsx` for modal-driven add/edit and card layout;
  cover URL validation, edit prefill and `subscription_update` args.
- New `subscription-dialog.test.tsx`: tab switch, feed generation from form
  values, edit prefill from feed query, service chips from mocked `feed_services`,
  non-fatal service fetch failure.
- Keep existing enable/interval/refresh error tests working with new layout.

## Decisions made by builder (not operator choices)

- Feed `format` locked to `subscription` — client cannot import `format=json`.
- Feed base URL hardcoded; no arbitrary feed server field in this pass.
- URL change does not trigger automatic refetch; user clicks Refresh.
- Native `<select>` instead of adding a new Radix wrapper.
- `Copy`/`Open` actions from FeedLinkPanel replaced by a generated URL in the
  dialog; avoids WebView external-link behaviour and clipboard permissions.
- Server worktree placed outside the server repo so its unignored `.worktrees/`
  name and staged main-worktree changes are untouched.
- `feed_services` split into thin command + testable URL-taking helper; tests use
  a raw `TcpListener` fake server, no new dev dependency.

## Left to builder

- Implement server endpoint + tests.
- Implement Rust command + contract + tests.
- Implement UI refactor + tests.
- Run both repos' test suites and report results.

## Approval gate

Do not write product code until operator approves this document. After approval,
use `superpowers:writing-plans` to produce the task-by-task plan.
