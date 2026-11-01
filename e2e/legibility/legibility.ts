// The legibility rule: every control an agent might use says
// what it is and what it does, in attributes an agent can read without guessing from pixels.
//
// Each interactive element needs:
// - an action: `data-command` (a registry command), `data-local` (a UI-only action, e.g.
//   "copy-id"), or `data-entity` + `data-id` (it selects that entity);
// - a role, native (button, a[href], input…) or explicit `role=`;
// - an accessible name (aria-label, aria-labelledby, its text, title, or an input's label);
// - when disabled, a reason: hover text (`title`, or a Hint's `data-hint` on it or around it)
//   and `aria-description`.

export const INTERACTIVE = [
  "button",
  "a[href]",
  "input:not([type=hidden])",
  "select",
  "textarea",
  "summary",
  '[role="button"]',
  '[role="link"]',
  '[role="menuitem"]',
  '[role="menuitemcheckbox"]',
  '[role="menuitemradio"]',
  '[role="tab"]',
  '[role="checkbox"]',
  '[role="radio"]',
  '[role="switch"]',
  '[role="option"]',
  '[role="combobox"]',
  '[role="slider"]',
  '[tabindex]:not([tabindex="-1"])',
  '[contenteditable="true"]',
  '[contenteditable=""]',
].join(",");

const NATIVE_ROLE = /^(button|a|input|select|textarea|summary)$/;

export type Problem = { key: string; missing: string[] };

function accessibleName(el: Element): string {
  const doc = el.ownerDocument;
  const label = el.getAttribute("aria-label")?.trim();
  if (label) return label;
  const by = el.getAttribute("aria-labelledby");
  if (by) {
    const text = by
      .split(/\s+/)
      .map((id) => doc.getElementById(id)?.textContent?.trim() ?? "")
      .join(" ")
      .trim();
    if (text) return text;
  }
  if (el.id) {
    const forLabel = doc.querySelector(`label[for="${CSS.escape(el.id)}"]`)?.textContent?.trim();
    if (forLabel) return forLabel;
  }
  const wrapping = el.closest("label")?.textContent?.trim();
  if (wrapping && el.tagName !== "LABEL") return wrapping;
  const text = el.textContent?.replace(/\s+/g, " ").trim();
  if (text) return text;
  const alt = el.querySelector("img[alt]")?.getAttribute("alt")?.trim();
  if (alt) return alt;
  return (el.getAttribute("title") ?? el.getAttribute("placeholder") ?? "").trim();
}

/** A stable name for the element, used in messages and the baseline. */
export function describe(el: Element): string {
  const tag = el.tagName.toLowerCase();
  const role = el.getAttribute("role");
  const name = accessibleName(el).slice(0, 60);
  const where = el.parentElement?.closest("[data-entity]")?.getAttribute("data-entity");
  return `${tag}${role ? `[role=${role}]` : ""} "${name}"${where ? ` in ${where}` : ""}`;
}

export function check(root: ParentNode): Problem[] {
  const problems: Problem[] = [];
  for (const el of root.querySelectorAll(INTERACTIVE)) {
    if (el.closest("[aria-hidden='true'], [inert]")) continue;
    const missing: string[] = [];
    const has = (a: string) => el.hasAttribute(a);
    if (!has("data-command") && !has("data-local") && !(has("data-entity") && has("data-id")))
      missing.push("data-command | data-local | data-entity+data-id");
    if (!NATIVE_ROLE.test(el.tagName.toLowerCase()) && !has("role")) missing.push("role");
    if (!accessibleName(el)) missing.push("accessible name");
    const disabled = has("disabled") || el.getAttribute("aria-disabled") === "true";
    // Hover text is a native title or a shadcn tooltip (components/hint.tsx), whose trigger, or
    // the box around a disabled control, carries data-hint.
    const hover = has("title") || !!el.closest("[data-hint]")?.getAttribute("data-hint");
    if (disabled && !(hover && has("aria-description")))
      missing.push("disabled reason (title + aria-description)");
    if (missing.length) problems.push({ key: describe(el), missing });
  }
  return problems;
}
