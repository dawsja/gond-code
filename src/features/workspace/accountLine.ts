import type { Account } from "@/lib/harnesses";

/** Who an account signs in as: its email and plan, or its masked key; never the key. */
export function accountLine(a?: Account) {
  if (!a) return "Its own sign-in";
  // An identity can carry an organisation id after the email ("me@x.com/9daae127-…"): the
  // email is what you'd recognise. A label that already says it ("me@x.com · Claude Max")
  // also names the plan.
  const who = a.identity?.split("/")[0];
  if (who && a.label.startsWith(who)) return a.label;
  return who ?? (a.plan ? `${a.label} · ${a.plan}` : a.label);
}
