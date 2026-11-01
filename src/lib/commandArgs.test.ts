// Every command call in the UI sends what the backend accepts (Settings once sent
// harness.install `{ harness }` and the backend refused it with "unknown field `harness`").
// This reads every `onCommand(...)`, `runCommand(...)` and `callOrRun(...)` with a literal name,
// and checks its literal input against that command's real input schema, exported from Rust in
// commands.generated.json: no unknown fields, every required field, and literal values of the
// right type. Inputs that aren't literals can't be read statically and are skipped.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript";
import { expect, it } from "vitest";
import generated from "./commands.generated.json";

type Schema = {
  $ref?: string;
  $defs?: Record<string, Schema>;
  type?: string | string[];
  properties?: Record<string, Schema>;
  required?: string[];
  additionalProperties?: boolean | Schema;
  oneOf?: Schema[];
  anyOf?: Schema[];
  allOf?: Schema[];
  enum?: unknown[];
  const?: unknown;
};

const SCHEMAS = new Map(
  (generated as { name: string; inputSchema?: Schema }[]).map((c) => [c.name, c.inputSchema]),
);

/** What we know of an input statically: a literal, an object of known keys, or unknown. */
type Shape =
  | { kind: "literal"; value: string | number | boolean | null }
  | { kind: "object"; props: Map<string, Shape>; open: boolean }
  | { kind: "unknown" };

const CALLEES = new Set(["onCommand", "runCommand", "callOrRun"]);
/** Names the deck router handles itself, with a different input than the UI's (useDeck LIVE). */
const LOCAL: Record<string, string> = {
  "project.add": "opens the folder picker, which then sends project.add { path }",
};
const ROOT = join(__dirname, "..");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return files(p);
    return /\.tsx?$/.test(f) && !/\.test\.tsx?$|\.d\.ts$/.test(f) ? [p] : [];
  });
}

function shapeOf(n: ts.Expression): Shape {
  if (ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isSatisfiesExpression(n))
    return shapeOf(n.expression);
  if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n))
    return { kind: "literal", value: n.text };
  if (ts.isNumericLiteral(n)) return { kind: "literal", value: Number(n.text) };
  if (n.kind === ts.SyntaxKind.TrueKeyword) return { kind: "literal", value: true };
  if (n.kind === ts.SyntaxKind.FalseKeyword) return { kind: "literal", value: false };
  if (n.kind === ts.SyntaxKind.NullKeyword) return { kind: "literal", value: null };
  if (ts.isObjectLiteralExpression(n)) {
    const props = new Map<string, Shape>();
    let open = false;
    for (const p of n.properties) {
      if (ts.isPropertyAssignment(p) && (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name)))
        props.set(p.name.text, shapeOf(p.initializer));
      else if (ts.isShorthandPropertyAssignment(p)) props.set(p.name.text, { kind: "unknown" });
      else open = true; // a spread or a computed key: some keys can't be known
    }
    return { kind: "object", props, open };
  }
  return { kind: "unknown" };
}

function resolve(s: Schema, root: Schema): Schema {
  if (!s.$ref) return s;
  const name = s.$ref.replace("#/$defs/", "");
  return resolve(root.$defs?.[name] ?? {}, root);
}

const typeOf = (v: string | number | boolean | null) =>
  v === null
    ? "null"
    : typeof v === "number"
      ? Number.isInteger(v)
        ? "integer"
        : "number"
      : typeof v;

/** Why `shape` doesn't fit `schema`, or [] when it does (or can't be told). */
function problems(shape: Shape, schema: Schema, root: Schema, at: string): string[] {
  const s = resolve(schema, root);
  const variants = s.oneOf ?? s.anyOf;
  if (variants) {
    const each = variants.map((v) => problems(shape, v, root, at));
    return each.some((p) => p.length === 0)
      ? []
      : each.reduce((a, b) => (a.length <= b.length ? a : b));
  }
  if (s.allOf) return s.allOf.flatMap((v) => problems(shape, v, root, at));
  if (shape.kind === "unknown") return [];
  const here = `\`${at.slice(0, -1) || "input"}\``;
  const types = s.type === undefined ? undefined : [s.type].flat();
  if (shape.kind === "literal") {
    if (s.const !== undefined && s.const !== shape.value)
      return [`${here} must be ${JSON.stringify(s.const)}`];
    if (s.enum && !s.enum.includes(shape.value))
      return [`${here} must be one of ${JSON.stringify(s.enum)}`];
    const t = typeOf(shape.value);
    if (types && !types.includes(t) && !(t === "integer" && types.includes("number")))
      return [`${here} is ${t}, expected ${types.join(" | ")}`];
    return [];
  }
  if (types && !types.includes("object"))
    return [`${here} is an object, expected ${types.join(" | ")}`];
  const out: string[] = [];
  const props = s.properties ?? {};
  if (s.additionalProperties === false)
    for (const k of shape.props.keys())
      if (!(k in props))
        out.push(
          `unknown field \`${at}${k}\`, expected one of ${Object.keys(props).join(", ") || "none"}`,
        );
  if (!shape.open)
    for (const k of s.required ?? [])
      if (!shape.props.has(k)) out.push(`missing field \`${at}${k}\``);
  for (const [k, v] of shape.props)
    if (props[k]) out.push(...problems(v, props[k], root, `${at}${k}.`));
  return out;
}

function calls() {
  const found: { where: string; name: string; problems: string[] }[] = [];
  for (const file of files(ROOT)) {
    const src = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true);
    const visit = (n: ts.Node) => {
      if (ts.isCallExpression(n)) {
        const callee = ts.isIdentifier(n.expression)
          ? n.expression.text
          : ts.isPropertyAccessExpression(n.expression)
            ? n.expression.name.text
            : "";
        const [first, input] = n.arguments;
        // A literal name, or `cond ? "a.b": "c.d"` (each branch is checked).
        const names = !first
          ? []
          : ts.isStringLiteral(first)
            ? [first.text]
            : ts.isConditionalExpression(first) &&
                ts.isStringLiteral(first.whenTrue) &&
                ts.isStringLiteral(first.whenFalse)
              ? [first.whenTrue.text, first.whenFalse.text]
              : [];
        for (const name of CALLEES.has(callee) ? names : []) {
          const schema = SCHEMAS.get(name);
          if (!schema || (callee === "onCommand" && LOCAL[name])) continue;
          const line = src.getLineAndCharacterOfPosition(n.getStart()).line + 1;
          const shape: Shape = input
            ? shapeOf(input)
            : { kind: "object", props: new Map(), open: false };
          found.push({
            where: `${relative(ROOT, file)}:${line}`,
            name,
            problems: problems(shape, schema, schema, ""),
          });
        }
      }
      ts.forEachChild(n, visit);
    };
    visit(src);
  }
  return found;
}

it("reads the UI's command calls (the check isn't vacuous)", () => {
  expect(calls().length).toBeGreaterThan(40);
});

it("every command call's input fits that command's real input schema", () => {
  const bad = calls()
    .filter((c) => c.problems.length)
    .map((c) => `${c.where} ${c.name}: ${c.problems.join("; ")}`);
  expect(bad).toEqual([]);
});

it("catches the dev.1492 bug: harness.install { harness }", () => {
  const schema = SCHEMAS.get("harness.install")!;
  const src = ts.createSourceFile(
    "x.ts",
    `({ harness: "claude", when: "afterStep" })`,
    ts.ScriptTarget.Latest,
  );
  const expr = (src.statements[0] as ts.ExpressionStatement).expression;
  expect(problems(shapeOf(expr), schema, schema, "")).toEqual([
    "unknown field `harness`, expected one of harnessId, machineScope, planIndex",
    "unknown field `when`, expected one of harnessId, machineScope, planIndex",
    "missing field `harnessId`",
  ]);
});
