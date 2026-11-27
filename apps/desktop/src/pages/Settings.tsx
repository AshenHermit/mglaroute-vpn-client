import { useState } from "react";
import {
  Tabs,
  TabsContent,
  TabsContents,
  TabsHighlight,
  TabsHighlightItem,
  TabsList,
  TabsTrigger,
} from "../components/ui/tabs";
import { ServiceProfileEditor } from "../features/service-profile-editor/ServiceProfileEditor";
import { SettingsForm } from "../features/settings-form/SettingsForm";
import { SubscriptionList } from "../features/subscription-list/SubscriptionList";
import { UpdatePanel } from "../features/updates/UpdatePanel";

type SettingsTab = "subscriptions" | "serviceProfiles" | "settings" | "updates";

const tabs: ReadonlyArray<{ value: SettingsTab; label: string }> = [
  { value: "subscriptions", label: "Subscriptions" },
  { value: "serviceProfiles", label: "Service profiles" },
  { value: "settings", label: "Settings" },
  { value: "updates", label: "Updates" },
];

/** Settings is a center-zone page: rail and bottom navigation stay mounted. */
export default function Settings() {
  const [tab, setTab] = useState<SettingsTab>("subscriptions");

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <h2 className="text-lg font-semibold">Settings</h2>
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as SettingsTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="grid shrink-0 grid-cols-4 gap-1 border-b border-border p-2">
          <TabsHighlight className="rounded-md bg-muted">
            {tabs.map(({ value, label }) => (
              <TabsHighlightItem key={value} value={value} className="min-h-11">
                <TabsTrigger
                  value={value}
                  className="min-h-11 w-full rounded-md px-1 text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring data-[state=active]:text-foreground"
                >
                  {label}
                </TabsTrigger>
              </TabsHighlightItem>
            ))}
          </TabsHighlight>
        </TabsList>

        <TabsContents className="min-h-0 flex-1 px-1 pb-2 pt-3">
          <TabsContent value="subscriptions">
            <SubscriptionList />
          </TabsContent>
          <TabsContent value="serviceProfiles">
            <ServiceProfileEditor />
          </TabsContent>
          <TabsContent value="settings">
            <SettingsForm />
          </TabsContent>
          <TabsContent value="updates">
            <UpdatePanel />
          </TabsContent>
        </TabsContents>
      </Tabs>
    </div>
  );
}
