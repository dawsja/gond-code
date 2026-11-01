import { useCallback, useEffect, useMemo, useState } from "react";
import { callOrRun, useAppState } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { pickFolder } from "@/lib/ipc";
import { isAppError, type Project } from "@/lib/types";

/**
 * The folders agents work in (projects). In the app they come from the state tree
 * and change through command_run; the browser mock keeps its own list.
 */
export function useWorkspace() {
  const [localProjects, setProjects] = useState<Project[]>([]);
  const treeProjects = useAppState((v) => v.projects as Record<string, Project> | undefined);
  const live = treeProjects !== undefined;
  const projects = useMemo(
    () =>
      live
        ? Object.values(treeProjects).sort((a, b) => b.lastOpenedAt.localeCompare(a.lastOpenedAt))
        : localProjects,
    [live, treeProjects, localProjects],
  );
  const [projectId, setProjectId] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    callOrRun("project_list", "project.list", {})
      .then((list) => {
        if (!cancelled) setProjects(list);
      })
      .finally(() => !cancelled && setLoaded(true))
      .catch(() => undefined);
    return () => void (cancelled = true);
  }, []);

  // A closed project (project.close) leaves the open list until you open it again.
  const open = useMemo(() => projects.filter((p) => p.open !== false), [projects]);
  const closed = useMemo(() => projects.filter((p) => p.open === false), [projects]);
  const project = open.find((p) => p.id === projectId) ?? open[0] ?? null;

  /** Makes a project current and moves it to the top of the recent list. */
  const openProject = useCallback((id: string) => {
    setProjectId(id);
    setProjects((ps) => {
      const p = ps.find((x) => x.id === id);
      return p
        ? [{ ...p, lastOpenedAt: new Date().toISOString() }, ...ps.filter((x) => x !== p)]
        : ps;
    });
    callOrRun("project_touch", "project.touch", { projectId: id }).catch(() => undefined);
  }, []);

  const addProject = useCallback(async () => {
    const path = await pickFolder();
    if (!path) return;
    try {
      const p = await callOrRun("project_add", "project.add", { path });
      setProjects((ps) => [p, ...ps.filter((x) => x.id !== p.id)]);
      setProjectId(p.id);
    } catch (e) {
      notify.error("Couldn't add the folder", {
        description: isAppError(e) ? e.message : String(e),
      });
    }
  }, []);

  return {
    loaded,
    projects: open,
    closedProjects: closed,
    project,
    openProject,
    addProject,
  };
}
