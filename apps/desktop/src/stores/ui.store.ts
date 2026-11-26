import { create } from "zustand";
import { persist } from "zustand/middleware";

/** Server list filter. `subscriptionId` is only set for `kind: "subscription"`. */
export type ServerFilter = {
  kind: "all" | "favorites" | "reachable" | "subscription";
  subscriptionId?: string;
};

export type UiStore = {
  favorites: string[];
  filter: ServerFilter;
  toggleFavorite: (id: string) => void;
  setFilter: (filter: ServerFilter) => void;
};

/**
 * UI preferences only. Persist exactly the two non-secret keys below via
 * `partialize`: never subscription URLs/links, credentials, raw config, or
 * any other durable data owned by Rust.
 */
export const useUiStore = create<UiStore>()(
  persist(
    (set) => ({
      favorites: [],
      filter: { kind: "all" },
      toggleFavorite: (id) =>
        set((state) => ({
          favorites: state.favorites.includes(id)
            ? state.favorites.filter((favorite) => favorite !== id)
            : [...state.favorites, id],
        })),
      setFilter: (filter) => set({ filter }),
    }),
    {
      name: "mglaroute-ui",
      partialize: (state) => ({
        favorites: state.favorites,
        filter: state.filter,
      }),
    },
  ),
);
