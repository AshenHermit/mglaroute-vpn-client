import {
  RoutingProfileSchema,
  RoutingRuleSchema,
  type RoutingProfile,
  type RoutingRule,
} from "@mglaroute/contracts";
import { useCallback, useEffect, useState } from "react";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

type Outbound = RoutingRule["outbound"];

type Draft = {
  id: string | null;
  remark: string;
  enabled: boolean;
  outbound: Outbound;
  domains: string;
  ips: string;
};

const emptyDraft = (): Draft => ({
  id: null,
  remark: "",
  enabled: true,
  outbound: "proxy",
  domains: "",
  ips: "",
});

const splitEntries = (text: string): string[] =>
  text
    .split(/[\n,]+/)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);

const ruleDraft = (rule: RoutingRule): Draft => ({
  id: rule.id,
  remark: rule.remark,
  enabled: rule.enabled,
  outbound: rule.outbound,
  domains: rule.domains.join("\n"),
  ips: rule.ips.join("\n"),
});

const textareaClass =
  "w-full min-h-11 min-w-0 rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-hidden focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background";

/** Local-only editor: routing state lives in SQLite, never in Zustand persist. */
export function RoutingEditor() {
  const setNotice = useAppStore((store) => store.setNotice);
  const [profiles, setProfiles] = useState<RoutingProfile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  const [rules, setRules] = useState<RoutingRule[]>([]);
  const [newProfileName, setNewProfileName] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(
    async (preferredId?: string | null) => {
      try {
        const rows = await invoke("routing_profile_list", RoutingProfileSchema.array());
        setProfiles(rows);
        const selected =
          preferredId && rows.some((profile) => profile.id === preferredId)
            ? preferredId
            : (rows[0]?.id ?? null);
        setSelectedProfileId(selected);
        if (!selected) {
          setRules([]);
          return;
        }
        const nextRules = await invoke("routing_rule_list", RoutingRuleSchema.array(), {
          profileId: selected,
        });
        setRules(nextRules);
      } catch {
        setNotice("Could not load routing profiles");
      }
    },
    [setNotice],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const selectProfile = async (profileId: string) => {
    setError(null);
    setDraft(null);
    setSelectedProfileId(profileId);
    try {
      const rows = await invoke("routing_rule_list", RoutingRuleSchema.array(), {
        profileId,
      });
      setRules(rows);
    } catch {
      setNotice("Could not load routing rules");
    }
  };

  const createProfile = async () => {
    const name = newProfileName.trim();
    if (!name) {
      setError("Enter a profile name.");
      return;
    }
    setError(null);
    try {
      const created = await invoke("routing_profile_create", RoutingProfileSchema, {
        name,
      });
      setNewProfileName("");
      await refresh(created.id);
    } catch {
      setNotice("Could not create routing profile");
    }
  };

  const activateProfile = async (profile: RoutingProfile) => {
    try {
      await invoke("routing_profile_activate", RoutingProfileSchema, { id: profile.id });
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not activate routing profile");
    }
  };

  const toggleProfile = async (profile: RoutingProfile) => {
    try {
      await invoke("routing_profile_update", RoutingProfileSchema, {
        id: profile.id,
        name: profile.name,
        enabled: !profile.enabled,
      });
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not update routing profile");
    }
  };

  const saveRule = async () => {
    if (!draft) return;
    const remark = draft.remark.trim();
    if (!selectedProfileId) {
      setError("Select a profile first.");
      return;
    }
    if (!remark) {
      setError("Enter a rule remark.");
      return;
    }
    const payload = {
      remark,
      enabled: draft.enabled,
      outbound: draft.outbound,
      domains: splitEntries(draft.domains),
      ips: splitEntries(draft.ips),
    };
    setError(null);
    try {
      if (draft.id) {
        await invoke("routing_rule_update", RoutingRuleSchema, {
          id: draft.id,
          ...payload,
        });
      } else {
        await invoke("routing_rule_create", RoutingRuleSchema, {
          profileId: selectedProfileId,
          ...payload,
        });
      }
      setDraft(null);
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not save routing rule");
    }
  };

  const toggleRule = async (rule: RoutingRule) => {
    try {
      await invoke("routing_rule_update", RoutingRuleSchema, {
        id: rule.id,
        remark: rule.remark,
        enabled: !rule.enabled,
        outbound: rule.outbound,
        domains: rule.domains,
        ips: rule.ips,
      });
      if (draft?.id === rule.id) setDraft({ ...draft, enabled: !rule.enabled });
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not update routing rule");
    }
  };

  const removeRule = async () => {
    if (!draft?.id) return;
    try {
      await invoke("routing_rule_remove", z.null(), { id: draft.id });
      setDraft(null);
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not remove routing rule");
    }
  };

  const reorderRule = async (direction: -1 | 1) => {
    if (!draft?.id) return;
    try {
      await invoke("routing_rule_reorder", z.null(), {
        id: draft.id,
        direction,
      });
      await refresh(selectedProfileId);
    } catch {
      setNotice("Could not reorder routing rules");
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Routing profiles</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="@container grid min-w-0 gap-4 @lg:grid-cols-2">
          <div className="min-w-0">
            <label htmlFor="routing-new-profile" className="mb-1 block text-xs text-muted-foreground">
              New profile name
            </label>
            <div className="flex gap-2">
              <Input
                id="routing-new-profile"
                className="min-w-0"
                value={newProfileName}
                onChange={(event) => setNewProfileName(event.target.value)}
                placeholder="Bypass local"
              />
              <Button
                className="shrink-0"
                aria-label="Add routing profile"
                onClick={() => void createProfile()}
              >
                +
              </Button>
            </div>
            {profiles.length === 0 ? (
              <p className="mt-3 text-sm text-muted-foreground">No routing profiles yet.</p>
            ) : (
              <ul className="mt-3 divide-y divide-border rounded-md border border-border">
                {profiles.map((profile) => (
                  <li key={profile.id} className="flex flex-wrap items-center gap-2 p-2">
                    <Button
                      variant={profile.id === selectedProfileId ? "primary" : "secondary"}
                      className="min-w-0 flex-1 justify-start"
                      onClick={() => void selectProfile(profile.id)}
                    >
                      <span className="truncate">{profile.name}</span>
                      {profile.isActive && (
                        <span className="ml-2 shrink-0 text-xs">Active</span>
                      )}
                    </Button>
                    <Switch
                      checked={profile.enabled}
                      label={`Enable ${profile.name}`}
                      onCheckedChange={() => void toggleProfile(profile)}
                    />
                    {!profile.isActive && (
                      <Button
                        variant="secondary"
                        aria-label={`Activate ${profile.name}`}
                        onClick={() => void activateProfile(profile)}
                      >
                        Activate
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="min-w-0">
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="text-sm font-medium">Rules</span>
              <Button onClick={() => setDraft(emptyDraft())}>+ Rule</Button>
            </div>
            {rules.length === 0 ? (
              <p className="text-sm text-muted-foreground">No rules in this profile.</p>
            ) : (
              <table className="w-full border-collapse text-left text-sm">
                <thead>
                  <tr className="border-b border-border text-xs text-muted-foreground">
                    <th className="p-2">Enabled</th>
                    <th className="p-2">Remark</th>
                    <th className="p-2">Outbound</th>
                    <th className="p-2">Domains / IPs</th>
                  </tr>
                </thead>
                <tbody>
                  {rules.map((rule) => (
                    <tr key={rule.id} className="border-b border-border align-top">
                      <td className="p-2">
                        <input
                          type="checkbox"
                          aria-label={`Enable ${rule.remark}`}
                          checked={rule.enabled}
                          onChange={() => void toggleRule(rule)}
                        />
                      </td>
                      <td className="p-2">
                        <Button
                          variant="secondary"
                          className="min-w-0"
                          onClick={() => setDraft(ruleDraft(rule))}
                        >
                          <span className="truncate">{rule.remark}</span>
                        </Button>
                      </td>
                      <td className="p-2">{rule.outbound}</td>
                      <td className="p-2 text-xs text-muted-foreground">
                        {[...rule.domains, ...rule.ips].join(", ")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>

        {error && (
          <p role="alert" className="mt-3 text-sm text-destructive">
            {error}
          </p>
        )}

        {draft && (
          <div className="mt-4 grid gap-3 rounded-md border border-border p-3 @lg:grid-cols-2">
            <div className="min-w-0">
              <label htmlFor="routing-rule-remark" className="mb-1 block text-xs text-muted-foreground">
                Remark
              </label>
              <Input
                id="routing-rule-remark"
                value={draft.remark}
                onChange={(event) => setDraft({ ...draft, remark: event.target.value })}
                placeholder="Bypass private networks"
              />
            </div>
            <div className="min-w-0">
              <label htmlFor="routing-rule-outbound" className="mb-1 block text-xs text-muted-foreground">
                Outbound
              </label>
              <select
                id="routing-rule-outbound"
                className={textareaClass}
                value={draft.outbound}
                onChange={(event) =>
                  setDraft({ ...draft, outbound: event.target.value as Outbound })
                }
              >
                <option value="proxy">proxy</option>
                <option value="direct">direct</option>
                <option value="block">block</option>
              </select>
            </div>
            <div className="min-w-0">
              <label htmlFor="routing-rule-domains" className="mb-1 block text-xs text-muted-foreground">
                Domains
              </label>
              <textarea
                id="routing-rule-domains"
                className={textareaClass}
                rows={3}
                value={draft.domains}
                onChange={(event) => setDraft({ ...draft, domains: event.target.value })}
                placeholder={"reg.ru\ngeosite:private"}
              />
            </div>
            <div className="min-w-0">
              <label htmlFor="routing-rule-ips" className="mb-1 block text-xs text-muted-foreground">
                IP / CIDR
              </label>
              <textarea
                id="routing-rule-ips"
                className={textareaClass}
                rows={3}
                value={draft.ips}
                onChange={(event) => setDraft({ ...draft, ips: event.target.value })}
                placeholder={"geoip:private\n10.0.0.0/8"}
              />
            </div>
            <Switch
              checked={draft.enabled}
              label="Enabled"
              onCheckedChange={(checked) => {
                if (draft.id) {
                  const rule = rules.find((item) => item.id === draft.id);
                  if (rule) void toggleRule({ ...rule, enabled: !checked });
                } else {
                  setDraft({ ...draft, enabled: checked });
                }
              }}
            />
            <div className="flex flex-wrap items-center gap-2 @lg:col-span-2">
              <Button onClick={() => void saveRule()}>Save rule</Button>
              <Button variant="secondary" onClick={() => setDraft(null)}>
                Cancel
              </Button>
              {draft.id && <Button variant="danger" onClick={() => void removeRule()}>Delete rule</Button>}
              {draft.id && (
                <>
                  <Button variant="secondary" onClick={() => void reorderRule(-1)}>
                    Move up
                  </Button>
                  <Button variant="secondary" onClick={() => void reorderRule(1)}>
                    Move down
                  </Button>
                </>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
