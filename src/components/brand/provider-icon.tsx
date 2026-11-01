import { useId, useSyncExternalStore } from "react";
import { cn } from "@/lib/utils";
import { brandOf, initials } from "./brands";

type Svgs = typeof import("./svgs");
let svgs: Svgs | null = null;
const waiting = new Set<() => void>();
let loading: Promise<void> | null = null;

function load() {
  loading ??= import("./svgs").then((m) => {
    svgs = m;
    waiting.forEach((fn) => fn());
  });
}

/** The brand icon chunk, shared by every ProviderIcon; null until it arrives (a frame or two). */
function useSvgs() {
  return useSyncExternalStore(
    (fn) => {
      waiting.add(fn);
      load();
      return () => void waiting.delete(fn);
    },
    () => svgs,
  );
}

type Props = {
  /** A provider, model family, agent or lobehub id: "anthropic", "claude", "claude-code", "codex". */
  id: string | undefined;
  /** For unknown ids: the name the initials come from (defaults to the id). */
  label?: string;
  /** "auto": the colour logo where it reads on our near-black, else mono in currentColor. */
  variant?: "auto" | "color" | "mono";
  /** Pixel size; defaults to 16 (size-4). */
  size?: number;
  className?: string;
};

/** A model's or agent's real brand mark (LobeHub icons, MIT). Unknown ids get their initials. */
export function ProviderIcon({ id, label, variant = "auto", size = 16, className }: Props) {
  const all = useSvgs();
  const brand = brandOf(id);
  // Each copy of a logo gets its own gradient and clip ids: with shared ones, a copy inside a
  // hidden pane or session takes them, and every visible copy draws blank (a white square).
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const style = { width: size, height: size };

  if (!brand) {
    return (
      <span
        data-slot="provider-icon"
        data-entity="provider"
        data-id={id}
        data-provider="unknown"
        role="img"
        aria-label={label ?? id ?? "Unknown"}
        style={{ ...style, fontSize: Math.max(7, size * 0.45) }}
        className={cn(
          "inline-grid shrink-0 place-items-center rounded-xs bg-muted font-semibold tracking-normal text-muted-foreground",
          className,
        )}
      >
        {initials(label ?? id ?? "?")}
      </span>
    );
  }

  const color = variant === "color" || (variant === "auto" && brand.color);
  const raw = all && ((color && all.COLOR[brand.icon]) || all.MONO[brand.icon]);
  const svg =
    raw &&
    raw
      .replace(/(\s)id="([^"]+)"/g, `$1id="$2-${uid}"`)
      .replace(/url\(#([^)]+)\)/g, `url(#$1-${uid})`)
      .replace(/href="#([^"]+)"/g, `href="#$1-${uid}"`);
  return (
    <span
      data-slot="provider-icon"
      data-entity="provider"
      data-id={id}
      data-provider={id}
      role="img"
      aria-label={brand.name}
      style={style}
      // overflow-hidden: some LobeHub SVGs draw a little past their box; the box is the icon.
      // size-full! beats a button's or toggle's own svg size, which clipped the mark.
      className={cn(
        "inline-flex shrink-0 overflow-hidden [&>svg]:size-full! [&>svg>title]:hidden",
        className,
      )}
      // Trusted, bundled SVG text from @lobehub/icons-static-svg.
      dangerouslySetInnerHTML={svg ? { __html: svg } : undefined}
    />
  );
}
