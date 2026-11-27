import { AnimateUIProvider } from "@animate-ui/react/provider";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AnimateUIProvider tempo={0.9}>
      <App />
    </AnimateUIProvider>
  </React.StrictMode>,
);
