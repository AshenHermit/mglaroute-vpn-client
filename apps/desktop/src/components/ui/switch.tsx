import * as SwitchPrimitive from "@animate-ui/react/primitives/radix-ui/switch";
import { cn } from "../../lib/utils";

export type SwitchProps = {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
};

/** Accessible switch built on the animated Animate UI Radix Switch primitive. */
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
        data-slot="switch"
        className={cn(
          "relative h-5 w-9 shrink-0 rounded-full disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
          checked ? "bg-primary" : "bg-muted",
        )}
      >
        <SwitchPrimitive.Thumb
          data-slot="switch-thumb"
          className="block size-4 rounded-full bg-background shadow"
        />
      </SwitchPrimitive.Root>
      <span>{label}</span>
    </label>
  );
}
