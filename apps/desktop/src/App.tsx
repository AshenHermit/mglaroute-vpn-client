import AppShell from "./app-shell/AppShell";
import { useBackendEvents } from "./stores/app.store";

export default function App() {
  useBackendEvents();
  return <AppShell />;
}
