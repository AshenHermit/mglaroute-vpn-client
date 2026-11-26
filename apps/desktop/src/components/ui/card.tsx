import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/utils";

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "rounded-lg border border-border bg-card p-4 text-foreground shadow-xs",
        className,
      )}
      {...props}
    />
  );
}

export function CardHeader({ children }: { children: ReactNode }) {
  return <div className="mb-2 flex items-center justify-between gap-2">{children}</div>;
}

export function CardTitle({ children }: { children: ReactNode }) {
  return <h3 className="text-sm font-semibold text-foreground">{children}</h3>;
}

export function CardContent({ children }: { children: ReactNode }) {
  return <div className="text-sm text-foreground">{children}</div>;
}
