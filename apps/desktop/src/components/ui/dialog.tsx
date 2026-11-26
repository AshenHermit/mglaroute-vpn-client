import * as DialogPrimitive from "@radix-ui/react-dialog";
import type { ReactNode } from "react";

export type DialogProps = {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
};

/** Local dialog built on the Radix Dialog primitive; no secrets in default UI. */
export function Dialog({ open, onClose, title, children }: DialogProps) {
  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-slate-900/50" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 w-full max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-lg bg-white p-6 shadow-xl focus:outline-none"
        >
          <div className="mb-4 flex items-center justify-between gap-4">
            <DialogPrimitive.Title className="text-lg font-semibold">
              {title}
            </DialogPrimitive.Title>
            <DialogPrimitive.Close
              aria-label="Close dialog"
              className="rounded px-2 text-slate-500 hover:bg-slate-100"
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
