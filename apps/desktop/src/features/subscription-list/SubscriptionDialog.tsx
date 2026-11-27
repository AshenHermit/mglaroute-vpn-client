import {
  FeedServiceListSchema,
  SubscriptionSchema,
  type FeedService,
  type Subscription,
} from "@mglaroute/contracts";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui/button";
import { Dialog } from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import {
  Tabs,
  TabsContent,
  TabsContents,
  TabsList,
  TabsTrigger,
} from "../../components/ui/tabs";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

export type SubscriptionDialogProps = {
  open: boolean;
  mode: "add" | "edit";
  subscription: Subscription | null;
  onClose: () => void;
  onSaved: (saved: Subscription) => void;
};

const FEED_BASE_URL = "https://api.mglaroute.ashen-hermit.space/api/feed";
const FEED_HOST = "api.mglaroute.ashen-hermit.space";
const SORTS = [
  "latency_asc",
  "latency_desc",
  "confidence_asc",
  "confidence_desc",
  "name_asc",
  "name_desc",
  "address_asc",
  "address_desc",
  "port_asc",
  "port_desc",
  "protocol_asc",
  "protocol_desc",
] as const;

type FeedForm = {
  protocol: string;
  reachable: string;
  anycast: string;
  sort: string;
  search: string;
  limit: string;
  services: string[];
};

const INITIAL_FEED_FORM: FeedForm = {
  protocol: "",
  reachable: "",
  anycast: "",
  sort: "latency_asc",
  search: "",
  limit: "200",
  services: [],
};

function clampLimit(raw: string | null): string {
  const parsed = Number.parseInt(raw ?? "", 10);
  if (Number.isNaN(parsed) || parsed < 1 || parsed > 1000) return "200";
  return String(parsed);
}

function isFeedUrl(raw?: string | null): boolean {
  try {
    const parsed = new URL(raw ?? "");
    return parsed.hostname === FEED_HOST && parsed.pathname === "/api/feed";
  } catch {
    return false;
  }
}

function feedFormFromUrl(raw?: string | null): FeedForm {
  if (!isFeedUrl(raw)) return INITIAL_FEED_FORM;
  const params = new URL(raw!).searchParams;
  const sort = params.get("sort") ?? "";
  return {
    protocol: params.get("protocol") ?? "",
    reachable: params.get("reachable") ?? "",
    anycast: params.get("anycast") ?? "",
    sort: (SORTS as readonly string[]).includes(sort) ? sort : INITIAL_FEED_FORM.sort,
    search: params.get("search") ?? "",
    limit: clampLimit(params.get("limit")),
    services: (params.get("services") ?? "")
      .split(",")
      .map((service) => service.trim())
      .filter(Boolean),
  };
}

function buildFeedUrl(form: FeedForm): string {
  const params = new URLSearchParams();
  params.set("format", "subscription");
  if (form.search.trim()) params.set("search", form.search.trim());
  if (form.protocol) params.set("protocol", form.protocol);
  if (form.reachable) params.set("reachable", form.reachable);
  if (form.anycast) params.set("anycast", form.anycast);
  if (form.services.length) params.set("services", form.services.join(","));
  params.set("sort", form.sort);
  params.set("limit", clampLimit(form.limit));
  return `${FEED_BASE_URL}?${params.toString()}`;
}

export function SubscriptionDialog({
  open,
  mode,
  subscription,
  onClose,
  onSaved,
}: SubscriptionDialogProps) {
  const setNotice = useAppStore((store) => store.setNotice);
  const [name, setName] = useState(subscription?.name ?? "");
  const [url, setUrl] = useState(subscription?.url ?? "");
  const [tab, setTab] = useState<"url" | "mglaroute">(
    isFeedUrl(subscription?.url) ? "mglaroute" : "url",
  );
  const [form, setForm] = useState<FeedForm>(feedFormFromUrl(subscription?.url));
  const [services, setServices] = useState<FeedService[] | null>(null);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);

  useEffect(() => {
    if (tab !== "mglaroute" || services !== null) return;
    let active = true;
    void invoke("feed_services", FeedServiceListSchema)
      .then((rows) => {
        if (active) setServices(rows);
      })
      .catch(() => {
        if (active) setServices([]);
      });
    return () => {
      active = false;
    };
  }, [tab, services]);

  const toggleService = (id: string) => {
    setForm((current) => ({
      ...current,
      services: current.services.includes(id)
        ? current.services.filter((service) => service !== id)
        : [...current.services, id],
    }));
  };

  const generatedUrl = buildFeedUrl(form);

  const generate = () => {
    setUrl(generatedUrl);
  };

  const save = async () => {
    if (savingRef.current) return;
    const trimmed = (tab === "mglaroute" ? generatedUrl : url).trim();
    if (!trimmed) {
      setNotice("Subscription URL is required");
      return;
    }
    if (mode === "edit" && !name.trim()) {
      setNotice("Name is required");
      return;
    }
    savingRef.current = true;
    setSaving(true);
    try {
      const saved =
        mode === "edit"
          ? await invoke("subscription_update", SubscriptionSchema, {
              id: subscription!.id,
              name: name.trim(),
              url: trimmed,
            })
          : await invoke(
              "subscription_add",
              SubscriptionSchema,
              name.trim() ? { url: trimmed, name: name.trim() } : { url: trimmed },
            );
      onSaved(saved);
    } catch {
      setNotice(mode === "add" ? "Could not add subscription" : "Could not update subscription");
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const selectClass =
    "w-full min-h-11 rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={mode === "add" ? "Add subscription" : "Edit subscription"}
      contentClassName="inset-0 m-auto max-h-[90vh] w-[calc(100%-2rem)] max-w-2xl overflow-y-auto rounded-xl border border-border bg-card p-6 text-foreground"
    >
      <div className="space-y-4">
        <div className="space-y-1">
          <label htmlFor="subscription-name" className="block text-xs text-muted-foreground">
            Name
          </label>
          <Input
            id="subscription-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Optional"
          />
        </div>

        <Tabs value={tab} onValueChange={(value) => setTab(value as "url" | "mglaroute")}>
          <TabsList className="grid grid-cols-2 gap-1 border-b border-border p-1">
            <TabsTrigger value="url">URL</TabsTrigger>
            <TabsTrigger value="mglaroute">Mglaroute</TabsTrigger>
          </TabsList>
          <TabsContents className="pt-4">
            <TabsContent value="url" className="space-y-1">
              <label htmlFor="subscription-url" className="block text-xs text-muted-foreground">
                Subscription URL
              </label>
              <Input
                id="subscription-url"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                placeholder="https://example.com/subscription"
              />
            </TabsContent>

            <TabsContent value="mglaroute" className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1">
                  <label htmlFor="feed-protocol" className="block text-xs text-muted-foreground">
                    Protocol
                  </label>
                  <select
                    id="feed-protocol"
                    className={selectClass}
                    value={form.protocol}
                    onChange={(event) => setForm({ ...form, protocol: event.target.value })}
                  >
                    <option value="">All</option>
                    <option value="vless">VLESS</option>
                    <option value="vmess">VMess</option>
                    <option value="ss">SS</option>
                    <option value="trojan">Trojan</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <label htmlFor="feed-reachable" className="block text-xs text-muted-foreground">
                    Reachable
                  </label>
                  <select
                    id="feed-reachable"
                    className={selectClass}
                    value={form.reachable}
                    onChange={(event) => setForm({ ...form, reachable: event.target.value })}
                  >
                    <option value="">All</option>
                    <option value="true">Yes</option>
                    <option value="false">No</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <label htmlFor="feed-anycast" className="block text-xs text-muted-foreground">
                    Anycast
                  </label>
                  <select
                    id="feed-anycast"
                    className={selectClass}
                    value={form.anycast}
                    onChange={(event) => setForm({ ...form, anycast: event.target.value })}
                  >
                    <option value="">All</option>
                    <option value="true">Yes</option>
                    <option value="false">No</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <label htmlFor="feed-sort" className="block text-xs text-muted-foreground">
                    Sort
                  </label>
                  <select
                    id="feed-sort"
                    className={selectClass}
                    value={form.sort}
                    onChange={(event) => setForm({ ...form, sort: event.target.value })}
                  >
                    {SORTS.map((sort) => (
                      <option key={sort} value={sort}>
                        {sort}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1">
                  <label htmlFor="feed-limit" className="block text-xs text-muted-foreground">
                    Limit
                  </label>
                  <Input
                    id="feed-limit"
                    type="number"
                    min={1}
                    max={1000}
                    value={form.limit}
                    onChange={(event) => setForm({ ...form, limit: event.target.value })}
                  />
                </div>
                <div className="space-y-1">
                  <label htmlFor="feed-search" className="block text-xs text-muted-foreground">
                    Search
                  </label>
                  <Input
                    id="feed-search"
                    placeholder="Remarks, name or address"
                    value={form.search}
                    onChange={(event) => setForm({ ...form, search: event.target.value })}
                  />
                </div>
              </div>

              {services && services.length > 0 && (
                <div className="space-y-2">
                  <span className="block text-xs text-muted-foreground">
                    Online services (all selected must be reachable)
                  </span>
                  <div className="flex flex-wrap gap-2">
                    {services.map((service) => {
                      const active = form.services.includes(service.id);
                      return (
                        <Button
                          key={service.id}
                          variant={active ? "primary" : "secondary"}
                          data-state={active ? "active" : "inactive"}
                          onClick={() => toggleService(service.id)}
                        >
                          {service.name}
                        </Button>
                      );
                    })}
                  </div>
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2">
                <Button onClick={generate}>Generate link</Button>
              </div>

              <div className="space-y-1">
                <Input
                  readOnly
                  aria-label="Generated feed URL"
                  value={generatedUrl}
                  onFocus={(event) => event.target.select()}
                />
              </div>

              <p className="text-sm text-muted-foreground">
                Anyone with this link can read the feed. It exposes server connections without
                authentication — share carefully.
              </p>
            </TabsContent>
          </TabsContents>
        </Tabs>

        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={() => void save()} disabled={saving}>
            {mode === "add" ? "Add subscription" : "Save changes"}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
