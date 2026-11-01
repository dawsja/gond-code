import { useState } from "react";
import { KeyRoundIcon, LogInIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { errorText } from "@/lib/types";
import type { RunCommand } from "@/features/deck/model";
import type { TestResult } from "@/lib/accounts";
import type { Harness } from "@/lib/harnesses";
import { notify } from "@/lib/notify";
import { AddAccount, type AccountKinds } from "./AddAccount";
import { SignIn } from "./SignIn";

/** Everything adding an account needs, built once by the Deck and shared by every place that
 *  offers it: Settings → Agents. */
export type AccountFlows = {
  harnesses: Harness[];
  kindsOf: (harness: string) => AccountKinds | undefined;
  onCommand: RunCommand;
  testDraft?: (input: Record<string, unknown>) => Promise<TestResult>;
  startSignIn?: (
    accountId: string,
    size: { cols: number; rows: number },
  ) => Promise<{ ptyId: string; jobId?: string }>;
  addSignIn?: (draft: Record<string, unknown>) => Promise<string>;
};

/**
 * "Use an API key" and "Sign in" on one harness's card: Add account preset to that harness
 * (key, base URL and options, Test connection), and the harness's own sign-in where
 * account.kinds offers it (account.add, then account.signIn). The same actions as buttons, or as
 * menu items for a narrow card; the dialogs render outside either, since a menu closes.
 */
export function useAgentAccountActions(harness: Harness, flows: AccountFlows) {
  const [adding, setAdding] = useState(false);
  const [signing, setSigning] = useState<{ id: string; label: string }>();
  const kinds = flows.kindsOf(harness.id)?.kinds ?? [];
  const canKey = kinds.some((k) => k === "apiKey" || k === "gateway");
  const canSignIn = kinds.includes("oauth");
  const signInReady = !!flows.addSignIn && !!flows.startSignIn;
  // A key or a sign-in can come before the install ("Not set up yet" offers all three);
  // the harness's own sign-in needs the harness, so it waits for the install.
  // Sign in shows once it can run (never a stub).
  const signInHere = canSignIn && harness.installed && signInReady;
  const any = canKey || signInHere;

  const signIn = () => {
    const label = `${harness.label} sign-in`;
    flows.addSignIn?.({ harness: harness.id, kind: "oauth", label }).then(
      (id) => setSigning({ id, label }),
      (e) => notify.error(errorText(e)),
    );
  };

  const buttons = (
    <>
      {canKey && (
        <Button
          variant="ghost"
          size="sm"
          className="rounded-full"
          data-local="account-add"
          data-arg={harness.id}
          aria-haspopup="dialog"
          onClick={() => setAdding(true)}
        >
          <KeyRoundIcon data-icon="inline-start" /> Use an API key
        </Button>
      )}
      {signInHere && (
        <Button
          variant="ghost"
          size="sm"
          className="rounded-full"
          data-command="account.signIn"
          data-arg={harness.id}
          onClick={signIn}
        >
          <LogInIcon data-icon="inline-start" /> Sign in
        </Button>
      )}
    </>
  );
  const items = (
    <>
      {canKey && (
        <DropdownMenuItem
          data-local="account-add"
          data-arg={harness.id}
          onSelect={() => setAdding(true)}
        >
          <KeyRoundIcon /> Use an API key
        </DropdownMenuItem>
      )}
      {signInHere && (
        <DropdownMenuItem data-command="account.signIn" data-arg={harness.id} onSelect={signIn}>
          <LogInIcon /> Sign in
        </DropdownMenuItem>
      )}
    </>
  );
  const dialogs = (
    <>
      {adding && (
        <AddAccount
          harnesses={flows.harnesses}
          kindsOf={flows.kindsOf}
          onCommand={flows.onCommand}
          testDraft={flows.testDraft}
          initialHarness={harness.id}
          initialKind="apiKey"
          onClose={() => setAdding(false)}
        />
      )}
      {signing && flows.startSignIn && (
        <SignIn
          label={signing.label}
          harness={harness.label}
          start={(size) => flows.startSignIn!(signing.id, size)}
          onClose={() => setSigning(undefined)}
        />
      )}
    </>
  );
  return { any, buttons, items, dialogs };
}

export function AgentAccountButtons({ harness, flows }: { harness: Harness; flows: AccountFlows }) {
  const { any, buttons, dialogs } = useAgentAccountActions(harness, flows);
  if (!any) return dialogs;
  return (
    <div className="flex flex-wrap gap-2">
      {buttons}
      {dialogs}
    </div>
  );
}
