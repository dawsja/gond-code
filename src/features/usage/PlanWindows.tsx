import { windowLine, type PlanUsage } from "@/lib/usage";
import { cn } from "@/lib/utils";

/** A bar that fills as the plan is used: green, then amber, then red. */
export function Bar({ used }: { used: number }) {
  const v = Math.max(0, Math.min(100, used));
  const left = 100 - v;
  const tone = left > 40 ? "bg-tone-ok" : left > 15 ? "bg-tone-warn" : "bg-tone-bad";
  return (
    <span className="block h-1.5 overflow-hidden rounded-full bg-hover">
      <span className={cn("block h-full rounded-full", tone)} style={{ width: `${v}%` }} />
    </span>
  );
}

/**
 * Every window of an agent's plan, one under another: its name, how much is used, when it
 * resets, and its bar. Settings → Usage's cards and the sidebar's usage card both show it.
 */
export function PlanWindows({
  windows,
  className,
}: {
  windows: NonNullable<PlanUsage["windows"]>;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-3", className)}>
      {windows.map((w) => {
        const line = windowLine(w);
        return (
          <div key={`${w.name}-${w.label ?? ""}`} className="flex flex-col gap-1">
            <p className="flex items-baseline gap-2 text-[12.5px]">
              <span className="font-medium">{line.label}</span>
              <span className="text-muted-foreground">{line.text}</span>
              {line.resets && <span className="ml-auto text-faint">{line.resets}</span>}
            </p>
            <Bar used={line.used} />
          </div>
        );
      })}
    </div>
  );
}
