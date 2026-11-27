import {
  Tabs as TabsPrimitive,
  TabsContent as TabsContentPrimitive,
  TabsContents as TabsContentsPrimitive,
  TabsHighlight as TabsHighlightPrimitive,
  TabsHighlightItem as TabsHighlightItemPrimitive,
  TabsList as TabsListPrimitive,
  TabsTrigger as TabsTriggerPrimitive,
  type TabsContentProps,
  type TabsContentsProps,
  type TabsHighlightItemProps,
  type TabsHighlightProps,
  type TabsListProps,
  type TabsProps,
  type TabsTriggerProps,
  useTabs,
} from "@animate-ui/react/primitives/tabs";
import { cn } from "../../lib/utils";

export { useTabs };
export type {
  TabsContentProps,
  TabsContentsProps,
  TabsHighlightItemProps,
  TabsHighlightProps,
  TabsListProps,
  TabsProps,
  TabsTriggerProps,
};

export function Tabs(props: TabsProps) {
  return <TabsPrimitive data-slot="tabs" {...props} />;
}

export function TabsList({ className, ...props }: TabsListProps) {
  return <TabsListPrimitive data-slot="tabs-list" className={className} {...props} />;
}

export function TabsTrigger({ className, ...props }: TabsTriggerProps) {
  return (
    <TabsTriggerPrimitive
      data-slot="tabs-trigger"
      className={cn(
        "inline-flex min-h-11 items-center justify-center rounded-md px-3 text-xs font-medium text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

export function TabsHighlight(props: TabsHighlightProps) {
  return <TabsHighlightPrimitive data-slot="tabs-highlight" {...props} />;
}

export function TabsHighlightItem(props: TabsHighlightItemProps) {
  return <TabsHighlightItemPrimitive {...props} />;
}

export function TabsContents({ className, ...props }: TabsContentsProps) {
  return (
    <TabsContentsPrimitive
      data-slot="tabs-contents"
      className={cn("min-h-0 flex-1", className)}
      {...props}
    />
  );
}

export function TabsContent({ className, ...props }: TabsContentProps) {
  return (
    <TabsContentPrimitive
      data-slot="tabs-content"
      className={cn("h-full min-h-0 overflow-y-auto", className)}
      {...props}
    />
  );
}
