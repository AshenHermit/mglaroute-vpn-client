# Subscription manager + mglaroute feed builder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the desktop client's Subscriptions panel an add/edit modal with a mglaroute feed-link builder, separate cards per subscription, and a minimal public server endpoint supplying enabled service names.

**Architecture:** Server exposes a curated public `GET /api/feed/services` returning only `{id,name}`. Rust adds a read-only `feed_services` command around the canonical no-proxy fetch client. React replaces the inline add form with one `SubscriptionDialog` used for add and edit; its Mglaroute tab rebuilds the FeedLinkPanel query string, and edit prefill parses matching feed URLs. No DB schema, CORS, auth, or auto-refetch changes.

**Tech Stack:** NestJS/TypeORM/Jest (server); Tauri v2/Rust/reqwest/serde_json (client native); React + TypeScript + Zod + Vitest + local Radix wrappers (client UI).

**Spec:** `docs/superpowers/specs/2026-11-28-subscription-manager-feed-design.md`

## Global Constraints

- Client worktree: `D:/Users/hermit/vpn/mglaroute-vpn-client/.worktrees/subscription-panel`, branch `feature/subscription-panel`. Session cwd already there; all client paths below are relative to it.
- Server worktree: `D:/Users/hermit/vpn/worktrees-server/feed-services-endpoint`, branch `feature/public-feed-services`. Server commands below use `git -C` with that absolute path.
- Server main worktree has unrelated staged changes (`README.md`, `docs/assets/mglaroute_server_character.gif`). Never touch that worktree. README conflict at merge time is operator's note, not this plan's problem.
- Feed base URL is fixed: `https://api.mglaroute.ashen-hermit.space/api/feed`. Services endpoint: `https://api.mglaroute.ashen-hermit.space/api/feed/services`.
- Feed `format` is locked to `subscription`; no `format=json` consumption.
- No CORS changes. No JWT on the public services endpoint. No call to `/api/online-services/enabled` from the client.
- No Rust storage/DB schema change. `subscription_update` already accepts `name` and `url`.
- Changing a URL never triggers automatic refetch; user clicks Refresh.
- Never log subscription URLs, generated feed URLs, service data, credentials, or raw config. Rust errors stay `CommandError::Fetch` with no native text.
- No new Cargo dependencies; native `<select>` for dropdowns; no new Radix wrapper.
- Zod schemas live in `packages/contracts/src/schemas.ts` and are re-exported by `packages/contracts/src/index.ts`.
- Rust tests need `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` and `-- --test-threads=1` because `EVENT_SINK` is process-global.

## Review Focus

- Partial edit must preserve `enabled` and `autoRefetchMinutes` — pinned by `apply_subscription_patch` Rust unit test and the dialog test asserting `subscription_update` args are only `{id,name,url}`.
- Generated feed URL must match FeedLinkPanel parameter order/encoding and ignore unknown params — pinned by the dialog `Generate link` and edit-prefill tests.
- Public services projection must never leak `testUrls`/`isEnabled`/`isDefault` and the route must stay public — pinned by `FeedService.listPublicServices` and controller metadata tests.
- Failure of `feed_services` must not block add/edit — pinned by the dialog test where the command rejects and the form still generates/saves.
- Validation must prevent bogus saves and must not auto-refetch — pinned by dialog tests for empty URL, empty edit name, and `subscription_update`-only success assertion.

---

### Task 1: Public feed services endpoint (server worktree)

**Files:**
- Modify: `backend/src/feed/feed.service.ts`
- Modify: `backend/src/feed/feed.service.spec.ts`
- Modify: `backend/src/feed/feed.controller.ts`
- Create: `backend/src/feed/feed.controller.spec.ts`
- Modify: `backend/src/feed/feed.module.ts`
- Modify: `README.md` (server worktree copy only)

**Interfaces:**
- Consumes: `OnlineServicesService.getEnabled(): Promise<OnlineService[]>` from `../online-services/online-services.service` (already exported by `OnlineServicesModule`).
- Produces: `FeedServiceEntry = { id: string; name: string }`; `FeedService.listPublicServices(): Promise<FeedServiceEntry[]>`; `GET /api/feed/services` returning a raw JSON array (no envelope).

- [ ] **Step 1: Write failing service tests**

In `backend/src/feed/feed.service.spec.ts`, change `setup()` so its 4th constructor arg is a mock `onlineServicesService = { getEnabled: jest.fn().mockResolvedValue([]) }`, return it from `setup()`, and append:

```ts
describe('FeedService.listPublicServices', () => {
  it('projects only id and name for enabled services', async () => {
    const { service, onlineServicesService } = setup([]);
    onlineServicesService.getEnabled.mockResolvedValue([
      { id: 'svc-1', name: 'YouTube', testUrls: ['https://secret.example'], isEnabled: true, isDefault: true },
    ]);
    await expect(service.listPublicServices()).resolves.toEqual([
      { id: 'svc-1', name: 'YouTube' },
    ]);
  });

  it('returns an empty array when no services are enabled', async () => {
    const { service } = setup([]);
    await expect(service.listPublicServices()).resolves.toEqual([]);
  });
});
```

- [ ] **Step 2: Run failing service test**

Run (from server worktree root):

```powershell
pnpm --filter mglaroute-vpn-server-backend exec jest --runInBand feed.service.spec
```

Expected: FAIL because `listPublicServices` does not exist and `setup()` supplies no 4th arg.

- [ ] **Step 3: Write failing controller test**

Create `backend/src/feed/feed.controller.spec.ts`:

```ts
import 'reflect-metadata';
import { Reflector } from '@nestjs/core';
import { IS_PUBLIC_KEY } from '../common/decorators/public.decorator';
import { FeedController } from './feed.controller';

describe('FeedController.getServices', () => {
  it('returns the curated array as raw JSON and stays public', async () => {
    const rows = [{ id: 'svc-1', name: 'YouTube' }];
    const feedService = { listPublicServices: jest.fn().mockResolvedValue(rows) } as any;
    const json = jest.fn();
    const controller = new FeedController(feedService);

    await controller.getServices({ json } as any);

    expect(json).toHaveBeenCalledWith(rows);
    expect(new Reflector().get(IS_PUBLIC_KEY, FeedController)).toBe(true);
    expect(new Reflector().get(IS_PUBLIC_KEY, FeedController.prototype.getServices)).toBeUndefined();
  });
});
```

- [ ] **Step 4: Run failing controller test**

```powershell
pnpm --filter mglaroute-vpn-server-backend exec jest --runInBand feed.controller.spec
```

Expected: FAIL because `getServices` does not exist.

- [ ] **Step 5: Implement the endpoint**

In `backend/src/feed/feed.service.ts`:
- add `import { OnlineServicesService } from '../online-services/online-services.service';`;
- export `interface FeedServiceEntry { id: string; name: string }`;
- inject `private readonly onlineServicesService: OnlineServicesService` as the 4th constructor arg after `settingsRepository`;
- implement `async listPublicServices(): Promise<FeedServiceEntry[]> { const rows = await this.onlineServicesService.getEnabled(); return rows.map(({ id, name }) => ({ id, name })); }`.

In `backend/src/feed/feed.controller.ts`, add before `@Get()`:

```ts
  @Get('services')
  @ApiOperation({
    summary: 'Public enabled service names',
    description: 'Curated id/name pairs for the desktop feed builder. No auth required.',
  })
  async getServices(@Res() res: Response): Promise<void> {
    res.json(await this.feedService.listPublicServices());
  }
```

In `backend/src/feed/feed.module.ts`, import `OnlineServicesModule` from `../online-services/online-services.module` and add it to `imports`.

In server-worktree `README.md`, add after the `/api/feed` row in the API table:

```markdown
| `GET` | `/api/feed/services` | Public | Enabled service id/name pairs for the desktop feed builder |
```

Do not edit the server main worktree.

- [ ] **Step 6: Run server feed tests**

```powershell
pnpm --filter mglaroute-vpn-server-backend exec jest --runInBand feed.service.spec feed.controller.spec
```

Expected: PASS, 2 new service tests + 1 controller test, all existing feed tests still pass.

- [ ] **Step 7: Commit server worktree**

```powershell
git -C "D:/Users/hermit/vpn/worktrees-server/feed-services-endpoint" add backend/src/feed/feed.service.ts backend/src/feed/feed.service.spec.ts backend/src/feed/feed.controller.ts backend/src/feed/feed.controller.spec.ts backend/src/feed/feed.module.ts README.md
git -C "D:/Users/hermit/vpn/worktrees-server/feed-services-endpoint" commit -m "feat: expose public feed services endpoint"
```

### Task 2: Client contracts for feed services

**Files:**
- Modify: `packages/contracts/src/schemas.ts`
- Modify: `packages/contracts/src/schemas.test.ts`

**Interfaces:**
- Consumes: Zod 4.6.7 from `zod`; `index.ts` already does `export * from "./schemas"`.
- Produces: `FeedServiceSchema`, `FeedServiceListSchema`, `FeedService` (`z.infer<typeof FeedServiceSchema>`).

- [ ] **Step 1: Write failing contract test**

In `packages/contracts/src/schemas.test.ts`, add `FeedServiceListSchema` and `FeedServiceSchema` to the existing import from `./index`, then append inside `describe("contract schemas")`:

```ts
  it("parses public feed services and strips admin-only fields", () => {
    expect(
      FeedServiceSchema.parse({
        id: "svc-1",
        name: "YouTube",
        testUrls: ["https://secret.example"],
        isEnabled: true,
        isDefault: true,
      }),
    ).toEqual({ id: "svc-1", name: "YouTube" });
    expect(
      FeedServiceListSchema.parse([{ id: "svc-2", name: "Discord" }]),
    ).toEqual([{ id: "svc-2", name: "Discord" }]);
    expect(() => FeedServiceSchema.parse({ id: "svc-3" })).toThrow();
  });
```

- [ ] **Step 2: Run failing contract test**

```powershell
pnpm --filter @mglaroute/contracts exec vitest run src/schemas.test.ts
```

Expected: FAIL on missing exports.

- [ ] **Step 3: Add schemas**

In `packages/contracts/src/schemas.ts`, after `SubscriptionSchema`/`Subscription`:

```ts
export const FeedServiceSchema = z.object({
  id: z.string(),
  name: z.string(),
});
export type FeedService = z.infer<typeof FeedServiceSchema>;

export const FeedServiceListSchema = z.array(FeedServiceSchema);
```

- [ ] **Step 4: Run contract test**

```powershell
pnpm --filter @mglaroute/contracts exec vitest run src/schemas.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add packages/contracts/src/schemas.ts packages/contracts/src/schemas.test.ts
git commit -m "feat: add feed service contracts"
```

### Task 3: Rust `feed_services` command and partial-update regression test

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/feed.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/subscription.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `build_client()`, `fetch_subscription()` from `crate::subscription::fetch`; `CommandError` from `super`; `serde_json`, `reqwest`, `tokio` already in `Cargo.toml`.
- Produces: `feed_services` Tauri command returning `Vec<FeedServicePayload>` (`{id,name}`); `FEED_SERVICES_URL` const; testable helper `fetch_feed_services(client: &reqwest::Client, url: &str) -> Result<Vec<FeedServicePayload>, CommandError>`; `apply_subscription_patch` in `subscription.rs`.

- [ ] **Step 1: Write failing Rust tests**

In `commands/subscription.rs`, append a `#[cfg(test)] mod tests` with:

```rust
#[test]
fn update_patch_preserves_omitted_fields() {
    let base = Subscription {
        id: "sub-1".to_string(),
        name: "old".to_string(),
        url: "https://old.example/sub".to_string(),
        enabled: true,
        auto_refetch_minutes: 60,
        ..Default::default()
    };
    let patched = apply_subscription_patch(
        base,
        Some("new".to_string()),
        Some("https://new.example/sub".to_string()),
        None,
        None,
    );
    assert_eq!(patched.name, "new");
    assert_eq!(patched.url, "https://new.example/sub");
    assert!(patched.enabled);
    assert_eq!(patched.auto_refetch_minutes, 60);
}
```

Create `commands/feed.rs` with only its test module first:

```rust
#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn run_async<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    fn serve_once(status_line: &str, body: &str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        let body = body.to_string();
        let status_line = status_line.to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).expect("read request");
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write response");
        });
        (format!("http://{addr}/services"), handle)
    }

    #[test]
    fn parses_public_service_rows() {
        let (url, handle) = serve_once("200 OK", r#"[{"id":"svc-1","name":"YouTube"}]"#);
        let rows = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await.expect("parse services")
        });
        handle.join().expect("server thread");
        assert_eq!(rows, vec![FeedServicePayload { id: "svc-1".into(), name: "YouTube".into() }]);
    }

    #[test]
    fn http_error_maps_to_fetch() {
        let (url, handle) = serve_once("404 Not Found", "missing");
        let result = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await
        });
        handle.join().expect("server thread");
        assert_eq!(result.err(), Some(CommandError::Fetch));
    }

    #[test]
    fn malformed_body_maps_to_fetch() {
        let (url, handle) = serve_once("200 OK", "not json");
        let result = run_async(async {
            let client = build_client();
            fetch_feed_services(&client, &url).await
        });
        handle.join().expect("server thread");
        assert_eq!(result.err(), Some(CommandError::Fetch));
    }
}
```

In `commands/mod.rs`, add `"feed_services"` between `"connection_disconnect"` and `"probe_cancel"` in the expected sorted vector in `command_names_registered`.

- [ ] **Step 2: Run failing Rust tests**

```powershell
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib feed::tests -- --test-threads=1
```

```powershell
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib update_patch_preserves_omitted_fields -- --test-threads=1
```

Expected: FAIL — `commands/feed.rs` is not a module and `apply_subscription_patch` does not exist.

- [ ] **Step 3: Implement `commands/feed.rs` above the test module**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{AppState, CommandError};
use crate::subscription::fetch::{build_client, fetch_subscription};

pub const FEED_SERVICES_URL: &str =
    "https://api.mglaroute.ashen-hermit.space/api/feed/services";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedServicePayload {
    pub id: String,
    pub name: String,
}

async fn fetch_feed_services(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<FeedServicePayload>, CommandError> {
    let body = fetch_subscription(client, url, None)
        .await
        .map_err(|_| CommandError::Fetch)?;
    serde_json::from_str(&body).map_err(|_| CommandError::Fetch)
}

#[tauri::command]
pub async fn feed_services(
    _state: State<'_, AppState>,
) -> Result<Vec<FeedServicePayload>, CommandError> {
    let client = build_client();
    fetch_feed_services(&client, FEED_SERVICES_URL).await
}
```

- [ ] **Step 4: Extract and use `apply_subscription_patch`**

In `commands/subscription.rs`, replace the inline `if let Some(...)` mutation block in `subscription_update` with a call to a new private function with this exact signature:

```rust
fn apply_subscription_patch(
    mut value: Subscription,
    name: Option<String>,
    url: Option<String>,
    enabled: Option<bool>,
    auto_refetch_minutes: Option<i64>,
) -> Subscription
```

The function body applies only `Some` fields (same order/logic as today). In the command, after fetching `value`, call `let value = apply_subscription_patch(value, name, url, enabled, auto_refetch_minutes);` before `subscription_repo::update`. Validation and permit order stay unchanged.

- [ ] **Step 5: Register the command**

In `commands/mod.rs`: add `pub mod feed;` next to the other `pub mod` lines and `pub use feed::feed_services;` next to the other `pub use` lines.

In `lib.rs`: add `feed_services,` after `connection_disconnect,` inside `command_list!`.

- [ ] **Step 6: Run Rust tests**

```powershell
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- --test-threads=1
```

Expected: PASS, including `feed::tests` (3 tests), `update_patch_preserves_omitted_fields`, and `command_names_registered`.

- [ ] **Step 7: Commit**

```powershell
git add apps/desktop/src-tauri/src/commands/feed.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/commands/subscription.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: add public feed services Tauri command"
```

### Task 4: SubscriptionDialog (add + edit + feed builder)

**Files:**
- Create: `apps/desktop/src/features/subscription-list/SubscriptionDialog.tsx`
- Create: `apps/desktop/src/features/subscription-list/subscription-dialog.test.tsx`

**Interfaces:**
- Consumes: `Subscription`, `FeedService`, `FeedServiceListSchema`, `SubscriptionSchema` from `@mglaroute/contracts`; `invoke` from `../../lib/invoke`; `Dialog` from `../../components/ui/dialog`; `Tabs`, `TabsList`, `TabsTrigger`, `TabsContents`, `TabsContent` from `../../components/ui/tabs`; `Input`, `Button`, `Switch` wrappers; `useAppStore` notice setter.
- Produces: `SubscriptionDialogProps = { open: boolean; mode: "add" | "edit"; subscription: Subscription | null; onClose: () => void; onSaved: (saved: Subscription) => void }` and `SubscriptionDialog`. Task 5 renders it conditionally only when open so state resets per open.

- [ ] **Step 1: Write failing dialog tests**

Create `subscription-dialog.test.tsx` with the shared mock and fixtures:

```tsx
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { FeedServiceListSchema, SubscriptionSchema, type Subscription } from "@mglaroute/contracts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/invoke", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/invoke")>()),
  invoke: vi.fn(),
}));

import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";
import { SubscriptionDialog } from "./SubscriptionDialog";

const mockedInvoke = vi.mocked(invoke);

const feedUrl = "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&search=alpha&protocol=vless&reachable=true&anycast=false&services=svc-1%2Csvc-2&sort=latency_desc&limit=50";

const subscription: Subscription = {
  id: "sub-1",
  name: "Primary feed",
  url: "https://secret.example/sub",
  enabled: true,
  status: "active",
  autoRefetchMinutes: 60,
  lastFetchedAt: null,
  lastError: null,
  userAgent: null,
  requestHeaders: null,
  sort: 0,
  createdAt: "2026-11-24T00:00:00Z",
  updatedAt: "2026-11-24T00:00:00Z",
};
```

Add `afterEach(cleanup)` and `beforeEach(() => { vi.clearAllMocks(); useAppStore.setState({ notice: null }); })`. Mock `invoke` per test.

Test 1 — edit non-feed URL sends only id/name/url and does not refresh:

```tsx
  it("edits name and URL without touching enabled or interval", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "subscription_update") return { ...subscription, name: "Renamed", url: "https://new.example/sub" } as never;
      return [] as never;
    });
    render(<SubscriptionDialog open mode="edit" subscription={subscription} onClose={vi.fn()} onSaved={vi.fn()} />);
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Renamed" } });
    fireEvent.change(dialog.getByLabelText("Subscription URL"), { target: { value: "https://new.example/sub" } });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1", name: "Renamed", url: "https://new.example/sub",
      }),
    );
    expect(mockedInvoke).not.toHaveBeenCalledWith("subscription_refresh", expect.anything(), expect.anything());
  });
```

Test 2 — edit feed URL opens Mglaroute tab prefilled and regenerates the link:

```tsx
  it("prefills feed settings from a mglaroute URL and regenerates the link", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "feed_services") return [{ id: "svc-1", name: "YouTube" }, { id: "svc-2", name: "Discord" }] as never;
      if (command === "subscription_update") return { ...subscription, url: feedUrl } as never;
      return [] as never;
    });
    const saved = vi.fn();
    render(
      <SubscriptionDialog open mode="edit" subscription={{ ...subscription, url: feedUrl }} onClose={vi.fn()} onSaved={saved} />,
    );
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByRole("tab", { name: "Mglaroute" }).getAttribute("data-state")).toBe("active");
    expect((dialog.getByLabelText("Search") as HTMLInputElement).value).toBe("alpha");
    expect((dialog.getByLabelText("Protocol") as HTMLSelectElement).value).toBe("vless");
    expect((dialog.getByLabelText("Reachable") as HTMLSelectElement).value).toBe("true");
    expect((dialog.getByLabelText("Anycast") as HTMLSelectElement).value).toBe("false");
    expect((dialog.getByLabelText("Sort") as HTMLSelectElement).value).toBe("latency_desc");
    expect((dialog.getByLabelText("Limit") as HTMLInputElement).value).toBe("50");
    await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("feed_services", FeedServiceListSchema));
    expect((dialog.getByRole("button", { name: "YouTube" }).getAttribute("data-state")).toBe("active"));
    fireEvent.change(dialog.getByLabelText("Search"), { target: { value: "beta" } });
    fireEvent.click(dialog.getByRole("button", { name: "Generate link" }));
    const generated = "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&search=beta&protocol=vless&reachable=true&anycast=false&services=svc-1%2Csvc-2&sort=latency_desc&limit=50";
    expect((dialog.getByLabelText("Generated feed URL") as HTMLInputElement).value).toBe(generated);
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(saved).toHaveBeenCalledWith(expect.objectContaining({ url: generated })));
  });
```

Test 3 — service fetch failure non-fatal:

```tsx
  it("keeps the feed form usable when feed_services fails", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "feed_services") throw { fetch: null };
      return [] as never;
    });
    render(<SubscriptionDialog open mode="add" subscription={null} onClose={vi.fn()} onSaved={vi.fn()} />);
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.click(dialog.getByRole("tab", { name: "Mglaroute" }));
    await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("feed_services", FeedServiceListSchema));
    fireEvent.click(dialog.getByRole("button", { name: "Generate link" }));
    expect((dialog.getByLabelText("Generated feed URL") as HTMLInputElement).value).toBe(
      "https://api.mglaroute.ashen-hermit.space/api/feed?format=subscription&sort=latency_asc&limit=200",
    );
  });
```

Test 4 — validation and add payload:

```tsx
  it("validates before saving and omits blank name on add", async () => {
    mockedInvoke.mockImplementation(async () => ({ ...subscription, id: "sub-2", name: "Subscription" }) as never);
    render(<SubscriptionDialog open mode="add" subscription={null} onClose={vi.fn()} onSaved={vi.fn()} />);
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));
    expect(mockedInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().notice).toContain("Subscription URL is required");

    fireEvent.change(dialog.getByLabelText("Subscription URL"), { target: { value: "https://feed.example/sub" } });
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_add", SubscriptionSchema, {
        url: "https://feed.example/sub",
      }),
    );
  });
```

Test 5 — edit requires a name:

```tsx
  it("rejects an empty name on edit", async () => {
    render(<SubscriptionDialog open mode="edit" subscription={subscription} onClose={vi.fn()} onSaved={vi.fn()} />);
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "  " } });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    expect(mockedInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().notice).toContain("Name is required");
  });
```

Import `waitFor` from `@testing-library/react` in the test file.

- [ ] **Step 2: Run failing dialog test**

```powershell
pnpm --filter @mglaroute/desktop exec vitest run src/features/subscription-list/subscription-dialog.test.tsx
```

Expected: FAIL — `SubscriptionDialog.tsx` does not exist.

- [ ] **Step 3: Implement `SubscriptionDialog.tsx`**

Component contract:
- `const [name, setName] = useState(subscription?.name ?? "")`;
- `const [url, setUrl] = useState(subscription?.url ?? "")` — this single URL string is edited on the URL tab and overwritten by `Generate link` on the Mglaroute tab;
- `const [tab, setTab] = useState<"url" | "mglaroute">(isFeedUrl(subscription?.url) ? "mglaroute" : "url")`;
- `const [form, setForm] = useState<FeedForm>(feedFormFromUrl(subscription?.url))`;
- `const [services, setServices] = useState<FeedService[] | null>(null)`;
- `const [generatedUrl, setGeneratedUrl] = useState(isFeedUrl(subscription?.url) ? subscription!.url : "")`;
- `Dialog` props: `open`, `onClose`, `title={mode === "add" ? "Add subscription" : "Edit subscription"}`, `contentClassName="inset-0 m-auto max-h-[90vh] w-[calc(100%-2rem)] max-w-2xl overflow-y-auto rounded-xl border border-border bg-card p-6 text-foreground"`.

Define in the same file:

```ts
const FEED_BASE_URL = "https://api.mglaroute.ashen-hermit.space/api/feed";
const FEED_HOST = "api.mglaroute.ashen-hermit.space";
const SORTS = [
  "latency_asc", "latency_desc", "confidence_asc", "confidence_desc",
  "name_asc", "name_desc", "address_asc", "address_desc",
  "port_asc", "port_desc", "protocol_asc", "protocol_desc",
] as const;
type FeedForm = {
  protocol: string; reachable: string; anycast: string; sort: string;
  search: string; limit: string; services: string[];
};
const INITIAL_FEED_FORM: FeedForm = {
  protocol: "", reachable: "", anycast: "", sort: "latency_asc",
  search: "", limit: "200", services: [],
};
```

Helpers (exact behavior):
- `isFeedUrl(raw?: string | null): boolean` — parse with `new URL(raw ?? "")`, return true only when `url.hostname === FEED_HOST && url.pathname === "/api/feed"`; return false on parse error.
- `feedFormFromUrl(raw?: string | null): FeedForm` — start from `INITIAL_FEED_FORM`; when `isFeedUrl` is true, read `search`, `protocol`, `reachable`, `anycast`, `sort`, `limit`, `services` from `URLSearchParams`. Accept sort only if in `SORTS`, else `latency_asc`. Parse limit with `Number.parseInt`; clamp to `1..1000`, else `200`. Split `services` by comma, trim and drop empties. Ignore `format` and all unknown params.
- `buildFeedUrl(form: FeedForm): string` — create `URLSearchParams`; set `format=subscription` first; then `search` (only if trimmed non-empty), `protocol`, `reachable`, `anycast`, `services` (comma-joined, only if non-empty), `sort`, `limit` (clamped as above); return `${FEED_BASE_URL}?${params.toString()}`.

Rendering:
- Name input with `label htmlFor="subscription-name"` text `Name` and `Input id="subscription-name"`.
- `Tabs value={tab} onValueChange={(v) => setTab(v as "url" | "mglaroute")}` with `TabsList` triggers `URL` (`value="url"`) and `Mglaroute` (`value="mglaroute"`), `TabsContents` with matching `TabsContent`s.
- URL tab: `label htmlFor="subscription-url"` text `Subscription URL`, `Input id="subscription-url"` placeholder `https://example.com/subscription`, value `url`.
- Mglaroute tab: native `<select>` for Protocol (options All/vless/vmess/ss/trojan), Reachable (All/true/false), Anycast (All/true/false), Sort (all `SORTS` values), `<Input type="number" min={1} max={1000}>` for Limit, `Input` for Search. Labels exactly `Protocol`, `Reachable`, `Anycast`, `Sort`, `Limit`, `Search`; `htmlFor` matches control ids `feed-protocol`, `feed-reachable`, `feed-anycast`, `feed-sort`, `feed-limit`, `feed-search`.
- Service chips: effect when `tab === "mglaroute" && services === null` invokes `("feed_services", FeedServiceListSchema)`; success stores rows, rejection stores `[]` (non-fatal). Render label `Online services (all selected must be reachable)` and `Button` per service with `variant={form.services.includes(id) ? "primary" : "secondary"}`, `data-state={... ? "active" : "inactive"}`, text `service.name`. Clicking toggles id in `form.services`.
- `Button` `Generate link`: sets `generatedUrl = buildFeedUrl(form)` and `url = generatedUrl`.
- `Input readOnly aria-label="Generated feed URL" value={generatedUrl}`; on focus select all. Render warning paragraph with exact text: `Anyone with this link can read the feed. It exposes server connections without authentication — share carefully.`
- Save button label `Add subscription` (add) or `Save changes` (edit). On click: trim `url`; if empty set notice `"Subscription URL is required"` and stop. If edit and `name.trim()` empty, set notice `"Name is required"` and stop. For add, args are `name.trim() ? { url: trimmed, name: name.trim() } : { url: trimmed }`; invoke `subscription_add`. For edit, invoke `subscription_update` with exactly `{ id: subscription!.id, name: name.trim(), url: trimmed }`. Catch any error → `setNotice(mode === "add" ? "Could not add subscription" : "Could not update subscription")`; do not call `onSaved`. Success → `onSaved(result)`.
- Import `setNotice` via `useAppStore((store) => store.setNotice)`.

- [ ] **Step 4: Run dialog test**

```powershell
pnpm --filter @mglaroute/desktop exec vitest run src/features/subscription-list/subscription-dialog.test.tsx
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```powershell
git add apps/desktop/src/features/subscription-list/SubscriptionDialog.tsx apps/desktop/src/features/subscription-list/subscription-dialog.test.tsx
git commit -m "feat: add subscription dialog with mglaroute feed builder"
```

### Task 5: SubscriptionList becomes cards and uses the dialog

**Files:**
- Modify: `apps/desktop/src/features/subscription-list/SubscriptionList.tsx`
- Rewrite: `apps/desktop/src/features/subscription-list/subscription-list.test.tsx`

**Interfaces:**
- Consumes: `SubscriptionDialog` props from Task 4; existing `subscription_list`, `subscription_refresh`, `subscription_remove`, `subscription_update` invoke shapes; `SubscriptionSchema`, `RefreshResultSchema`.
- Produces: `SubscriptionList` with page header button `Add subscription`, one `Card` per subscription, and no inline add form.

- [ ] **Step 1: Rewrite failing list tests**

Replace the inline-form assertions in `subscription-list.test.tsx` with modal/card assertions. Keep `errored`, the `vi.mock("../../lib/invoke", ...)`, `beforeEach`, and the error-code table unchanged. Add `waitFor` to the testing-library import if absent. Replace the first test and the container-query test with:

```tsx
  it("opens the add dialog and posts URL plus optional name", async () => {
    render(<SubscriptionList />);
    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: "Add subscription" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Subscription URL"), {
      target: { value: "https://feed.example/sub" },
    });
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Added feed" } });
    fireEvent.click(dialog.getByRole("button", { name: "Add subscription" }));

    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_add", SubscriptionSchema, {
        url: "https://feed.example/sub",
        name: "Added feed",
      }),
    );
  });

  it("renders one card per subscription and opens edit prefilled", async () => {
    mockedInvoke.mockImplementation(async (command, _schema, args) => {
      if (command === "subscription_update") {
        const payload = args as Record<string, unknown>;
        return { ...errored, name: String(payload.name) } as never;
      }
      return [errored] as never;
    });
    render(<SubscriptionList />);
    await waitFor(() => expect(screen.getByText("Primary feed")).toBeTruthy());
    expect(document.querySelectorAll('[data-slot="card"]')).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: /edit primary feed/i }));
    const dialog = within(await screen.findByRole("dialog"));
    expect((dialog.getByLabelText("Name") as HTMLInputElement).value).toBe("Primary feed");
    expect((dialog.getByLabelText("Subscription URL") as HTMLInputElement).value).toBe(
      "https://secret.example/sub",
    );
    fireEvent.change(dialog.getByLabelText("Name"), { target: { value: "Renamed" } });
    fireEvent.click(dialog.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(mockedInvoke).toHaveBeenCalledWith("subscription_update", SubscriptionSchema, {
        id: "sub-1",
        name: "Renamed",
        url: "https://secret.example/sub",
      }),
    );
  });
```

In `subscription_enable_and_interval_wired`, delete the final inline-add block (lines that change `getByPlaceholderText(/example\.com\/subscription/i)` and click `Add subscription`); keep switch and interval assertions. Keep `maps refresh error ...` unchanged. Delete `uses container queries ...` entirely.

- [ ] **Step 2: Run failing list test**

```powershell
pnpm --filter @mglaroute/desktop exec vitest run src/features/subscription-list/subscription-list.test.tsx
```

Expected: FAIL — no dialog, no Edit button, still one card around the list.

- [ ] **Step 3: Implement card layout + dialog wiring**

In `SubscriptionList.tsx`:
- remove `url` and `name` local state and the inline add form/`add()` function;
- add `const [dialog, setDialog] = useState<{ mode: "add" | "edit"; subscription: Subscription | null } | null>(null);`;
- keep `intervals`, `showRows`, `useEffect` load, `refresh`, `remove`, `setEnabled`, `commitInterval`, `badgeStyles`, and every error notice unchanged;
- add `onSaved` handler:

```ts
  const onSaved = (saved: Subscription) => {
    setDialog(null);
    const exists = subscriptions.some((row) => row.id === saved.id);
    showRows(
      exists
        ? subscriptions.map((row) => (row.id === saved.id ? saved : row))
        : [...subscriptions, saved],
    );
  };
```

- root element becomes a `<div className="space-y-3">` containing a page header `<h2 className="text-lg font-semibold">Subscriptions</h2>` and `<Button onClick={() => setDialog({ mode: "add", subscription: null })}>Add subscription</Button>`;
- replace the single `Card`/`<ul>` with `{subscriptions.length === 0 ? <p ...>No subscriptions yet.</p> : subscriptions.map(...)}`; each item is its own `<Card key={subscription.id}>` with:
  - `CardHeader`: `CardTitle` name, `data-testid={`subscription-status-${subscription.id}`}` badge using `badgeStyles`, `Button` `aria-label={`Edit ${subscription.name}`}` text `Edit`;
  - `CardContent`: URL text with `className="min-w-0 break-all text-xs text-muted-foreground"` and `title={subscription.url}`; buttons `Refresh`, `Remove`; `Switch` with same `label={`Enabled for ${subscription.name}`}`; interval label `Refetch interval for ${subscription.name} (minutes)` with `break-words`; `Input` type number min 0, `aria-label` same as label, same `intervals` state and `onBlur` handler;
- render at the end: `{dialog && <SubscriptionDialog open mode={dialog.mode} subscription={dialog.subscription} onClose={() => setDialog(null)} onSaved={onSaved} />}`.

Do not change any invoke args for enable/interval/refresh/remove.

- [ ] **Step 4: Run list test**

```powershell
pnpm --filter @mglaroute/desktop exec vitest run src/features/subscription-list/subscription-list.test.tsx
```

Expected: PASS, including badge, enable/interval, refresh-error table, add dialog, and card layout tests.

- [ ] **Step 5: Commit**

```powershell
git add apps/desktop/src/features/subscription-list/SubscriptionList.tsx apps/desktop/src/features/subscription-list/subscription-list.test.tsx
git commit -m "feat: render subscriptions as cards with modal add and edit"
```

### Task 6: Docs and full verification

**Files:**
- Modify: `AGENTS.md` (client worktree)

**Interfaces:**
- Consumes: behavior delivered in Tasks 1–5.
- Produces: accurate client status/verification notes; both worktrees green.

- [ ] **Step 1: Update client AGENTS.md**

Add a short paragraph after the release/updater status entry:

```markdown
Subscriptions panel refactor implemented: add/edit happen in one modal with
`URL` and `Mglaroute` tabs; the latter builds a `format=subscription` feed link
for `https://api.mglaroute.ashen-hermit.space/api/feed` using enabled services
from the public `GET /api/feed/services` endpoint (Rust `feed_services`, no
WebView fetch). Editing a matching feed URL prefills the form. Subscriptions
render as separate cards. URL edits do not auto-refetch. Design (status
`accepted`): `docs/superpowers/specs/2026-11-28-subscription-manager-feed-design.md`.
Plan: `docs/superpowers/plans/2026-11-28-subscription-manager-feed.md`.
```

- [ ] **Step 2: Run full client JS suite**

```powershell
$env:NX_DAEMON='false'; pnpm test
```

Expected: PASS (baseline was 162 tests; count grows by new dialog/contract tests). `Cannot switch to pnpm@9` and zod annotation warnings are pre-existing.

- [ ] **Step 3: Run full Rust suite**

```powershell
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1
```

Expected: PASS, including `feed_services` command-surface assertion.

- [ ] **Step 4: Run full server backend suite**

```powershell
pnpm --filter mglaroute-vpn-server-backend exec jest --runInBand
```

Expected: PASS (baseline was 108 tests in 17 suites; grows by new feed tests).

- [ ] **Step 5: Commit docs**

```powershell
git add AGENTS.md
git commit -m "docs: record subscription manager feed builder"
```
