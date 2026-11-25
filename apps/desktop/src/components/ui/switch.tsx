export type SwitchProps = {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
};

/** Accessible switch implemented with a native button; no extra dependency. */
export function Switch({
  checked,
  onCheckedChange,
  label,
  disabled = false,
}: SwitchProps) {
  return (
    <label className="flex items-center gap-2 text-sm text-slate-700">
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onCheckedChange(!checked)}
        className={`relative h-5 w-9 rounded-full transition-colors disabled:opacity-50 ${checked ? "bg-sky-700" : "bg-slate-300"}`}
      >
        <span
          className={`absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-all ${checked ? "left-[18px]" : "left-0.5"}`}
        />
      </button>
      <span>{label}</span>
    </label>
  );
}
