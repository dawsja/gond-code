import { ProviderIcon } from "@/components/brand/provider-icon";
import { useAppState } from "@/lib/appState";
import { usageKey, windowLine, type PlanUsage } from "@/lib/accounts";
import type { Account, Harness } from "@/lib/harnesses";
import { cn } from "@/lib/utils";
import { Hint } from "@/components/hint";

/** A bar that fills as the plan is used: green, then amber, then red. */
function Bar({ used }: { used: number }) {
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
 * Settings → Usage: every account's plan, as its agent reports it: each
 * window (what's left, when it resets). Gond can't see what an agent spends in its own CLI, so it
 * shows no costs of its own.
 */
export function UsageSettings({
  accounts,
  harnesses,
}: {
  accounts: Account[];
  harnesses: Harness[];
}) {
  const usage = useAppState((t) => t.usage as Record<string, PlanUsage> | undefined);
  const nameOf = (id?: string) => harnesses.find((h) => h.id === id)?.label ?? id ?? "Any agent";
  return (
    <div data-slot="usage" className="flex max-w-[880px] flex-col gap-5">
      <header>
        <h1 className="text-xl font-semibold">Usage</h1>
        <p className="text-sm text-muted-foreground">
          What each account has left of its plan, as its agent reports it.
        </p>
      </header>

      <section aria-label="Accounts" className="grid gap-3 @min-[46rem]:grid-cols-2">
        {accounts.map((a) => {
          const u = usage?.[usageKey(a)];
          const demoWindow =
            a.usage.kind === "window" || a.usage.kind === "weekly" ? a.usage : undefined;
          const windows =
            u?.windows ??
            (demoWindow
              ? [
                  {
                    name: demoWindow.kind === "weekly" ? ("weekly" as const) : ("session" as const),
                    percentLeft: 100 - demoWindow.usedPercent,
                    resetsAt: demoWindow.resetsAt,
                  },
                ]
              : []);
          const nothing = windows.length === 0;
          return (
            <article
              key={a.id}
              data-entity="account-usage"
              data-id={a.id}
              aria-label={a.label}
              className="flex flex-col gap-3 rounded-2xl border border-edge bg-card p-4"
            >
              <div className="flex items-center gap-2.5">
                <ProviderIcon id={a.harness ?? a.provider} size={18} />
                <div className="min-w-0 flex-1">
                  <Hint text={a.label}>
                    <p className="truncate text-sm font-medium">{a.label}</p>
                  </Hint>
                  <p className="truncate text-[12px] text-muted-foreground">
                    {nameOf(a.harness)}
                    {a.own ? " · your own sign-in" : ""}
                  </p>
                </div>
                {a.limitedUntil && (
                  <span className="shrink-0 rounded-full bg-[color-mix(in_oklch,var(--tone-warn)_15%,transparent)] px-2 py-0.5 text-[11.5px] text-tone-warn">
                    Limited until {a.limitedUntil}
                  </span>
                )}
              </div>
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
              {nothing && (
                <p className="text-[12.5px] text-faint">
                  Nothing reported yet. It shows once its agent reports its plan.
                </p>
              )}
            </article>
          );
        })}
        {accounts.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No signed-in agents yet: sign in from Settings → Agents.
          </p>
        )}
      </section>
    </div>
  );
}
