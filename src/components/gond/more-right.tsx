import { useEffect, useState, type RefObject } from "react";
import { ChevronRightIcon } from "lucide-react";

/**
 * The hint on a sideways scroller (Lumen): a 24px fade at its right edge and an "n more ›"
 * chip that scrolls to the first item out of view. `inset` keeps it clear of a tile pinned on the
 * right (the crew row's Add to crew). Renders nothing when everything fits.
 */
export function MoreRight({
  scroller,
  selector,
  inset = 0,
  surface = "background",
}: {
  scroller: RefObject<HTMLElement | null>;
  /** The scroller's items, as a CSS selector. */
  selector: string;
  inset?: number;
  /** What the fade blends into: the page, or a card the scroller sits in. */
  surface?: "background" | "card";
}) {
  const [hidden, setHidden] = useState<HTMLElement[]>([]);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => {
      const edge = el.getBoundingClientRect().right - inset;
      const items = [...el.querySelectorAll<HTMLElement>(selector)];
      // An item counts as hidden when less than half of it shows.
      setHidden(
        items.filter((i) => {
          const r = i.getBoundingClientRect();
          return r.left + r.width / 2 > edge;
        }),
      );
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    // Items can arrive after mount (the crew loads from the backend) without the row resizing.
    const mo = new MutationObserver(measure);
    mo.observe(el, { childList: true });
    el.addEventListener("scroll", measure, { passive: true });
    return () => {
      ro.disconnect();
      mo.disconnect();
      el.removeEventListener("scroll", measure);
    };
  }, [scroller, selector, inset]);

  if (hidden.length === 0) return null;
  // The chip sits inside the fade, on solid ground, so it never covers card text (Lumen).
  const solid = surface === "card" ? "from-card via-card" : "from-background via-background";
  return (
    <div
      className={`pointer-events-none absolute inset-y-0 flex items-center bg-linear-to-l ${solid} via-70% to-transparent pr-1 pl-8`}
      style={{ right: inset }}
    >
      <button
        type="button"
        data-local="scroll-more"
        onClick={() =>
          hidden[0]?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" })
        }
        className="pointer-events-auto flex items-center gap-0.5 rounded-full border border-edge bg-card px-2.5 py-1 text-xs whitespace-nowrap text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint"
      >
        {hidden.length} more <ChevronRightIcon className="size-3.5" />
      </button>
    </div>
  );
}
