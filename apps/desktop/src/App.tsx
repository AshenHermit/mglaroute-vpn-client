import { useState } from "react";
import Dashboard from "./pages/Dashboard";
import Servers from "./pages/Servers";
import ServiceProfiles from "./pages/ServiceProfiles";
import Settings from "./pages/Settings";
import Subscriptions from "./pages/Subscriptions";
import { useAppStore } from "./stores/app.store";

const pages = {
  dashboard: { label: "Dashboard", component: Dashboard },
  subscriptions: { label: "Subscriptions", component: Subscriptions },
  services: { label: "Service profiles", component: ServiceProfiles },
  servers: { label: "Servers", component: Servers },
  settings: { label: "Settings", component: Settings },
} as const;

type Page = keyof typeof pages;

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const notice = useAppStore((store) => store.notice);
  const setNotice = useAppStore((store) => store.setNotice);
  const CurrentPage = pages[page].component;

  return (
    <div className="min-h-screen bg-slate-50 text-slate-900">
      <header className="border-b border-slate-200 bg-white">
        <div className="mx-auto flex max-w-5xl flex-wrap items-center gap-4 px-6 py-4">
          <h1 className="text-lg font-semibold">mglaroute</h1>
          <nav aria-label="Main navigation" className="flex flex-wrap gap-1">
            {(Object.keys(pages) as Page[]).map((key) => (
              <button
                key={key}
                type="button"
                aria-current={page === key ? "page" : undefined}
                onClick={() => setPage(key)}
                className={`rounded-md px-3 py-1.5 text-sm ${
                  page === key
                    ? "bg-sky-700 text-white"
                    : "text-slate-600 hover:bg-slate-100"
                }`}
              >
                {pages[key].label}
              </button>
            ))}
          </nav>
        </div>
      </header>
      <main className="mx-auto grid max-w-5xl gap-4 p-6">
        <CurrentPage />
      </main>
      {notice && (
        <div
          role="status"
          className="fixed bottom-4 right-4 flex max-w-sm items-center gap-3 rounded-md bg-slate-800 px-4 py-3 text-sm text-white shadow-lg"
        >
          <span>{notice}</span>
          <button
            type="button"
            aria-label="Dismiss notice"
            onClick={() => setNotice(null)}
            className="text-slate-300 hover:text-white"
          >
            ×
          </button>
        </div>
      )}
    </div>
  );
}
