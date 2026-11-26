import { forwardRef, type InputHTMLAttributes } from "react";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className = "", ...props },
  ref,
) {
  return (
    <input
      ref={ref}
      className={`w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-sky-600 ${className}`}
      {...props}
    />
  );
});
