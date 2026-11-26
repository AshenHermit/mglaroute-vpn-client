import { ServiceProfileSchema } from "@mglaroute/contracts";
import { useEffect, useState } from "react";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { invoke } from "../../lib/invoke";
import { useAppStore } from "../../stores/app.store";

export function ServiceProfileEditor() {
  const serviceProfiles = useAppStore((store) => store.serviceProfiles);
  const setServiceProfiles = useAppStore((store) => store.setServiceProfiles);
  const setNotice = useAppStore((store) => store.setNotice);
  const [name, setName] = useState("");
  const [urlsText, setUrlsText] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void invoke("service_profile_list", ServiceProfileSchema.array())
      .then((rows) => {
        if (active) setServiceProfiles(rows);
      })
      .catch(() => {
        if (active) setNotice("Could not load service profiles");
      });
    return () => {
      active = false;
    };
  }, [setServiceProfiles, setNotice]);

  const create = async () => {
    const trimmedName = name.trim();
    const testUrls = urlsText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    if (!trimmedName) {
      setError("Enter a profile name.");
      return;
    }
    if (testUrls.length === 0) {
      setError("Add at least one test URL.");
      return;
    }
    setError(null);
    try {
      const created = await invoke("service_profile_create", ServiceProfileSchema, {
        name: trimmedName,
        testUrls,
      });
      setServiceProfiles([...serviceProfiles, created]);
      setName("");
      setUrlsText("");
    } catch {
      setNotice("Could not create service profile");
    }
  };

  const toggle = async (id: string, isEnabled: boolean) => {
    try {
      const updated = await invoke("service_profile_update", ServiceProfileSchema, {
        id,
        isEnabled,
      });
      setServiceProfiles(
        serviceProfiles.map((profile) =>
          profile.id === id ? updated : profile,
        ),
      );
    } catch {
      setNotice("Could not update service profile");
    }
  };

  const remove = async (id: string) => {
    try {
      await invoke("service_profile_remove", z.null(), { id });
      setServiceProfiles(serviceProfiles.filter((profile) => profile.id !== id));
    } catch {
      setNotice("Could not remove service profile");
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Service profiles</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="mb-4 grid gap-3 sm:grid-cols-2">
          <div>
            <label htmlFor="service-profile-name" className="mb-1 block text-xs text-slate-600">
              Profile name
            </label>
            <Input
              id="service-profile-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Critical services"
            />
          </div>
          <div>
            <label htmlFor="service-profile-urls" className="mb-1 block text-xs text-slate-600">
              Test URLs (one per line)
            </label>
            <textarea
              id="service-profile-urls"
              value={urlsText}
              onChange={(event) => setUrlsText(event.target.value)}
              rows={3}
              className="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm outline-none focus:border-sky-600"
              placeholder="https://example.com/health"
            />
          </div>
        </div>
        <Button onClick={() => void create()}>Add profile</Button>
        {error && (
          <p role="alert" className="mt-3 text-sm text-red-700">
            {error}
          </p>
        )}

        {serviceProfiles.length === 0 ? (
          <p className="mt-4 text-sm text-slate-500">No service profiles yet.</p>
        ) : (
          <ul className="mt-4 divide-y divide-slate-200">
            {serviceProfiles.map((profile) => (
              <li
                key={profile.id}
                className="flex flex-wrap items-center justify-between gap-2 py-2"
              >
                <div>
                  <span className="font-medium">{profile.name}</span>
                  <span className="ml-2 text-xs text-slate-500">
                    {profile.testUrls.length} URL(s)
                  </span>
                </div>
                <div className="flex items-center gap-3">
                  <Switch
                    checked={profile.isEnabled}
                    label={`Enable ${profile.name}`}
                    onCheckedChange={(checked) => void toggle(profile.id, checked)}
                  />
                  <Button
                    variant="danger"
                    onClick={() => void remove(profile.id)}
                  >
                    Delete
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
