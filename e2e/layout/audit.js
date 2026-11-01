// Layout audit. Runs inside the app's page (real WebView2 on Windows, via
// scripts/layout-check.ts) and returns every place where text escapes its box or the layout is
// crowded. Plain JS, no imports: it's sent as a string over the DevTools protocol.
//
// Rules:
//   overflow     content wider than its box (scrollWidth > clientWidth + 1) and not a scroller
//   clipped      text cut off by overflow:hidden with no ellipsis
//   no-tooltip   text truncated with an ellipsis but no title / aria-description / hint to show it all
//   page-x       the window scrolls sideways
//   nested-scroll  a scroller inside a scroller on the same axis
//   word-per-line  text wrapped about one word per line (Lumen's heuristic: lines > words / 2)
//   narrow-text  a wrapping text box under 12ch
//   min-width    an element marked data-layout="<kind>" (or data-min-width) narrower than its minimum
//   control-clipped  a control (button, link, field, [data-command]) whose border box is cut by an
//                overflow:hidden/clip ancestor
//   header-centred  an app-header control whose vertical centre is more than 1px off the header's
//                centre line
//   concentric   a painted box inset evenly in a rounded painted parent whose corners are tighter
//                than the parent's radius minus the inset (a square highlight in a pill track)
//   logo-count   Gond Code's mark (the triangle, never the cog): none on the rail; exactly
//                one in the app header; none on pages without it (first run, setup: Windows' own
//                title bar shows the app icon), except one in a new chat's greeting
//                ([data-entity="greeting"]).
//   label-wrap   a menu item, button or badge whose own label wraps onto a second line (a
//                "Close / thread" menu item); stacked labels in separate elements are fine
// Opt-outs: data-overflow="scroll" on a deliberate scroller (and its contents);
// data-corner="square" on a box that's square on purpose (concentric only).
(() => {
  //  minimums, by data-layout kind.
  const MIN = {
    lane: 220,
    "task-card": 204,
    "crew-card": 260,
    "add-tile": 56,
    "inbox-column": 320,
    "loop-column": 280,
    talk: 340,
    "work-area": 480,
    "inbox-list": 320,
    "inbox-detail": 480,
    "settings-nav": 180,
    "command-sheet": 720,
    "receipt-sheet": 760,
  };
  const out = [];
  const seen = new Set();
  const css = (el) => getComputedStyle(el);
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return false;
    const s = css(el);
    return s.visibility !== "hidden" && s.display !== "none";
  };
  const path = (el) => {
    const parts = [];
    for (let e = el; e && e !== document.body && parts.length < 4; e = e.parentElement) {
      let p = e.tagName.toLowerCase();
      const tag = e.getAttribute("data-layout") || e.getAttribute("data-entity");
      if (tag) p += `[${tag}${e.getAttribute("data-id") ? "=" + e.getAttribute("data-id") : ""}]`;
      parts.unshift(p);
    }
    return parts.join(" > ");
  };
  const text = (el) => (el.innerText || el.textContent || "").replace(/\s+/g, " ").trim();
  const report = (rule, el, detail) => {
    const key = rule + "|" + path(el) + "|" + text(el).slice(0, 40);
    if (seen.has(key)) return;
    seen.add(key);
    out.push({ rule, where: path(el), text: text(el).slice(0, 80), detail });
  };
  const optedOut = (el) => !!el.closest('[data-overflow="scroll"]');
  const scrolls = (s, axis) => /(auto|scroll)/.test(axis === "x" ? s.overflowX : s.overflowY);
  const hasTooltip = (el) => {
    for (let e = el, i = 0; e && i < 4; e = e.parentElement, i++)
      // data-hint: a shadcn tooltip (components/hint.tsx) holds the full text.
      if (
        e.title ||
        e.getAttribute("aria-description") ||
        e.getAttribute("data-tooltip") ||
        e.getAttribute("data-hint")
      )
        return true;
    return false;
  };
  const ownText = (el) =>
    [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim().length > 0);
  const chWidth = (el) => {
    const c = chWidth.c || (chWidth.c = document.createElement("canvas").getContext("2d"));
    c.font = css(el).font;
    return c.measureText("0").width || 8;
  };
  // Lines of the element's own text (icons and child boxes beside it don't count).
  const lineCount = (el) => {
    const tops = new Set();
    for (const n of el.childNodes) {
      if (n.nodeType !== 3 || !n.textContent.trim()) continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      for (const r of range.getClientRects()) if (r.width > 0) tops.add(Math.round(r.top));
    }
    return tops.size;
  };
  // Screen-reader-only text (Tailwind sr-only: 1px, clipped) is meant to be invisible.
  const srOnly = (el) => {
    const s = css(el);
    return (
      (s.position === "absolute" && el.clientWidth <= 1) ||
      (s.clip && s.clip !== "auto") ||
      s.clipPath.startsWith("inset(50%")
    );
  };

  // Page scrolls sideways.
  const doc = document.documentElement;
  if (doc.scrollWidth > window.innerWidth + 1)
    out.push({
      rule: "page-x",
      where: "html",
      text: "",
      detail: `page ${doc.scrollWidth}px wide in a ${window.innerWidth}px window`,
    });

  // Nested corners are concentric: inner radius = outer − inset.
  const painted = (s) =>
    (s.backgroundColor && !/rgba\(.*,\s*0\)$|transparent/.test(s.backgroundColor)) ||
    parseFloat(s.borderTopWidth) > 0;
  const cornerOf = (el, r) =>
    Math.min(parseFloat(css(el).borderTopLeftRadius) || 0, r.height / 2, r.width / 2);
  const all = [...document.body.querySelectorAll("*")].filter(
    (el) =>
      !["SCRIPT", "STYLE", "svg", "path", "CANVAS", "BR"].includes(el.tagName) &&
      !el.closest("svg, .xterm"),
  );
  const overflowing = new Set();
  for (const el of all) {
    if (!visible(el) || optedOut(el) || srOnly(el)) continue;
    const s = css(el);
    const inline = s.display === "inline" || s.display === "contents";

    // Content wider than its box.
    if (!inline && el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0 && !scrolls(s, "x")) {
      if (text(el)) overflowing.add(el);
    }

    // Concentric corners: only a box inset evenly top and bottom (a highlight in a track).
    const parent = el.parentElement;
    if (
      parent &&
      parent !== document.body &&
      el.getAttribute("data-corner") !== "square" &&
      painted(s) &&
      painted(css(parent))
    ) {
      const r = el.getBoundingClientRect();
      const pr = parent.getBoundingClientRect();
      const top = r.top - pr.top;
      const bottom = pr.bottom - r.bottom;
      if (top >= 1 && top <= 10 && Math.abs(top - bottom) <= 1 && r.height >= 16) {
        const want = cornerOf(parent, pr) - top;
        const have = cornerOf(el, r);
        if (want > 4 && have + 2 < want)
          report(
            "concentric",
            el,
            `corner ${Math.round(have)}px inside a ${Math.round(cornerOf(parent, pr))}px parent inset ${Math.round(top)}px; want ${Math.round(want)}px`,
          );
      }
    }

    // Nested scrollers on the same axis (only ones that actually scroll).
    for (const axis of ["x", "y"]) {
      const active =
        axis === "x" ? el.scrollWidth > el.clientWidth + 1 : el.scrollHeight > el.clientHeight + 1;
      if (!scrolls(s, axis) || !active) continue;
      for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
        const as = css(a);
        const aActive =
          axis === "x" ? a.scrollWidth > a.clientWidth + 1 : a.scrollHeight > a.clientHeight + 1;
        if (scrolls(as, axis) && aActive) {
          report("nested-scroll", el, `${axis}-scroller inside ${axis}-scroller ${path(a)}`);
          break;
        }
      }
    }

    // Minimum widths for marked layout elements.
    const kind = el.getAttribute("data-layout");
    const min = Number(el.getAttribute("data-min-width")) || (kind && MIN[kind]);
    if (min) {
      const w = el.getBoundingClientRect().width;
      if (w + 0.5 < min)
        report("min-width", el, `${kind || "element"} is ${Math.round(w)}px, minimum ${min}px`);
    }

    // Text-level rules on elements that hold text themselves.
    if (!inline && ownText(el)) {
      const words = text(el).split(" ").filter(Boolean).length;
      const lines = lineCount(el);
      if (
        lines >= 2 &&
        ((words >= 4 && lines > words / 2) || (words >= 2 && words <= 3 && lines >= words))
      )
        report(
          "word-per-line",
          el,
          `${words} words on ${lines} lines in ${Math.round(el.clientWidth)}px`,
        );
      if (lines >= 2 && el.clientWidth > 0 && el.clientWidth < 12 * chWidth(el))
        report(
          "narrow-text",
          el,
          `${Math.round(el.clientWidth)}px is under 12ch (${Math.round(12 * chWidth(el))}px)`,
        );
      if (el.scrollWidth > el.clientWidth + 1 && s.overflowX === "hidden") {
        if (s.textOverflow === "ellipsis") {
          if (!hasTooltip(el))
            report(
              "no-tooltip",
              el,
              "truncated with an ellipsis but no title to show the full text",
            );
        } else
          report(
            "clipped",
            el,
            `${el.scrollWidth}px of text cut to ${el.clientWidth}px with no ellipsis`,
          );
      }
    }
  }
  // The Gond mark: never on the rail; exactly one in the window chrome and the Gond bar
  // when the app draws its own caption, none in-app under Windows' caption (which shows the icon).
  // Crew avatars that reuse the cog for running agents are allowed, so only the logo itself counts.
  const titlebar = document.querySelector('[data-entity="titlebar"]');
  const captionMode = titlebar ? "header" : "none";
  const maxMarks = titlebar ? 1 : 0;
  // Gond Code's mark is the triangle; the cog is Gond's and must not stand in for it.
  const svgText = (src) => (src.startsWith("data:image/svg") ? decodeURIComponent(src) : src);
  const isTriangle = (src) =>
    /M58\.53(%20| )26\.38/.test(svgText(src)) || /gond-code[^/]*\.svg/i.test(src);
  const isCog = (src) =>
    /M40\.14(%20| )25\.36/.test(svgText(src)) ||
    (/\/gond[-.][^/]*\.svg/i.test(src) && !/gond-code/i.test(src));
  const marks = [
    ...document.querySelectorAll('[data-slot="gond-mark"], [data-entity="gond-mark"], img'),
  ].filter((el) => {
    if (el.tagName === "IMG" && !el.matches("[data-slot], [data-entity]")) {
      const src = el.getAttribute("src") || "";
      if (!/gond[^/]*\.svg/i.test(src) && !isTriangle(src) && !isCog(src)) return false;
    }
    // Visible is what counts: a modal sheet marks the app behind it aria-hidden, but the bar's
    // mark is still on screen.
    return visible(el);
  });
  for (const el of marks.filter((m) => m.tagName === "IMG" && isCog(m.getAttribute("src") || "")))
    report("logo-count", el, "the Gond cog stands in for Gond Code's mark; use the triangle");
  const RAIL = '[data-layout="rail"], [data-entity="nav"], [data-entity="rail"]';
  for (const el of marks.filter((m) => m.closest(RAIL)))
    report(
      "logo-count",
      el,
      "the Gond mark is on the rail; it belongs in the title bar / Gond bar only",
    );
  const GREETING = '[data-entity="greeting"]';
  const greeting = marks.filter((m) => m.closest(GREETING));
  if (greeting.length > 1)
    report("logo-count", greeting[1], "more than one Gond mark in the new chat's greeting");
  const chrome = marks.filter((m) => !m.closest(RAIL) && !m.closest(GREETING));
  if (chrome.length !== maxMarks)
    out.push({
      rule: "logo-count",
      where: chrome.map(path).join(" · ") || "window chrome",
      text: "",
      detail:
        maxMarks === 0
          ? `${chrome.length} Gond mark(s) in-app; none under Windows' caption, which shows the icon (caption: ${captionMode || "unknown"})`
          : `${chrome.length} Gond marks in the title bar / Gond bar; exactly one (caption: ${captionMode || "unknown"})`,
    });

  // Controls must show their whole border box.
  const CONTROLS =
    'button, a[href], input, select, textarea, [role="button"], [role="tab"], [data-command]';
  const bar = document.querySelector('[data-entity="titlebar"]');
  for (const el of document.querySelectorAll(CONTROLS)) {
    if (!visible(el) || optedOut(el) || srOnly(el) || el.closest("svg, .xterm")) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) continue;
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      const st = css(a);
      // Inside a scroller the control scrolls into view; what's beyond it isn't cut.
      if (scrolls(st, "x") || scrolls(st, "y") || a.matches('[data-overflow="scroll"]')) break;
      const clipsX = /(hidden|clip)/.test(st.overflowX);
      const clipsY = /(hidden|clip)/.test(st.overflowY);
      if (!clipsX && !clipsY) continue;
      const ar = a.getBoundingClientRect();
      const cut =
        (clipsY && (r.top < ar.top - 1 || r.bottom > ar.bottom + 1)) ||
        (clipsX && (r.left < ar.left - 1 || r.right > ar.right + 1));
      // Wholly outside is hidden on purpose (a collapsed panel), not cut.
      const outside =
        r.bottom <= ar.top || r.top >= ar.bottom || r.right <= ar.left || r.left >= ar.right;
      if (cut && !outside) {
        report("control-clipped", el, `border box cut by ${path(a)} (overflow hidden)`);
        break;
      }
    }
  }

  // The app header: every control on one centre line.
  if (bar) {
    const br = bar.getBoundingClientRect();
    const line = (br.top + br.bottom - (parseFloat(css(bar).borderBottomWidth) || 0)) / 2;
    for (const el of bar.querySelectorAll(CONTROLS)) {
      if (
        !visible(el) ||
        srOnly(el) ||
        el.closest('[role="menu"], [data-radix-popper-content-wrapper]')
      )
        continue;
      const r = el.getBoundingClientRect();
      if (r.height < 8) continue;
      const off = (r.top + r.bottom) / 2 - line;
      if (Math.abs(off) > 1)
        report(
          "header-centred",
          el,
          `centre ${off > 0 ? "+" : ""}${off.toFixed(1)}px off the header's centre line`,
        );
    }
  }

  // A control's own label must sit on one line: each text node inside a menu item, button or
  // badge is measured, so two deliberate lines in separate elements never count.
  const LABELS =
    'button, [role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"], [data-slot="badge"], [data-slot$="-menu-item"]';
  for (const el of document.querySelectorAll(LABELS)) {
    if (!visible(el) || optedOut(el) || srOnly(el)) continue;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      // Short labels only (2 to 4 words, like "Close thread"): a sentence-long title on a card or
      // an approval may wrap cleanly onto more lines, which the layout rules allow.
      const words = n.textContent.trim().split(/\s+/).length;
      if (!n.textContent.trim() || words < 2 || words > 4) continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      const tops = new Set(
        [...range.getClientRects()].filter((r) => r.width > 0).map((r) => Math.round(r.top)),
      );
      if (tops.size > 1) {
        report(
          "label-wrap",
          el,
          `"${n.textContent.trim().slice(0, 40)}" wraps onto ${tops.size} lines`,
        );
        break;
      }
    }
  }

  // Report the innermost overflowing boxes only: a parent overflows because its child does.
  for (const el of overflowing) {
    if ([...overflowing].some((o) => o !== el && el.contains(o))) continue;
    const s = css(el);
    if (s.overflowX === "hidden") continue; // handled as clipped / no-tooltip above
    report("overflow", el, `content ${el.scrollWidth}px in a ${el.clientWidth}px box`);
  }
  return out;
})();
