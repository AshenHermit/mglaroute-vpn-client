import AppShell from "./app-shell/AppShell";
import { NoticeToaster } from "./components/notice-toaster";
import { Toaster } from "./components/ui/sonner";
import { useBackendEvents } from "./stores/app.store";

export default function App() {
  useBackendEvents();
  return (
    <>
      <AppShell />
      <Toaster />
      <NoticeToaster />
    </>
  );
}
