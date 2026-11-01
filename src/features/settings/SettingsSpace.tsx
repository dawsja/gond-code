import {
  ArrowUpCircleIcon,
  ChartColumnIcon,
  LinkIcon,
  SlidersHorizontalIcon,
  WrenchIcon,
  type LucideIcon,
} from "lucide-react";
import type { RunCommand } from "@/features/deck/model";
import { cn } from "@/lib/utils";
import { MoreRight } from "@/components/gond/more-right";
import { Component, useRef, type ErrorInfo, type ReactNode } from "react";

export type SettingsSection = "general" | "usage" | "harnesses" | "connections" | "updates";

/** No Skills or Connectors: each agent brings its own skills and MCP servers. */
const SECTIONS: { id: SettingsSection; label: string; icon: LucideIcon }[] = [
  { id: "general", label: "General", icon: SlidersHorizontalIcon },
  { id: "usage", label: "Usage", icon: ChartColumnIcon },
  { id: "harnesses", label: "Agents", icon: WrenchIcon },
  { id: "connections", label: "GitHub", icon: LinkIcon },
  { id: "updates", label: "Updates", icon: ArrowUpCircleIcon },
];

/**
 * The Settings space: a section nav and the section. Sections go through
 * layout.focus { panel: "settings", entityId }. Below 1100 the nav
 * becomes a top tab row.
 */
export function SettingsSpace({
  section,
  onCommand,
  children,
}: {
  section: SettingsSection;
  onCommand: RunCommand;
  children: React.ReactNode;
}) {
  const nav = useRef<HTMLElement>(null);
  return (
    <div className="flex min-h-0 flex-1 max-[1099px]:flex-col">
      {/* Below 1100 the nav is a sideways tab row; the fade and "n more" show it scrolls. */}
      <div className="relative flex shrink-0 max-[1099px]:w-full">
        <nav
          ref={nav}
          data-overflow="scroll"
          aria-label="Settings"
          data-layout="settings-nav"
          className="flex w-50 shrink-0 flex-col gap-1 border-r p-4 max-[1099px]:w-full max-[1099px]:flex-row max-[1099px]:overflow-x-auto max-[1099px]:border-r-0 max-[1099px]:border-b min-[1920px]:w-55"
        >
          <p className="px-3 pb-2 text-xs text-muted-foreground max-[1099px]:hidden">Settings</p>
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              data-command="layout.focus"
              data-arg={id}
              aria-current={id === section ? "page" : undefined}
              onClick={() => onCommand("layout.focus", { panel: "settings", entityId: id })}
              className={cn(
                "flex shrink-0 items-center gap-3 rounded-xl px-3 py-2 text-sm whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-tint disabled:opacity-40",
                id === section
                  ? "bg-hover font-medium"
                  : "text-muted-foreground enabled:hover:text-foreground",
              )}
            >
              <Icon className="size-4" />
              {label}
            </button>
          ))}
        </nav>
        <MoreRight scroller={nav} selector='[data-command="layout.focus"]' />
      </div>
      {/* A container: each section lays out for the room it has, beside the sidebar. */}
      <div className="@container min-h-0 min-w-0 flex-1 overflow-y-auto p-4 min-[1920px]:p-6 min-[2560px]:p-8">
        <PageGuard key={section}>{children}</PageGuard>
      </div>
    </div>
  );
}

/**
 * One settings page that throws says so, in its own space, instead of taking the whole app
 * with it. Opening another section starts afresh.
 */
class PageGuard extends Component<{ children: ReactNode }, { error?: Error }> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("A settings page failed", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div role="alert" className="flex max-w-xl flex-col gap-3 rounded-2xl border border-edge p-5">
        <p className="font-medium">This page hit a problem and couldn&apos;t show.</p>
        <p data-selectable className="font-mono text-xs break-words text-muted-foreground">
          {error.message}
        </p>
        <button
          type="button"
          onClick={() => this.setState({ error: undefined })}
          className="w-fit rounded-full border border-edge px-3 py-1.5 text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint"
        >
          Try again
        </button>
      </div>
    );
  }
}
