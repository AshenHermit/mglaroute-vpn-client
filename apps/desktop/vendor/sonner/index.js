import React, { useSyncExternalStore } from "react";

let items = [];
let nextId = 0;
const listeners = new Set();

const emit = () => {
  for (const listener of listeners) listener();
};

const subscribe = (listener) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

const getSnapshot = () => items;

const dismiss = (id) => {
  items = items.filter((item) => item.id !== id);
  emit();
};

const push = (message, variant) => {
  const id = ++nextId;
  items = [...items, { id, message, variant }];
  emit();
  if (typeof setTimeout !== "undefined") {
    setTimeout(() => dismiss(id), 5000);
  }
  return id;
};

const createVariant = (variant) => (message) => push(message, variant);

export const toast = Object.assign(
  (message, options = {}) => push(message, options.variant ?? "info"),
  {
    info: createVariant("info"),
    success: createVariant("success"),
    error: createVariant("error"),
    loading: createVariant("loading"),
    dismiss,
  },
);

const variantColor = {
  info: "var(--normal-text, #f4f4f5)",
  success: "var(--success-color, #4ade80)",
  error: "var(--error-color, #f87171)",
  loading: "var(--normal-text, #f4f4f5)",
};

export function Toaster({
  position = "bottom-right",
  offset = 16,
  style,
  className = "",
  theme: _theme,
  icons: _icons,
  ...props
}) {
  const toasts = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const right = position.includes("right");
  const bottom = position.includes("bottom");

  return React.createElement(
    "div",
    {
      ...props,
      className: `sonner-toaster ${className}`.trim(),
      style: {
        position: "fixed",
        zIndex: 100,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "min(360px, calc(100vw - 24px))",
        right: right ? offset : undefined,
        left: right ? undefined : offset,
        bottom: bottom ? offset : undefined,
        top: bottom ? undefined : offset,
        pointerEvents: "none",
        ...style,
      },
    },
    toasts.map((item) =>
      React.createElement(
        "div",
        {
          key: item.id,
          role: "status",
          tabIndex: 0,
          style: {
            pointerEvents: "auto",
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "12px 14px",
            borderRadius: "var(--border-radius, 12px)",
            border: "1px solid var(--normal-border, rgba(255,255,255,0.12))",
            background: "var(--normal-bg, rgba(24,24,24,0.96))",
            color: "var(--normal-text, #f4f4f5)",
            boxShadow: "0 10px 30px rgba(0,0,0,0.45)",
            fontSize: 14,
            lineHeight: 1.4,
          },
        },
        React.createElement(
          "span",
          {
            "aria-hidden": "true",
            style: { color: variantColor[item.variant] ?? variantColor.info, flexShrink: 0 },
          },
          item.variant === "success" ? "✓" : item.variant === "error" ? "✕" : "•",
        ),
        React.createElement("span", { style: { minWidth: 0, overflowWrap: "anywhere" } }, item.message),
        React.createElement(
          "button",
          {
            type: "button",
            "aria-label": "Dismiss notification",
            onClick: () => dismiss(item.id),
            style: {
              marginLeft: "auto",
              flexShrink: 0,
              background: "transparent",
              border: 0,
              color: "inherit",
              cursor: "pointer",
              fontSize: 16,
              lineHeight: 1,
              padding: 4,
            },
          },
          "×",
        ),
      ),
    ),
  );
}
