import type { HTMLAttributes, ReactNode } from "react";

export function Card({ className = "", ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={`rounded-lg border border-slate-200 bg-white p-4 shadow-sm ${className}`}
      {...props}
    />
  );
}

export function CardHeader({ children }: { children: ReactNode }) {
  return <div className="mb-2 flex items-center justify-between gap-2">{children}</div>;
}

export function CardTitle({ children }: { children: ReactNode }) {
  return <h3 className="text-sm font-semibold text-slate-700">{children}</h3>;
}

export function CardContent({ children }: { children: ReactNode }) {
  return <div className="text-sm text-slate-900">{children}</div>;
}
