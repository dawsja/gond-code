import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AccountFlows } from "@/features/accounts/AgentAccountButtons";
import { useHarnessInstalls } from "@/features/harness/useHarnessInstalls";
import { useSetup } from "@/features/setup/useSetup";
import { callOrRun, isRegistered, liveBackend, runCommand, useAppState } from "@/lib/appState";
import type { TestResult } from "@/lib/accounts";
import type { DeviceCode, GithubStatus } from "@/lib/github";
import { accountModels, useAccounts, useKinds, useModels } from "@/features/harness/useModels";
import { TooltipProvider } from "@/components/ui/tooltip";
import { GeneralSettings } from "@/features/settings/GeneralSettings";
import { UpdateSettings } from "@/features/settings/UpdateSettings";
import { SettingsSpace, type SettingsSection } from "@/features/settings/SettingsSpace";
import { HarnessManager } from "@/features/harness/HarnessManager";
import { useHarnessChecks } from "@/features/harness/useHarnessChecks";
import { applyTint } from "@/lib/theme";
import type { Project, Settings } from "@/lib/types";
import { MOCK_PROJECTS } from "@/lib/projects";
import { UsageSettings } from "@/features/usage/UsageSettings";
import { DEMO } from "./live";
import type { SpawnData } from "./model";
import {
  MOCK_ACCOUNTS,
  MOCK_HARNESSES,
  MOCK_KINDS,
  mockAccountTest,
  MOCK_MODELS,
} from "@/features/harness/mock";
import { ConnectionsSettings } from "@/features/connections/ConnectionsSettings";
import { MOCK_GITHUB, mockDeviceCode } from "@/features/connections/mock";
import { SetupToast } from "@/features/setup/SetupToast";
import type { JobState } from "@/lib/setup";
import { MOCK_JOB } from "@/features/setup/mock";
import { useDeck } from "./useDeck";
import { Dictation, dictate } from "@/features/workspace/Dictation";
import { KeyHelp, showKeys } from "@/features/workspace/KeyHelp";
import { AutoShell } from "@/features/workspace/AutoShell";
import { NewAgent } from "@/features/workspace/NewAgent";
import { SessionsSidebar } from "@/features/workspace/SessionsSidebar";
import { useStatuses } from "@/features/workspace/status";
import { useSessions } from "@/features/workspace/useSessions";
import { find, panes } from "@/features/workspace/layout";
import { StatusBar } from "@/features/workspace/StatusBar";
import { Workspace } from "@/features/workspace/Workspace";

const URL_TOAST = import.meta.env.DEV && new URLSearchParams(window.location.search).has("toast");

// sonner loads after first paint; toasts come through lib/notify.
const Toaster = lazy(() => import("@/components/ui/sonner").then((m) => ({ default: m.Toaster })));

/** Settings (tint and the rest) from the backend. */
function useSettings() {
  const [settings, setSettings] = useState<Settings>({ tint: "blue" });
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    callOrRun("settings_get", "settings.get", {}).then(setSettings, () => undefined);
    callOrRun("app_version", "app.version", {}).then(setVersion, () => undefined);
  }, []);
  useEffect(() => applyTint(settings.tint), [settings.tint]);
  const change = (patch: Partial<Settings>) => {
    setSettings((s) => ({ ...s, ...patch }));
    callOrRun("settings_set", "settings.set", { patch }).then(setSettings);
  };
  return { settings, version, change };
}

/**
 * Gond Code: a terminal workspace for coding agents. The sidebar lists your
 * folders and the agents running in each; each agent runs its own CLI in a pane you split right
 * or down, signed in with your subscription or an API key.
 */
/** The account providers each built-in agent signs in with. */
const PROVIDERS: Record<string, string[]> = {
  claude: ["anthropic"],
  codex: ["openai"],
  agy: ["google"],
  grok: ["xai"],
  copilot: ["github"],
  cursor: ["cursor"],
};

export function Deck() {
  const { layout, onCommand, agents, refreshAgents, ws } = useDeck();
  // Installing an agent: follow its job, then re-detect the agents.
  const harnessInstalls = useHarnessInstalls(refreshAgents);
  const { settings, version, change } = useSettings();
  // A project's kind lives in the tree; the IPC list only has name and path, so read both.
  const treeProjects = useAppState(
    (v) => v.projects as Record<string, Pick<Project, "vcs">> | undefined,
  );
  const withTree = <P extends Project>(p: P): P => ({ ...p, ...treeProjects?.[p.id] });
  const liveProjects = ws.projects.map(withTree);
  const panel = layout.focus.panel;
  const section = ((panel === "settings" ? layout.focus.entityId : undefined) ??
    "general") as SettingsSection;
  const installedIds = agents.filter((a) => a.installed).map((a) => a.id);
  const installs = useAppState((t) => t.installs as Record<string, JobState> | undefined);
  const liveJobs = useMemo(() => Object.values(installs ?? {}), [installs]);
  // GitHub through gh (Settings → Plugins): signed-in accounts live at /github.
  const github = useAppState((t) => t.github as GithubStatus | undefined);
  const ghLive = !DEMO && liveBackend();
  const liveModels = useModels(installedIds);
  const liveAccounts = useAccounts();
  const liveKinds = useKinds(installedIds);
  // Newer agent versions, looked for on their own; you install them from Settings → Harnesses.
  const updates = useHarnessChecks(!DEMO && liveBackend() && isRegistered("harness.check"));
  const spawnData: SpawnData = {
    ...(DEMO
      ? { harnesses: MOCK_HARNESSES, models: MOCK_MODELS, accounts: MOCK_ACCOUNTS }
      : {
          // Live: the detected agents as harnesses, their models from model.list, and the
          // accounts in the tree (an endpoint account's own models come from model.list too).
          harnesses: agents.map((a) => ({
            id: a.id,
            label: a.name,
            brand: a.provider ?? a.id,
            installed: a.installed,
            version: a.version,
            latest: updates.checks[a.id]?.updateAvailable ? updates.checks[a.id].latest : a.version,
            notesUrl: updates.checks[a.id]?.changelog || undefined,
            // Whose accounts it signs in with (Accounts' "Works in …").
            providers: PROVIDERS[a.id] ?? [],
          })),
          models: liveModels,
          accounts: liveAccounts,
          accountModels,
        }),
  };
  // Adding an account, wherever it's offered (New agent, Settings → Agents).
  const signInLive = !DEMO && liveBackend() && isRegistered("account.signIn");
  const testLive = !DEMO && liveBackend() && isRegistered("account.test");
  const accountFlows: AccountFlows = {
    harnesses: spawnData.harnesses,
    kindsOf: (h) => (DEMO ? MOCK_KINDS[h] : liveKinds[h]),
    onCommand,
    testDraft: DEMO
      ? mockAccountTest
      : testLive
        ? (input) => runCommand<TestResult>("account.test", input)
        : undefined,
    startSignIn: signInLive
      ? (accountId, size) =>
          runCommand<{ ptyId: string; jobId?: string }>("account.signIn", { accountId, ...size })
      : undefined,
    addSignIn: signInLive
      ? (draft) => runCommand<{ id: string }>("account.add", draft).then((a) => a.id)
      : undefined,
  };
  // The agents' own sign-ins, looked for again (not only at startup): when Accounts opens, and
  // when you come back to the window, at most every two minutes.
  const discoverLive = !DEMO && liveBackend() && isRegistered("account.discover");
  const lastDiscover = useRef(0);
  const discover = useCallback(() => {
    if (!discoverLive || Date.now() - lastDiscover.current < 120_000) return;
    lastDiscover.current = Date.now();
    runCommand("account.discover", {}).catch(() => undefined);
  }, [discoverLive]);
  useEffect(() => {
    if (panel === "settings" && section === "harnesses") discover();
  }, [panel, section, discover]);
  // Settings → Plugins reads gh again when it opens (a sign-in made in a terminal shows up).
  useEffect(() => {
    if (!ghLive || panel !== "settings" || section !== "connections") return;
    if (!isRegistered("github.status")) return;
    runCommand("github.status", {}).catch(() => undefined);
  }, [ghLive, panel, section]);
  // Plan limits (% used, when they reset) read from each agent's own usage source when
  // Accounts or Usage opens, at most once a minute (usage.refresh).
  const lastLimits = useRef(0);
  useEffect(() => {
    const live = !DEMO && liveBackend() && isRegistered("usage.refresh");
    if (!live || panel !== "settings" || section !== "usage") return;
    if (Date.now() - lastLimits.current < 60_000) return;
    lastLimits.current = Date.now();
    runCommand("usage.refresh", {}).catch(() => undefined);
  }, [panel, section]);
  useEffect(() => {
    window.addEventListener("focus", discover);
    return () => window.removeEventListener("focus", discover);
  }, [discover]);
  const statuses = useStatuses();
  const sp = useSessions({
    agents,
    models: spawnData.models,
    accounts: spawnData.accounts,
    projects: DEMO ? MOCK_PROJECTS : liveProjects,
  });
  // Each agent's open panes, in every session: Settings → Agents won't uninstall it under them.
  const openPanes: Record<string, number> = {};
  for (const g of sp.sessions)
    for (const p of panes(g.layout.root))
      if (p.agentId) openPanes[p.agentId] = (openPanes[p.agentId] ?? 0) + 1;
  const toWorkspace = useCallback(() => {
    if (layout.focus.panel === "settings") onCommand("layout.focus", { panel: "chat" });
  }, [layout.focus.panel, onCommand]);
  // Ctrl+T, Ctrl+Shift+Space and Ctrl+Shift+P wherever focus is; a pane's terminal handles its
  // own, so a key pressed there doesn't happen twice.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || (e.target as Element | null)?.closest?.(".xterm")) return;
      if (e.shiftKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        showKeys();
      } else if (!e.shiftKey && e.key.toLowerCase() === "t") {
        e.preventDefault();
        sp.openPicker({});
      } else if (e.shiftKey && e.code === "Space") {
        e.preventDefault();
        dictate();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [sp]);
  const setup = useSetup();
  // The set-up job (Settings → Plugins installs Git and gh) carries on in a toast; ?toast shows one
  // on the mock build.
  const [job, setJob] = useState<JobState | undefined>(() => (URL_TOAST ? MOCK_JOB : undefined));
  // Hide hides this job's toast; the next install shows its own.
  const [hiddenToast, setHiddenToast] = useState<string>();
  // Live, the job is /installs/{jobId}; a finished one stays gone once its toast has gone.
  const [goneJob, setGoneJob] = useState<string>();
  const liveJob = setup.live && !DEMO ? setup.job : undefined;
  const shownJob = setup.live && !DEMO ? (liveJob?.id === goneJob ? undefined : liveJob) : job;
  const clearJob = useCallback(() => {
    setJob(undefined);
    setGoneJob(liveJob?.id);
  }, [liveJob?.id]);

  const projects = DEMO ? MOCK_PROJECTS : liveProjects;
  // The status bar follows the focused pane's folder.
  const focusedPane = sp.current?.layout.focus
    ? find(sp.current.layout.root, sp.current.layout.focus)
    : undefined;
  const focusedProject = projects.find((p) => p.id === focusedPane?.projectId);
  return (
    <TooltipProvider delayDuration={300}>
      <div
        className="flex h-screen overflow-hidden bg-background"
        data-entity="app"
        data-focus={panel}
      >
        <SessionsSidebar
          sessions={sp.sessions}
          current={sp.current?.id}
          focusedPane={sp.current?.layout.focus}
          projects={projects}
          settings={panel === "settings"}
          agents={agents}
          statuses={statuses}
          renaming={sp.renaming}
          onNewAgent={() => {
            toWorkspace();
            sp.openPicker({});
          }}
          onSelectSession={(id) => {
            toWorkspace();
            sp.selectSession(id);
          }}
          onAddSession={() => {
            toWorkspace();
            sp.addSession();
          }}
          onRenameSession={sp.actions.rename}
          onRenamedSession={sp.renameSession}
          onRemoveSession={sp.removeSession}
          onSelectPane={(id) => {
            toWorkspace();
            sp.actions.focus(id);
          }}
          onSettings={() => onCommand("layout.focus", { panel: "settings", entityId: "general" })}
          onUpdates={() => onCommand("layout.focus", { panel: "settings", entityId: "updates" })}
        />
        <main className="relative flex min-w-0 flex-1 flex-col">
          {panel === "settings" && (
            <SettingsSpace section={section} onCommand={onCommand}>
              {section === "connections" ? (
                <ConnectionsSettings
                  status={DEMO ? MOCK_GITHUB : github}
                  startWebLogin={
                    DEMO
                      ? mockDeviceCode
                      : ghLive && isRegistered("github.login.web")
                        ? () => runCommand<DeviceCode>("github.login.web", {})
                        : undefined
                  }
                  onToken={
                    DEMO
                      ? (token) => onCommand("github.login.token", { token })
                      : ghLive && isRegistered("github.login.token")
                        ? (token) => runCommand("github.login.token", { token })
                        : undefined
                  }
                  onCommand={onCommand}
                />
              ) : section === "updates" ? (
                <UpdateSettings version={version} />
              ) : section === "usage" ? (
                <UsageSettings accounts={spawnData.accounts} harnesses={spawnData.harnesses} />
              ) : section === "harnesses" ? (
                <HarnessManager
                  running={openPanes}
                  harnesses={spawnData.harnesses}
                  jobs={liveJobs}
                  accountFlows={accountFlows}
                  installs={!DEMO && liveBackend() ? harnessInstalls : undefined}
                  checkUpdates={updates.check}
                  onCommand={onCommand}
                />
              ) : (
                <GeneralSettings
                  settings={settings}
                  onChange={change}
                  version={version}
                  agents={spawnData.harnesses.filter((h) => h.installed)}
                />
              )}
            </SettingsSpace>
          )}
          {/* Every session's panes stay mounted (hidden when you're elsewhere), so the agents in
              them keep running while you look at another layout or at Settings. */}
          {sp.loaded &&
            sp.sessions.map((g) => (
              <Workspace
                key={g.id}
                layout={g.layout}
                visible={panel !== "settings" && g.id === sp.current?.id}
                agents={agents}
                statuses={statuses}
                restarts={sp.restarts}
                start={sp.opener}
                actions={sp.actions}
                renaming={sp.renaming}
                onRenamed={sp.renamed}
                empty={<AutoShell onEmpty={() => sp.startShell(g.id)} />}
              />
            ))}
          <Dictation target={() => sp.current?.layout.focus} />
          <KeyHelp />
          <StatusBar
            paneId={focusedPane?.id}
            path={focusedProject?.path}
            mock={
              focusedProject && {
                folder: focusedProject.name,
                ...(focusedProject.vcs !== "folder" && {
                  repo: focusedProject.name,
                  branch: focusedProject.defaultBranch,
                }),
              }
            }
            version={version}
          />
        </main>
        <NewAgent
          open={sp.picker.open}
          onOpenChange={sp.setPickerOpen}
          agents={agents}
          accounts={spawnData.accounts}
          models={spawnData.models}
          projects={projects}
          initial={sp.picker.initial}
          hasPanes={() => !!sp.current?.layout.root}
          onStart={(s) => void sp.start(s, sp.picker.at)}
        />
        {shownJob && shownJob.id !== hiddenToast && (
          <SetupToast job={shownJob} onHide={() => setHiddenToast(shownJob.id)} onGone={clearJob} />
        )}
        <Suspense>
          <Toaster position="bottom-right" />
        </Suspense>
      </div>
    </TooltipProvider>
  );
}
