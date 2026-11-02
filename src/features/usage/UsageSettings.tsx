import { ProviderIcon } from "@/components/brand/provider-icon";
import { useAppState } from "@/lib/appState";
import type { Harness } from "@/lib/harnesses";
import { limitedUntil, PLAN_AGENTS, usageKey, type PlanUsage } from "@/lib/usage";
import { DEMO } from "@/features/deck/live";
import { MOCK_USAGE } from "@/features/harness/mock";
import { Hint } from "@/components/hint";
import { PlanWindows } from "./PlanWindows";

/**
 * Settings → Usage: each installed agent's plan, as its own sign-in reports it: each window
 * (what's left, when it resets). Gond can't see what an agent spends in its own CLI, so it shows
 * no costs of its own.
 */
export function UsageSettings({ harnesses }: { harnesses: Harness[] }) {
  const live = useAppState((t) => t.usage as Record<string, PlanUsage> | undefined);
  const usage = DEMO ? MOCK_USAGE : live;
  // The agents whose plan Gond reads (their file's `usage`), in the Agents page's order; the
  // demo's sample agents by the list.
  const agents = harnesses.filter(
    (h) => h.installed && (h.plan ?? (DEMO && PLAN_AGENTS.includes(h.id))),
  );
  return (
    <div data-slot="usage" className="flex max-w-[880px] flex-col gap-5">
      <header>
        <h1 className="text-xl font-semibold">Usage</h1>
        <p className="text-sm text-muted-foreground">
          What each agent has left of its plan, as the agent reports it.
        </p>
      </header>

      <section aria-label="Agents" className="grid gap-3 @min-[46rem]:grid-cols-2">
        {agents.map((h) => {
          const u = usage?.[usageKey(h.id)];
          const windows = u?.windows ?? [];
          const limited = limitedUntil(u);
          return (
            <article
              key={h.id}
              data-entity="agent-usage"
              data-id={h.id}
              aria-label={h.label}
              className="flex flex-col gap-3 rounded-2xl border border-edge bg-card p-4"
            >
              <div className="flex items-center gap-2.5">
                <ProviderIcon id={h.brand} size={18} />
                <div className="min-w-0 flex-1">
                  <Hint text={h.label}>
                    <p className="truncate text-sm font-medium">{h.label}</p>
                  </Hint>
                </div>
                {limited && (
                  <span className="shrink-0 rounded-full bg-[color-mix(in_oklch,var(--tone-warn)_15%,transparent)] px-2 py-0.5 text-[11.5px] text-tone-warn">
                    Limited until {limited}
                  </span>
                )}
              </div>
              {windows.length > 0 && <PlanWindows windows={windows} />}
              {windows.length === 0 && (
                <p className="text-[12.5px] text-faint">
                  Nothing reported yet. It shows once you're signed in to {h.label} on a plan with
                  limits.
                </p>
              )}
            </article>
          );
        })}
        {agents.length === 0 && (
          <p className="text-sm text-muted-foreground">No agents installed that report a plan.</p>
        )}
      </section>
    </div>
  );
}
