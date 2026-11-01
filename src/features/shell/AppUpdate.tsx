import { ArrowUpCircleIcon } from "lucide-react";
import { useUpdate } from "@/features/settings/UpdateSettings";

/**
 * A newer Gond Code, above Settings: found by itself every few hours. It opens Settings → Updates,
 * where Restart to update installs it.
 */
export function AppUpdate({ onOpen }: { onOpen: () => void }) {
  const update = useUpdate();
  const version = update?.available?.version;
  if (!version) return null;
  const downloading = update.phase === "downloading" || update.phase === "installing";
  return (
    <button
      type="button"
      data-command="layout.focus"
      data-arg="updates"
      onClick={onOpen}
      className="flex items-center gap-2.5 rounded-xl border border-tint-edge bg-tint-wash px-2.5 py-2 text-left text-[13px] outline-none hover:bg-tint/25 focus-visible:ring-2 focus-visible:ring-tint"
    >
      <ArrowUpCircleIcon className="size-4 shrink-0 text-tint-ink" />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">Gond Code {version}</span>
        <span className="block truncate text-[11.5px] text-muted-foreground">
          {downloading ? `Downloading… ${update.percent ?? 0}%` : "Restart to update"}
        </span>
      </span>
    </button>
  );
}
