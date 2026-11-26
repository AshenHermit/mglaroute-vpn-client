import { beforeEach, describe, expect, it } from "vitest";
import { useUiStore, type ServerFilter, type SheetTab } from "./ui.store";

const defaults = {
  favorites: [] as string[],
  filter: { kind: "all" } as ServerFilter,
  sheetTab: "subscriptions" as SheetTab,
};

describe("ui store", () => {
  beforeEach(() => {
    localStorage.clear();
    useUiStore.setState({ ...defaults });
  });

  it("starts with non-secret defaults", () => {
    const state = useUiStore.getState();
    expect(state.favorites).toEqual([]);
    expect(state.filter.kind).toBe("all");
    expect(state.sheetTab).toBe("subscriptions");
  });

  it("toggleFavorite adds then removes an id", () => {
    useUiStore.getState().toggleFavorite("p-1");
    expect(useUiStore.getState().favorites).toEqual(["p-1"]);

    useUiStore.getState().toggleFavorite("p-1");
    expect(useUiStore.getState().favorites).toEqual([]);
  });

  it("setFilter replaces the previous filter", () => {
    const first: ServerFilter = { kind: "subscription", subscriptionId: "sub-1" };
    const second: ServerFilter = { kind: "reachable" };

    useUiStore.getState().setFilter(first);
    expect(useUiStore.getState().filter).toEqual(first);

    useUiStore.getState().setFilter(second);
    expect(useUiStore.getState().filter).toEqual(second);
  });

  it.each<SheetTab>(["subscriptions", "serviceProfiles", "settings"])(
    "setSheetTab accepts %s",
    (tab) => {
      useUiStore.getState().setSheetTab(tab);
      expect(useUiStore.getState().sheetTab).toBe(tab);
    },
  );

  it("persists only non-secret keys", () => {
    useUiStore.getState().toggleFavorite("p-1");
    useUiStore.getState().setFilter({ kind: "subscription", subscriptionId: "sub-1" });

    const raw = localStorage.getItem("mglaroute-ui");
    expect(raw).not.toBeNull();
    const persisted = JSON.parse(raw as string) as { state: Record<string, unknown> };
    expect(Object.keys(persisted.state).sort()).toEqual(["favorites", "filter", "sheetTab"]);
    expect(persisted.state.favorites).toEqual(["p-1"]);
    expect(persisted.state.filter).toEqual({
      kind: "subscription",
      subscriptionId: "sub-1",
    });
    expect(raw).not.toContain("vless://");
    expect(raw).not.toContain("rawConfig");
  });
});
