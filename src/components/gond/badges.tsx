import type * as React from "react";
import { CheckIcon, InfoIcon, TriangleAlertIcon, XIcon, type LucideIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";

/** A badge's tone: what it says about the thing it marks. */
export type Tone = "ok" | "warn" | "bad" | "info" | "neutral";

const VARIANT: Record<Tone, "ok" | "warn" | "bad" | "info" | "secondary"> = {
  ok: "ok",
  warn: "warn",
  bad: "bad",
  info: "info",
  neutral: "secondary",
};
const ICON: Record<Tone, LucideIcon | null> = {
  ok: CheckIcon,
  warn: TriangleAlertIcon,
  bad: XIcon,
  info: InfoIcon,
  neutral: null,
};

/** Gond's badge: 1–4 words with a tone, an optional leading icon. */
export function ToneBadge({
  tone = "neutral",
  icon = true,
  className,
  children,
  ...props
}: { tone?: Tone; icon?: boolean } & React.ComponentProps<typeof Badge>) {
  const Icon = icon ? ICON[tone] : null;
  return (
    <Badge
      variant={VARIANT[tone]}
      data-entity="badge"
      data-tone={tone}
      className={className}
      {...props}
    >
      {Icon && <Icon data-icon="inline-start" />}
      {children}
    </Badge>
  );
}
