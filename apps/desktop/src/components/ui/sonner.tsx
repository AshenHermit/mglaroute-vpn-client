import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react";
import type { CSSProperties } from "react";
import { Toaster as Sonner, type ToasterProps } from "sonner";

/** shadcn Sonner wrapper, pinned to the app's dark neutral tokens. */
const Toaster = ({ ...props }: ToasterProps) => (
  <Sonner
    theme="dark"
    position="bottom-right"
    offset={16}
    mobileOffset={{ bottom: "calc(72px + env(safe-area-inset-bottom))" }}
    className="toaster group"
    icons={{
      success: <CircleCheckIcon className="size-4" />,
      info: <InfoIcon className="size-4" />,
      warning: <TriangleAlertIcon className="size-4" />,
      error: <OctagonXIcon className="size-4" />,
      loading: <Loader2Icon className="size-4 animate-spin" />,
    }}
    style={
      {
        "--width": "min(360px, calc(100vw - 24px))",
        "--normal-bg": "var(--popover)",
        "--normal-text": "var(--popover-foreground)",
        "--normal-border": "var(--border)",
        "--success-color": "var(--primary)",
        "--error-color": "var(--destructive)",
        "--border-radius": "var(--radius)",
      } as CSSProperties
    }
    {...props}
  />
);

export { Toaster };
