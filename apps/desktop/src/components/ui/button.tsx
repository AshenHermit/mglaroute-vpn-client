import {
  Button as ButtonPrimitive,
  type ButtonProps as ButtonPrimitiveProps,
} from "@animate-ui/react/primitives/buttons/button";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "inline-flex min-h-11 items-center justify-center rounded-md px-3 py-2 text-sm font-medium disabled:cursor-not-allowed disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
  {
    variants: {
      variant: {
        primary: "bg-primary text-primary-foreground hover:bg-primary/90",
        secondary: "bg-muted text-foreground hover:bg-muted/80",
        danger: "bg-destructive text-background hover:bg-destructive/90",
      },
    },
    defaultVariants: {
      variant: "primary",
    },
  },
);

export type ButtonProps = ButtonPrimitiveProps &
  VariantProps<typeof buttonVariants>;

/** shadcn-style button on the animated Animate UI button primitive. */
export function Button({
  variant,
  type = "button",
  className,
  hoverScale = 0.98,
  tapScale = 0.96,
  ...props
}: ButtonProps) {
  return (
    <ButtonPrimitive
      type={type}
      data-slot="button"
      className={cn(buttonVariants({ variant }), className)}
      hoverScale={hoverScale}
      tapScale={tapScale}
      {...props}
    />
  );
}
