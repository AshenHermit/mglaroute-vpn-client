import * as SwitchPrimitive from "@animate-ui/react/primitives/radix-ui/switch";
import { cn } from "../../lib/utils";

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
    <label className="flex min-h-11 items-center gap-2 text-sm text-foreground">
      <SwitchPrimitive.Root
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        aria-label={label}
        className={cn(
          "relative h-5 w-9 rounded-full transition-colors disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
          checked ? "bg-primary" : "bg-muted",
        )}
      >
        <SwitchPrimitive.Thumb
          className={cn(
            "block h-4 w-4 rounded-full bg-background shadow",
            checked ? "translate-x-[18px]" : "translate-x-0.5",
          )}
        />
      </SwitchPrimitive.Root>
      <span>{label}</span>
    </label>
  );
}
