import { useAppState } from "@/lib/appState";

/** "Gond Code", or "Gond Code Dev" for a dev build (`/update.channel`; src-tauri/src/channel.rs). */
export const useAppName = () =>
  useAppState((t) => (t.update as { channel?: string } | undefined)?.channel) === "dev"
    ? "Gond Code Dev"
    : "Gond Code";
