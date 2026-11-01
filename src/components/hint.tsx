import * as React from "react";
import { Tooltip as TooltipPrimitive } from "radix-ui";
import { Kbd } from "@/components/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type Props = {
  /** What shows on hover; nothing (no tooltip) when empty. */
  text?: string | null;
  /** A shortcut shown in a Kbd, e.g. "Ctrl T". */
  keys?: string;
  children: React.ReactElement<{ disabled?: boolean; className?: string }>;
  /** Classes for the hover box a disabled control is wrapped in (it can't be hovered itself). */
  wrapClassName?: string;
} & Pick<React.ComponentProps<typeof TooltipContent>, "side" | "align">;

/**
 * Gond's hover text, as a shadcn tooltip rather than the browser's:
 * `<Hint text="Close pane" keys="Ctrl ⇧ W"><button …/></Hint>`. Its trigger carries `data-hint`,
 * so the full text of a truncated label is in the DOM for agents and tests. A control that can be
 * disabled always gets a hoverable box around it (disabled elements don't hover), even while
 * enabled, so turning it on and off never remounts it.
 */
export function Hint({ text, keys, children, side, align, wrapClassName }: Props) {
  const canDisable = "disabled" in children.props;
  if (!text && !canDisable) return children;
  const trigger = canDisable ? (
    <span data-hint={text} className={cn("inline-flex", wrapClassName)}>
      {children}
    </span>
  ) : (
    React.cloneElement(children, { "data-hint": text } as Record<string, unknown>)
  );
  // The same tree with or without text, so a reason coming and going never remounts the control.
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <Tooltip open={text ? undefined : false}>
        <TooltipTrigger asChild>{trigger}</TooltipTrigger>
        {text && (
          <TooltipContent side={side} align={align} className="max-w-sm break-words">
            {text}
            {keys && <Kbd>{keys}</Kbd>}
          </TooltipContent>
        )}
      </Tooltip>
    </TooltipPrimitive.Provider>
  );
}
