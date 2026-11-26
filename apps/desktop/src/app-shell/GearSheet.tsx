import {
  Tabs,
  TabsContent,
  TabsContents,
  TabsHighlight,
  TabsHighlightItem,
  TabsList,
  TabsTrigger,
} from "@animate-ui/react/primitives/tabs";
import { Dialog } from "../components/ui/dialog";
import { ServiceProfileEditor } from "../features/service-profile-editor/ServiceProfileEditor";
import { SettingsForm } from "../features/settings-form/SettingsForm";
import { SubscriptionList } from "../features/subscription-list/SubscriptionList";
import type { SheetTab } from "../stores/ui.store";
import { useUiStore } from "../stores/ui.store";

export type GearSheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

const tabs: ReadonlyArray<{ value: SheetTab; label: string }> = [
  { value: "subscriptions", label: "Subscriptions" },
  { value: "serviceProfiles", label: "Service profiles" },
  { value: "settings", label: "Settings" },
];

/** Right-side sheet hosting the three real advanced pages. */
export function GearSheet({ open, onOpenChange }: GearSheetProps) {
  const sheetTab = useUiStore((store) => store.sheetTab);
  const setSheetTab = useUiStore((store) => store.setSheetTab);

  return (
    <Dialog
      open={open}
      onClose={() => onOpenChange(false)}
      title="Settings"
      closeLabel="Close settings"
      contentClassName="fixed right-0 top-0 flex h-full w-full max-w-none translate-x-0 translate-y-0 flex-col overflow-hidden rounded-none border-l border-border bg-card p-0 text-foreground sm:w-[min(480px,100vw)]"
    >
      <Tabs
        value={sheetTab}
        onValueChange={(value) => setSheetTab(value as SheetTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="grid shrink-0 grid-cols-3 gap-1 border-b border-border p-2">
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

        <TabsContents className="min-h-0 flex-1 px-4 pb-4 pt-3">
          <TabsContent value="subscriptions" className="h-full min-h-0 overflow-y-auto">
            <SubscriptionList />
          </TabsContent>
          <TabsContent value="serviceProfiles" className="h-full min-h-0 overflow-y-auto">
            <ServiceProfileEditor />
          </TabsContent>
          <TabsContent value="settings" className="h-full min-h-0 overflow-y-auto">
            <SettingsForm />
          </TabsContent>
        </TabsContents>
      </Tabs>
    </Dialog>
  );
}
