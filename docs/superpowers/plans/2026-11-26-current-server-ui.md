# Current Server UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One active server shown on Dashboard; a power icon in each server row sets it active and switches the live VPN when connected.

**Architecture:** Frontend-only. `selectedProfileId` remains the session-only active target. A new `Power` button in `ServerTable` is the only row interaction. `Servers.tsx` additionally calls `connection_connect({ profileId })` when `ConnectionState.status === "connected"`. Dashboard derives active id as `connection.profileId ?? selectedProfileId`. Add `lucide-react`.

**Tech Stack:** TypeScript, React, Zustand, Tailwind, lucide-react, Vitest, Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-26-current-server-ui-design.md`

## Global Constraints

- No Rust or contract changes. `connection_connect` already supports switching from `connected` (see `commands/connection.rs:43-112,297-303`).
- `Connect` / `Disconnect` panel stays unchanged and remains the only way to start or stop the global connection while disconnected.
- Row body must not be clickable; only the power button changes the active server.
- Active id = `connection.state.profileId ?? selectedProfileId`. When connected, backend state wins.
- Button states: `inactive` (white), `active` (gray, VPN off), `connected` (green). Expose as `data-state` for tests.
- Do not persist `selectedProfileId` or auto-connect.
- Conventional Commits; no AI attribution.

## Review Focus

- Connected VPN + power click on another row: `connection_connect` invoked with that profile id — Task 2 test `switches the live connection when power is clicked while connected`.
- No connection + power click: only `selectedProfileId` changes, no `connection_connect` — Task 2 test `sets the active server without connecting when disconnected`.
- Dashboard after restart with live connection: active server comes from `ConnectionState.profileId` — Task 3 test `dashboard shows the connected server`.
- Three visual states on the row button: inactive/active/connected distinct — Task 1 test `renders three power states`.
- No row-click selection left: clicking row body does nothing — Task 1 test `row body click does not select a server`.

---

### Task 1: Power button in ServerTable

**Files:**
- Modify: `apps/desktop/package.json` (add `lucide-react`)
- Modify: `apps/desktop/src/features/server-table/ServerTable.tsx`
- Test: `apps/desktop/src/features/server-table/server-table.test.tsx`

**Interfaces:**
- Consumes: `profiles: Profile[]`, `testingIds?: string[]`, `selectedId?: string | null`, `onSelect?: (id: string) => void`.
- Produces: new prop `connectedId?: string | null`; power button `data-testid="server-power"`, `data-profile-id`, `data-state="inactive|active|connected"`, `aria-label` naming the action and server. Row `data-active` replaces `data-selected`; row no longer has `role`/keyboard handlers.

- [ ] **Step 1: Install dependency**

From `apps/desktop`: `pnpm add lucide-react`. Expected: `lucide-react` in `dependencies`, lockfile updated.

- [ ] **Step 2: Write failing tests**

Replace `reports a clicked row and reflects the selected id` in `server-table.test.tsx` with:

```tsx
it("row body click does not select a server", () => {
  const onSelect = vi.fn();
  render(<ServerTable profiles={[profile("p-1", 10, true, 0)]} onSelect={onSelect} />);
  fireEvent.click(screen.getByTestId("server-row"));
  expect(onSelect).not.toHaveBeenCalled();
});

it("power button sets the active server", () => {
  const onSelect = vi.fn();
  render(
    <ServerTable
      profiles={[profile("p-1", 10, true, 0), profile("p-2", 20, true, 1)]}
      selectedId="p-1"
      onSelect={onSelect}
    />,
  );
  const buttons = screen.getAllByTestId("server-power");
  const p2 = buttons.find((b) => b.getAttribute("data-profile-id") === "p-2");
  fireEvent.click(p2!);
  expect(onSelect).toHaveBeenCalledWith("p-2");
});

it("renders three power states", () => {
  const { rerender } = render(
    <ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId={null} />,
  );
  expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("inactive");

  rerender(<ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId="p-1" />);
  expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("active");

  rerender(
    <ServerTable profiles={[profile("p-1", 10, true, 0)]} selectedId="p-1" connectedId="p-1" />,
  );
  expect(screen.getByTestId("server-power").getAttribute("data-state")).toBe("connected");
});
```

- [ ] **Step 3: Run RED**

`pnpm exec vitest run src/features/server-table/server-table.test.tsx` from `apps/desktop`. Expected: FAIL (no power button; row role still `button`).

- [ ] **Step 4: Implement**

In `ServerTable.tsx`: add `Power` import; add `connectedId` prop. Compute `activeId = connectedId ?? selectedId`. For each row: `isActive = activeId === profile.id`, `isConnected = connectedId === profile.id`. Set `data-state` = `isConnected ? "connected" : isActive ? "active" : "inactive"`. Remove `role`, `tabIndex`, `aria-pressed`, `onClick`, `onKeyDown`, `cursor-pointer` from `TableRow`; add `data-active`. Add a new leading `TableCell` containing a `<button type="button" data-testid="server-power" data-profile-id={profile.id} data-state={...} aria-label={...} onClick={() => onSelect?.(profile.id)}>` with `<Power aria-hidden="true" className="h-4 w-4" />`. Classes: inactive `border border-slate-300 bg-white text-slate-700`, active `border border-slate-400 bg-slate-400 text-white`, connected `border border-emerald-600 bg-emerald-600 text-white`. Bump empty-row `colSpan` from 5 to 6. `aria-label`: connected → `Connected server: ${name}`, active → `Active server: ${name}`, else `Set active server: ${name}` where `name = profile.remarks ?? profile.serverId`.

- [ ] **Step 5: Run GREEN**

`pnpm exec vitest run src/features/server-table/server-table.test.tsx` from `apps/desktop`. Expected: PASS.

- [ ] **Step 6: Commit**

`git add pnpm-lock.yaml apps/desktop/package.json apps/desktop/src/features/server-table/; git commit -m "feat(servers): add power action per server row"`

---

### Task 2: Wire power action to connection

**Files:**
- Modify: `apps/desktop/src/pages/Servers.tsx`
- Test: `apps/desktop/src/pages/Servers.test.tsx`

**Interfaces:**
- Consumes: `ServerTable` power `onSelect(id)`, `useConnectionStore.state: ConnectionState`, `invoke`.
- Produces: `setActive(profileId: string)` — sets `selectedProfileId`; when `state.status === "connected"` also awaits `invoke("connection_connect", ConnectionStateSchema, { profileId })` and `setState(next)` on success; errors call `setNotice("Connection failed")`. No local optimistic connected state.

- [ ] **Step 1: Write failing tests**

In `Servers.test.tsx`, import `ConnectionStateSchema` from contracts and `useConnectionStore`. Add:

```tsx
it("sets the active server without connecting when disconnected", async () => {
  useConnectionStore.setState({
    state: { status: "disconnected", profileId: null, proxyPort: null },
  });
  mockedInvoke.mockImplementation(async (command) => {
    if (command === "profile_list") return [server, otherServer] as never;
    return await new Promise(() => undefined);
  });
  render(<Servers />);
  await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));
  const p2 = screen.getAllByTestId("server-power").find(
    (b) => b.getAttribute("data-profile-id") === "p-2",
  );
  fireEvent.click(p2!);
  expect(useAppStore.getState().selectedProfileId).toBe("p-2");
  expect(mockedInvoke).not.toHaveBeenCalledWith(
    "connection_connect",
    expect.anything(),
    expect.anything(),
  );
});

it("switches the live connection when power is clicked while connected", async () => {
  useConnectionStore.setState({
    state: { status: "connected", profileId: "p-1", proxyPort: 10809 },
  });
  mockedInvoke.mockImplementation(async (command) => {
    if (command === "profile_list") return [server, otherServer] as never;
    if (command === "connection_connect") {
      return { status: "connected", profileId: "p-2", proxyPort: 10809 } as never;
    }
    return await new Promise(() => undefined);
  });
  render(<Servers />);
  await waitFor(() => expect(useAppStore.getState().profiles).toHaveLength(2));
  const p2 = screen.getAllByTestId("server-power").find(
    (b) => b.getAttribute("data-profile-id") === "p-2",
  );
  fireEvent.click(p2!);
  await waitFor(() =>
    expect(mockedInvoke).toHaveBeenCalledWith(
      "connection_connect",
      ConnectionStateSchema,
      { profileId: "p-2" },
    ),
  );
  expect(useAppStore.getState().selectedProfileId).toBe("p-2");
});
```

Reset `useConnectionStore` state in `beforeEach`.

- [ ] **Step 2: Run RED**

`pnpm exec vitest run src/pages/Servers.test.tsx` from `apps/desktop`. Expected: FAIL (no power button / no connect call).

- [ ] **Step 3: Implement**

In `Servers.tsx`: import `ConnectionStateSchema`, `useConnectionStore`; read `connection = useConnectionStore((store) => store.state)` and `setConnectionState = useConnectionStore((store) => store.setState)`. Add `setActive = async (profileId: string) => { setSelectedProfileId(profileId); if (connection.status !== "connected") return; try { const next = await invoke("connection_connect", ConnectionStateSchema, { profileId }); setConnectionState(next); } catch { setNotice("Connection failed"); } };`. Pass `connectedId={connection.profileId}` and `onSelect={(id) => void setActive(id)}` to `ServerTable`.

- [ ] **Step 4: Run GREEN**

`pnpm exec vitest run src/pages/Servers.test.tsx src/features/server-table/server-table.test.tsx` from `apps/desktop`. Expected: PASS.

- [ ] **Step 5: Commit**

`git add apps/desktop/src/pages/Servers.tsx apps/desktop/src/pages/Servers.test.tsx; git commit -m "feat(servers): switch connection from row power action"`

---

### Task 3: Dashboard active server + full verification

**Files:**
- Modify: `apps/desktop/src/pages/Dashboard.tsx`
- Test: `apps/desktop/src/pages/Dashboard.test.tsx`
- Modify: `AGENTS.md` only if it still says row click selects a server

**Interfaces:**
- Consumes: `useAppStore.profiles`, `selectedProfileId`; `useConnectionStore.state`.
- Produces: `data-testid="active-server"` card with `data-state="none|active|connected"` showing the active profile name.

- [ ] **Step 1: Write failing tests**

In `Dashboard.test.tsx`, import `useConnectionStore`. Reset connection state in `beforeEach`. Add:

```tsx
it("dashboard shows the connected server", async () => {
  useConnectionStore.setState({
    state: { status: "connected", profileId: "p-1", proxyPort: 10809 },
  });
  render(<Dashboard />);
  const active = await screen.findByTestId("active-server");
  expect(active.getAttribute("data-state")).toBe("connected");
  expect(active.textContent).toContain("node 0");
});

it("dashboard shows a selected server when disconnected", async () => {
  act(() => {
    useAppStore.setState({ selectedProfileId: "p-2" });
  });
  render(<Dashboard />);
  const active = await screen.findByTestId("active-server");
  expect(active.getAttribute("data-state")).toBe("active");
  expect(active.textContent).toContain("node 2");
});
```

- [ ] **Step 2: Run RED**

`pnpm exec vitest run src/pages/Dashboard.test.tsx` from `apps/desktop`. Expected: FAIL (no active-server card).

- [ ] **Step 3: Implement**

In `Dashboard.tsx`: import `Power` from `lucide-react`; import `useConnectionStore`. Read `profiles`, `selectedProfileId`, `connection`. Compute `activeId = connection.profileId ?? selectedProfileId`, `active = profiles.find((p) => p.id === activeId) ?? null`, `dataState = !active ? "none" : connection.status === "connected" ? "connected" : "active"`. Insert a card before the metric grid:

```tsx
<Card>
  <CardHeader><CardTitle>Current server</CardTitle></CardHeader>
  <CardContent data-testid="active-server" data-state={dataState} className="flex items-center gap-2">
    <Power aria-hidden="true" className="h-4 w-4" />
    {active ? `${active.remarks ?? active.serverId} — ${connection.status === "connected" ? "Connected" : "Active"}` : "No active server"}
  </CardContent>
</Card>
```

Use green text class when `dataState === "connected"`, slate otherwise.

- [ ] **Step 4: Run GREEN**

`pnpm exec vitest run src/pages/Dashboard.test.tsx src/pages/Servers.test.tsx src/features/server-table/server-table.test.tsx` from `apps/desktop`. Expected: PASS.

- [ ] **Step 5: Full verification**

From worktree root: `pnpm exec nx run-many --target=test --skip-nx-cache`. Expected: contracts 5/5, desktop all green. From `apps/desktop`: `pnpm build`. Expected: clean except pre-existing Zod/Rollup comments.

- [ ] **Step 6: Commit**

`git add apps/desktop/src/pages/Dashboard.tsx apps/desktop/src/pages/Dashboard.test.tsx AGENTS.md; git commit -m "feat(dashboard): show active server and connection state"`
