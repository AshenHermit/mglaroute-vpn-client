import AppShell from "./app-shell/AppShell";
import { NoticeToaster } from "./components/notice-toaster";
import { Toaster } from "./components/ui/sonner";
import { useUpdateCheck } from "./features/updates/use-update-check";
import { useBackendEvents } from "./stores/app.store";

export default function App() {
  useBackendEvents();
  useUpdateCheck();
  return (
    <>
      <AppShell />
      <Toaster />
      <NoticeToaster />
    </>
  );
}
