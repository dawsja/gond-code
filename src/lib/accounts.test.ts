import { expect, it } from "vitest";
import { endpointOk, headerProblem, optionsFor } from "./accounts";

it("takes https, or http only on this PC", () => {
  expect(endpointOk("https://openrouter.ai/api/v1")).toBe(true);
  expect(endpointOk("http://localhost:11434/v1")).toBe(true);
  expect(endpointOk("http://127.0.0.1:4000")).toBe(true);
  expect(endpointOk("http://gateway.internal")).toBe(false);
  expect(endpointOk("https://")).toBe(false);
});

it("refuses auth headers and duplicates", () => {
  const all = [
    { name: "x-api-key", value: "k" },
    { name: "X-Team", value: "a" },
    { name: "x-team", value: "b" },
  ];
  expect(headerProblem(all[0], all)).toMatch(/Auth headers/);
  expect(headerProblem(all[1], all)).toBe("Already added");
  expect(headerProblem({ name: "X-Ok", value: "" }, [{ name: "X-Ok", value: "" }])).toBeUndefined();
});

it("sends only what the harness maps", () => {
  const o = {
    bearer: true,
    organization: "org-1",
    azureApiVersion: "2025-04-01",
    headers: [{ name: "X-Team", value: "web" }],
  };
  expect(optionsFor("claude", o)).toEqual({ bearer: true, headers: { "X-Team": "web" } });
  expect(optionsFor("codex", o)).toEqual({
    organization: "org-1",
    azureApiVersion: "2025-04-01",
    headers: { "X-Team": "web" },
  });
  expect(optionsFor("grok", o)).toBeUndefined();
});
