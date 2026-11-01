import { useState } from "react";
import { ChevronDownIcon, PlusIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { can, headerProblem, type AccountOptions } from "@/lib/accounts";
import { cn } from "@/lib/utils";

type Props = {
  harness: string;
  value: AccountOptions;
  onChange: (o: AccountOptions) => void;
};

const CLOUD = [
  ["none", "Direct"],
  ["bedrock", "Amazon Bedrock"],
  ["vertex", "Google Vertex"],
] as const;

/**
 * An API account's options, only those the harness maps: the cloud for
 * Claude, Bearer auth, extra headers, OpenAI org and project, Azure and the wire API. Folded under
 * "More options" so a plain key stays a two-field form.
 */
export function EndpointOptions({ harness, value: o, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const set = (patch: Partial<AccountOptions>) => onChange({ ...o, ...patch });
  const headers = o.headers ?? [];
  const cloud = o.cloud?.kind ?? "none";

  return (
    <div className="flex flex-col gap-4">
      {can(harness, "cloud") && (
        <Row label="Where it runs">
          <ToggleGroup
            type="single"
            value={cloud}
            onValueChange={(v) =>
              v &&
              set({
                cloud:
                  v === "bedrock"
                    ? { kind: "bedrock", region: "" }
                    : v === "vertex"
                      ? { kind: "vertex", region: "", project: "" }
                      : undefined,
              })
            }
            aria-label="Where it runs"
            data-local="account-draft"
            spacing={0}
            track
            className="w-fit flex-wrap"
          >
            {CLOUD.map(([v, label]) => (
              <ToggleGroupItem
                key={v}
                value={v}
                data-local="account-draft"
                className="h-8 rounded-full px-4 whitespace-nowrap data-[state=on]:bg-hover"
              >
                {label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </Row>
      )}
      {o.cloud && (
        <div className="grid gap-3 min-[900px]:grid-cols-2">
          <TextField
            label="Region"
            value={o.cloud.region}
            placeholder={o.cloud.kind === "bedrock" ? "us-east-1" : "us-east5"}
            onChange={(region) => set({ cloud: { ...o.cloud!, region } })}
          />
          {o.cloud.kind === "vertex" && (
            <TextField
              label="Google Cloud project"
              value={o.cloud.project}
              onChange={(project) =>
                set({ cloud: { kind: "vertex", region: o.cloud!.region, project } })
              }
            />
          )}
        </div>
      )}

      <Collapsible open={open} onOpenChange={setOpen} className="flex flex-col gap-4">
        <CollapsibleTrigger
          data-local="account-more"
          className="flex w-fit items-center gap-1 text-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
        >
          More options
          <ChevronDownIcon className={cn("size-4 transition-transform", open && "rotate-180")} />
        </CollapsibleTrigger>
        <CollapsibleContent className="flex flex-col gap-4">
          {can(harness, "bearer") && (
            <label className="flex items-start gap-3 text-sm">
              <Switch
                data-local="account-draft"
                aria-label="Send key as Bearer"
                className="mt-0.5 after:inset-x-0"
                checked={!!o.bearer}
                onCheckedChange={(bearer) => set({ bearer })}
              />
              <span className="max-w-[72ch]">
                Send key as Bearer{" "}
                <span className="text-muted-foreground">
                  Most gateways, such as LiteLLM and OpenRouter, want this.
                </span>
              </span>
            </label>
          )}
          {can(harness, "organization") && (
            <div className="grid gap-3 min-[900px]:grid-cols-2">
              <TextField
                label="Organization"
                value={o.organization ?? ""}
                placeholder="org-…"
                onChange={(organization) => set({ organization })}
              />
              <TextField
                label="Project"
                value={o.project ?? ""}
                placeholder="proj_…"
                onChange={(project) => set({ project })}
              />
            </div>
          )}
          {can(harness, "azure") && (
            <TextField
              label="Azure API version"
              hint="Set it and the address is Azure OpenAI"
              value={o.azureApiVersion ?? ""}
              placeholder="2025-04-01-preview"
              onChange={(azureApiVersion) => set({ azureApiVersion })}
            />
          )}
          {can(harness, "wireApi") && (
            <Row label="Wire API" hint="Advanced">
              <ToggleGroup
                type="single"
                value={o.wireApi ?? ""}
                onValueChange={(v) =>
                  set({ wireApi: (v || undefined) as AccountOptions["wireApi"] })
                }
                aria-label="Wire API"
                data-local="account-draft"
                spacing={0}
                track
                className="w-fit"
              >
                {(["responses", "chat"] as const).map((v) => (
                  <ToggleGroupItem
                    key={v}
                    value={v}
                    data-local="account-draft"
                    className="h-8 rounded-full px-4 font-mono text-xs data-[state=on]:bg-hover"
                  >
                    {v}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </Row>
          )}
          {can(harness, "headers") && (
            <Row label="Headers" hint="Extra headers; the key is sent for you">
              <ul className="flex flex-col gap-2">
                {headers.map((h, i) => {
                  const problem = h.name ? headerProblem(h, headers) : undefined;
                  return (
                    <li key={i} className="flex flex-col gap-1">
                      <div className="flex items-center gap-2">
                        <Input
                          aria-label={`Header ${i + 1} name`}
                          data-local="account-draft"
                          value={h.name}
                          placeholder="X-Team"
                          aria-invalid={!!problem}
                          onChange={(e) =>
                            set({
                              headers: headers.map((x, j) =>
                                j === i ? { ...x, name: e.target.value } : x,
                              ),
                            })
                          }
                          className="h-9 w-48 shrink-0 rounded-full font-mono"
                        />
                        <Input
                          aria-label={`Header ${i + 1} value`}
                          data-local="account-draft"
                          value={h.value}
                          onChange={(e) =>
                            set({
                              headers: headers.map((x, j) =>
                                j === i ? { ...x, value: e.target.value } : x,
                              ),
                            })
                          }
                          className="h-9 min-w-0 flex-1 rounded-full font-mono"
                        />
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          data-local="account-draft"
                          aria-label={`Remove header ${i + 1}`}
                          onClick={() => set({ headers: headers.filter((_, j) => j !== i) })}
                        >
                          <XIcon />
                        </Button>
                      </div>
                      {problem && <p className="pl-4 text-xs text-tone-warn">{problem}</p>}
                    </li>
                  );
                })}
                <li>
                  <Button
                    variant="ghost"
                    size="sm"
                    data-local="account-draft"
                    onClick={() => set({ headers: [...headers, { name: "", value: "" }] })}
                  >
                    <PlusIcon data-icon="inline-start" /> Add a header
                  </Button>
                </li>
              </ul>
            </Row>
          )}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div role="group" aria-label={label} className="flex min-w-0 flex-col gap-2">
      <p className="text-sm font-medium">
        {label} {hint && <span className="font-normal text-muted-foreground">{hint}</span>}
      </p>
      {children}
    </div>
  );
}

function TextField({
  label,
  hint,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  hint?: string;
  value: string;
  placeholder?: string;
  onChange: (v: string) => void;
}) {
  return (
    <Row label={label} hint={hint}>
      <Input
        aria-label={label}
        data-local="account-draft"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="h-10 rounded-full font-mono"
      />
    </Row>
  );
}
