import type { AccountKinds } from "@/features/accounts/AddAccount";
import { useEffect, useMemo, useState } from "react";
import { liveBackend, runCommand, useAppState } from "@/lib/appState";
import { fromModelList, type Account, type ListedModel, type ModelInfo } from "@/lib/harnesses";
import { limitedUntil, usageKey, type PlanUsage } from "@/lib/accounts";

/**
 * Each installed harness's models from `model.list { harness }`, for Add to crew's model
 * and thinking pickers. Empty on the browser mock; ?demo brings its own.
 */
export function useModels(harnessIds: string[]): Record<string, ModelInfo[]> {
  const [models, setModels] = useState<Record<string, ModelInfo[]>>({});
  const key = [...harnessIds].sort().join(",");
  useEffect(() => {
    if (!liveBackend() || !key) return;
    let live = true;
    for (const harness of key.split(",")) {
      runCommand<ListedModel[]>("model.list", { harness }).then(
        (list) => live && setModels((m) => ({ ...m, [harness]: fromModelList(list) })),
        () => undefined, // a harness without a driver yet: its picker says so
      );
    }
    return () => {
      live = false;
    };
  }, [key]);
  return models;
}

/** An account as `/accounts/{id}` holds it: never a key. */
type TreeAccount = {
  id: string;
  harness: string;
  provider: string;
  label: string;
  kind: Account["kind"];
  endpoint?: string | null;
  /** Who it signs in as: an email and plan, or a masked key. */
  identity?: string | null;
  isDefault?: boolean;
  own?: boolean;
};

/** The accounts from the state tree, as the pickers and Settings → Accounts read them. Usage
 *  shows only what a harness reports, so it starts unknown. */
export function useAccounts(): Account[] {
  const tree = useAppState((t) => t.accounts as Record<string, TreeAccount> | undefined);
  const usage = useAppState((t) => t.usage as Record<string, PlanUsage> | undefined);
  return useMemo(
    () =>
      Object.values(tree ?? {}).map((a): Account => {
        const u = usage?.[usageKey(a)];
        return {
          id: a.id,
          harness: a.harness,
          provider: a.provider,
          label: a.label,
          kind: a.kind,
          identity: a.identity ?? undefined,
          isDefault: a.isDefault,
          own: a.own,
          // Plan windows show from /usage directly.
          usage: { kind: "unknown" },
          limitedUntil: limitedUntil(u),
        };
      }),
    [tree, usage],
  );
}

/** `model.list { harness, accountId }`: an endpoint account's own models, with the
 *  catalog's thinking levels where the id is known and none where it isn't. */
export const accountModels = (harness: string, accountId: string) =>
  runCommand<ListedModel[]>("model.list", { harness, accountId }).then(fromModelList);

/** `account.kinds { harness }` per installed harness, for Add account. */
export function useKinds(harnessIds: string[]): Record<string, AccountKinds> {
  const [kinds, setKinds] = useState<Record<string, AccountKinds>>({});
  const key = [...harnessIds].sort().join(",");
  useEffect(() => {
    if (!liveBackend() || !key) return;
    let live = true;
    for (const harness of key.split(","))
      runCommand<AccountKinds>("account.kinds", { harness }).then(
        (k) => live && setKinds((m) => ({ ...m, [harness]: k })),
        () => undefined,
      );
    return () => {
      live = false;
    };
  }, [key]);
  return kinds;
}
