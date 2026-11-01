// Projects from any folder: a plain folder needs no Git words.
import type { Project } from "./types";

const at = "2026-10-30T09:00:00Z";
/** ?demo's open projects: a repo on GitHub, a plain folder, and a local-only repo. */
export const MOCK_PROJECTS: Project[] = [
  {
    id: "prj_postplan",
    name: "postplan",
    path: "C:\\Users\\you\\code\\postplan",
    defaultBranch: "main",
    addedAt: at,
    lastOpenedAt: at,
    vcs: "git",
    remote: { name: "origin", url: "https://github.com/you/postplan", host: "github" },
  },
  {
    id: "prj_budget",
    name: "Family budget",
    path: "C:\\Users\\you\\Documents\\Family budget",
    defaultBranch: "main",
    addedAt: at,
    lastOpenedAt: at,
    vcs: "folder",
    remote: null,
    versions: 38,
  },
  {
    id: "prj_site",
    name: "site",
    path: "C:\\Users\\you\\code\\site",
    defaultBranch: "main",
    addedAt: at,
    lastOpenedAt: at,
    vcs: "git",
    remote: null,
  },
];
