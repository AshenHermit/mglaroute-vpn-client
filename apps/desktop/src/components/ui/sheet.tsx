import * as DialogPrimitive from "@animate-ui/react/primitives/radix-ui/dialog";
import type { ReactNode } from "react";
import { cn } from "../../lib/utils";

export type SheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
  side?: "left" | "right";
  className?: string;
  closeLabel?: string;
};

/** Animated side sheet built on the Animate UI Radix Dialog primitive. */
export function Sheet({
  open,
  onOpenChange,
  title,
  children,
  side = "right",
  className,
  closeLabel = "Close",
}: SheetProps) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="sheet-overlay"
          className="fixed inset-0 z-50 bg-background/80 backdrop-blur-sm"
        />
        <DialogPrimitive.Content
          data-slot="sheet-content"
          aria-describedby={undefined}
          className={cn(
            "fixed inset-y-0 z-50 flex w-full max-w-[min(480px,100vw)] flex-col overflow-hidden border-border bg-card p-0 text-foreground shadow-xl focus:outline-hidden",
            side === "right" ? "right-0 border-l" : "left-0 border-r",
            className,
          )}
        >
          <div className="flex shrink-0 items-center justify-between gap-4 border-b border-border p-4">
            <DialogPrimitive.Title className="text-base font-semibold text-foreground">
              {title}
            </DialogPrimitive.Title>
            <DialogPrimitive.Close
              aria-label={closeLabel}
              className="inline-flex min-h-11 min-w-11 items-center justify-center rounded-md px-2 text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
            >
              ×
            </DialogPrimitive.Close>
          </div>
          {children}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
