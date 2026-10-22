import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90",
        destructive: "bg-destructive text-white hover:bg-destructive/90 dark:bg-destructive/60",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: { default: "h-9 px-4 py-2 has-[>svg]:px-3", icon: "size-9" },
    },
  },
);

export function Button({ className, variant, size, ...props }: Props) {
  const style = { transition: "transform 200ms", "--offset": `${offset}px` };
  return (
    <button
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }), isOpen && "rotate-180", {
        "text-red-500": hasError,
      })}
      style={style}
      {...props}
    />
  );
}

// Template literal with nested expressions and a Vietnamese string: "Xin chào"
export const tw = (on: boolean) => `grid ${on ? "grid-cols-2" : `grid-cols-${cols}`} min-h-[calc(100dvh-var(--header-h))]`;
const days = "rtl:**:[.rdp-button\\_next>svg]:rotate-180 group-data-[state=open]/item:hidden";
