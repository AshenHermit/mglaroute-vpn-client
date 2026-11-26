import * as React from "react";

export type ToastVariant = "info" | "success" | "error" | "loading";

export interface ToasterProps extends React.HTMLAttributes<HTMLDivElement> {
  position?: "top-left" | "top-center" | "top-right" | "bottom-left" | "bottom-center" | "bottom-right";
  offset?: number | string;
  theme?: "light" | "dark" | "system";
  icons?: Record<string, React.ReactNode>;
}

export declare function Toaster(props: ToasterProps): React.JSX.Element;

export interface ToastOptions {
  variant?: ToastVariant;
  description?: string;
  duration?: number;
}

export interface ToastFunction {
  (message: React.ReactNode, options?: ToastOptions): number;
  info(message: React.ReactNode, options?: ToastOptions): number;
  success(message: React.ReactNode, options?: ToastOptions): number;
  error(message: React.ReactNode, options?: ToastOptions): number;
  loading(message: React.ReactNode, options?: ToastOptions): number;
  dismiss(id: number): void;
}

export declare const toast: ToastFunction;
