import { CircleCheck, Info, Loader2, OctagonX, TriangleAlert } from "lucide-react";
import type { CSSProperties, ComponentProps } from "react";
import { Toaster as Sonner } from "sonner";

type ToasterProps = ComponentProps<typeof Sonner>;

/** shadcn Sonner wrapper, pinned to the app's dark neutral tokens. */
export function Toaster(props: ToasterProps) {
  return (
    <Sonner
      theme="dark"
      position="bottom-right"
      offset={16}
      className="toaster group"
      icons={{
        success: <CircleCheck className="size-4" />,
        info: <Info className="size-4" />,
        warning: <TriangleAlert className="size-4" />,
        error: <OctagonX className="size-4" />,
        loading: <Loader2 className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover, oklch(0.21 0 0))",
          "--normal-text": "var(--popover-foreground, oklch(0.93 0 0))",
          "--normal-border": "var(--border)",
          "--success-color": "var(--primary)",
          "--error-color": "var(--destructive)",
          "--border-radius": "var(--radius)",
        } as CSSProperties
      }
      {...props}
    />
  );
}
