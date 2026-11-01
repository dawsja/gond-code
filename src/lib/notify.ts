// Toasts are sonner's (shadcn's toast for Radix projects), loaded on first use so they don't
// weigh on startup: notify.success / notify.error / notify.info take sonner's arguments.
import type { ExternalToast } from "sonner";

type Kind = "success" | "error" | "info";
const show = (kind: Kind) => (title: string, data?: ExternalToast) =>
  void import("sonner").then(({ toast }) => toast[kind](title, data));

export const notify = { success: show("success"), error: show("error"), info: show("info") };
