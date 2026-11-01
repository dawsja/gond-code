import { useEffect, useState } from "react";
import { FolderIcon, GitBranchIcon } from "lucide-react";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Hint } from "@/components/hint";
import { liveBackend, runCommand } from "@/lib/appState";
import { useAppName } from "@/lib/appName";
import { cn } from "@/lib/utils";
import { usePaneFolder } from "./paneFolder";

/** `folder.git`: a folder's name, and the repository and branch when it's in one. */
export type Place = { folder: string; repo?: string; branch?: string };

/** How often the branch is read again, so a checkout in a terminal shows. */
const EVERY_MS = 5000;

function usePlace(path: string | undefined, mock?: Place): Place | undefined {
  // The last reading, with the folder it's for: one for another folder isn't shown.
  const [read, setRead] = useState<{ path: string; place: Place }>();
  const live = liveBackend();
  useEffect(() => {
    if (!path || !live) return;
    let gone = false;
    const ask = () => {
      if (document.hidden) return;
      runCommand<Place>("folder.git", { path }).then(
        (place) => !gone && setRead({ path, place }),
        () => undefined,
      );
    };
    ask();
    const timer = window.setInterval(ask, EVERY_MS);
    return () => {
      gone = true;
      window.clearInterval(timer);
    };
  }, [path, live]);
  // The browser mock has no git: its folders are their projects.
  if (!live) return path ? mock : undefined;
  return read && read.path === path ? read.place : undefined;
}

/**
 * The strip under the panes: where the focused pane is (the repository and branch, or a plain
 * folder) on the left, and Gond Code's version on the right.
 */
export function StatusBar({
  paneId,
  path,
  mock,
  version,
}: {
  /** The focused pane: a shell's folder follows its prompt. */
  paneId?: string;
  /** The focused pane's own folder (its project's). */
  path?: string;
  /** The browser mock's answer for `path`. */
  mock?: Place;
  version?: string;
}) {
  const here = usePaneFolder(paneId) ?? path;
  const place = usePlace(here, mock);
  const name = useAppName();
  return (
    <footer
      data-entity="status-bar"
      className="flex h-[26px] shrink-0 items-center justify-between gap-4 border-t border-sidebar-border bg-sidebar px-3 text-[11.5px] text-muted-foreground"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        {place?.repo ? (
          <Hint text={here ?? place.repo}>
            <span className="flex min-w-0 items-center gap-1.5" data-slot="repo">
              <ProviderIcon id="gitbash" label="Git" size={13} />
              <span className="truncate font-medium text-sidebar-foreground">{place.repo}</span>
              {place.branch && (
                <>
                  <span className="text-faint">·</span>
                  <GitBranchIcon className="size-[13px] shrink-0" aria-hidden />
                  <span
                    className="truncate font-mono tracking-normal text-sidebar-foreground"
                    data-slot="branch"
                  >
                    {place.branch}
                  </span>
                </>
              )}
            </span>
          </Hint>
        ) : place ? (
          <Hint text={here ?? place.folder}>
            <span className="flex min-w-0 items-center gap-1.5" data-slot="folder">
              <FolderIcon className="size-[13px] shrink-0" aria-hidden />
              <span className="truncate font-medium text-sidebar-foreground">{place.folder}</span>
              <span className="text-faint">·</span>
              <span className="truncate">Not a git repository</span>
            </span>
          </Hint>
        ) : null}
      </div>
      {version && (
        <span
          className={cn(
            "shrink-0 whitespace-nowrap",
            name !== "Gond Code" && "font-medium text-tone-warn",
          )}
        >
          {name} {version}
        </span>
      )}
    </footer>
  );
}
