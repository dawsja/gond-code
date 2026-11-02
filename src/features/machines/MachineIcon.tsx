import { MonitorIcon, ServerIcon } from "lucide-react";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { shellBrand } from "@/components/brand/brands";
import { cn } from "@/lib/utils";

/** A machine's icon: this PC, a WSL distribution's logo, or a server for an SSH machine. */
export function MachineIcon({ id, size = 15 }: { id?: string; size?: number }) {
  if (id?.startsWith("wsl:")) return <ProviderIcon id={shellBrand(id)} size={size} />;
  const Icon = id?.startsWith("ssh:") ? ServerIcon : MonitorIcon;
  return (
    <Icon
      aria-hidden
      className={cn("shrink-0 text-muted-foreground")}
      style={{ width: size, height: size }}
    />
  );
}

/** A connection's dot: up, on its way, out of reach. */
export function LinkDot({ state }: { state?: string }) {
  if (!state) return null;
  return (
    <span
      aria-hidden
      className={cn(
        "size-[7px] shrink-0 rounded-full",
        state === "up" && "bg-tone-ok",
        state === "connecting" && "animate-pulse bg-tone-warn",
        state === "down" && "bg-tone-bad",
        state === "off" && "bg-faint",
      )}
    />
  );
}
