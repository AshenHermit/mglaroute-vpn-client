import { Ellipsis, Power, Server } from "lucide-react";

export type BottomNavProps = {
  active: "connect" | "servers" | "more";
  onConnect: () => void;
  onServers: () => void;
  onMore: () => void;
};

const tabClass =
  "flex min-h-11 min-w-11 flex-1 touch-manipulation flex-col items-center justify-center gap-0.5 rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Portrait-only tab bar: hidden by CSS at `sm` and up, no JS media query. */
export function BottomNav({ active, onConnect, onServers, onMore }: BottomNavProps) {
  return (
    <nav
      aria-label="Bottom navigation"
      data-testid="bottom-nav"
      className="pb-safe fixed inset-x-0 bottom-0 z-50 flex items-stretch justify-around border-t border-border bg-card sm:hidden"
    >
      <button
        type="button"
        data-testid="bottom-nav-connect"
        aria-label="Connect"
        aria-current={active === "connect" ? "page" : undefined}
        onClick={onConnect}
        className={`${tabClass} ${active === "connect" ? "text-primary" : ""}`}
      >
        <Power aria-hidden="true" className="size-5" />
        <span>Connect</span>
      </button>
      <button
        type="button"
        data-testid="bottom-nav-servers"
        aria-label="Servers"
        aria-current={active === "servers" ? "page" : undefined}
        onClick={onServers}
        className={`${tabClass} ${active === "servers" ? "text-primary" : ""}`}
      >
        <Server aria-hidden="true" className="size-5" />
        <span>Servers</span>
      </button>
      <button
        type="button"
        data-testid="bottom-nav-more"
        aria-label="More"
        aria-current={active === "more" ? "page" : undefined}
        onClick={onMore}
        className={`${tabClass} ${active === "more" ? "text-primary" : ""}`}
      >
        <Ellipsis aria-hidden="true" className="size-5" />
        <span>More</span>
      </button>
    </nav>
  );
}
