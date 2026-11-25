import type {
  TableHTMLAttributes,
  TdHTMLAttributes,
  ThHTMLAttributes,
  HTMLAttributes,
} from "react";

export function Table({ className = "", ...props }: TableHTMLAttributes<HTMLTableElement>) {
  return <table className={`w-full border-collapse text-sm ${className}`} {...props} />;
}

export function TableHead({ className = "", ...props }: HTMLAttributes<HTMLTableSectionElement>) {
  return <thead className={`bg-slate-100 text-left ${className}`} {...props} />;
}

export function TableBody({ className = "", ...props }: HTMLAttributes<HTMLTableSectionElement>) {
  return <tbody className={className} {...props} />;
}

export function TableRow({ className = "", ...props }: HTMLAttributes<HTMLTableRowElement>) {
  return <tr className={`border-b border-slate-200 ${className}`} {...props} />;
}

export function TableHeaderCell({
  className = "",
  ...props
}: ThHTMLAttributes<HTMLTableCellElement>) {
  return <th className={`px-3 py-2 font-semibold ${className}`} {...props} />;
}

export function TableCell({ className = "", ...props }: TdHTMLAttributes<HTMLTableCellElement>) {
  return <td className={`px-3 py-2 ${className}`} {...props} />;
}
