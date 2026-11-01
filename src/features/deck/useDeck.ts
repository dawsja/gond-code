import { useCallback, useEffect, useState } from "react";
import { callOrRun, isRegistered, liveBackend, runCommand, useAppState } from "@/lib/appState";
import { useWorkspace } from "@/features/shell/useWorkspace";
import { notify } from "@/lib/notify";
import type { AgentInfo } from "@/lib/types";
import { DEMO } from "./live";
import { errorText } from "@/lib/types";

/** What the app shows: the sessions' panes ("chat", the name saved layouts keep), or Settings. */
type Panel = "chat" | "settings";

/** layout in the state tree: what the human is looking at. */
type DeckLayout = {
  focus: { panel: Panel; entityId?: string };
};

type TreeLayout = { focus?: { panel: string; entityId?: string } };

// Dev only (screenshots): ?settings=<section> opens a Settings section.
const URL_SETTINGS = import.meta.env.DEV
  ? new URLSearchParams(window.location.search).get("settings")
  : null;

/** Anything that isn't Settings is the panes: older saved layouts named other panels. */
const panelOf = (p?: string): Panel => (p === "settings" ? "settings" : "chat");

/**
 * The app's state and its command router: the folders, the installed agents and where you are.
 * Commands go to the registry; `?demo` swaps in sample data.
 */
export function useDeck() {
  const ws = useWorkspace();
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  // agent.detect again after an install, so the harness list shows it as installed.
  const refreshAgents = useCallback(
    () => void callOrRun("agent_detect", "agent.detect", {}).then(setAgents, () => undefined),
    [],
  );
  useEffect(refreshAgents, [refreshAgents]);
  const [localLayout, setLayout] = useState<DeckLayout>({
    focus:
      URL_SETTINGS !== null
        ? { panel: "settings", entityId: URL_SETTINGS || undefined }
        : { panel: "chat" },
  });
  // In the app, layout is the state tree's; ?demo and the browser mock keep it local.
  const tree = useAppState((v) => v.layout as TreeLayout | undefined);
  const liveLayout = !DEMO && liveBackend() && tree?.focus !== undefined;
  const layout: DeckLayout = liveLayout
    ? { focus: { panel: panelOf(tree.focus!.panel), entityId: tree.focus!.entityId } }
    : localLayout;

  const onCommand = useCallback(
    (name: string, input: Record<string, unknown>) => {
      if (!DEMO && name in LIVE) return LIVE[name](ws, input);
      if (!DEMO && liveBackend() && isRegistered(name) && !name.startsWith("layout."))
        return void runCommand(name, input).catch(fail);
      switch (name) {
        case "layout.focus":
          if (liveLayout) return void runCommand("layout.focus", input).catch(fail);
          setLayout({
            focus: {
              panel: panelOf(input.panel as string),
              entityId: input.entityId as string | undefined,
            },
          });
          break;
        default:
          if (!DEMO && liveBackend() && isRegistered(name))
            return void runCommand(name, input).catch(fail);
      }
    },
    [ws, liveLayout],
  );

  return { layout, onCommand, agents, refreshAgents, ws };
}

type Workspace = ReturnType<typeof useWorkspace>;
const fail = (e: unknown) => notify.error("That didn't work", { description: errorText(e) });
/** Commands the workspace keeps in step itself, by registry name ("Was"). */
const LIVE: Record<string, (ws: Workspace, input: Record<string, unknown>) => void> = {
  "project.add": (ws) => void ws.addProject(),
  "project.touch": (ws, i) => ws.openProject(i.projectId as string),
};
