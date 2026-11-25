import * as SwitchPrimitive from "@radix-ui/react-switch";

export type SwitchProps = {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
};

/** Accessible switch built on the Radix Switch primitive. */
export function Switch({
  checked,
  onCheckedChange,
  label,
  disabled = false,
}: SwitchProps) {
  return (
    <label className="flex items-center gap-2 text-sm text-slate-700">
      <SwitchPrimitive.Root
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        aria-label={label}
        className={`relative h-5 w-9 rounded-full transition-colors disabled:opacity-50 ${
          checked ? "bg-sky-700" : "bg-slate-300"
        }`}
      >
        <SwitchPrimitive.Thumb
          className={`block h-4 w-4 rounded-full bg-white shadow transition-all ${
            checked ? "translate-x-[18px]" : "translate-x-0.5"
          }`}
        />
      </SwitchPrimitive.Root>
      <span>{label}</span>
    </label>
  );
}
