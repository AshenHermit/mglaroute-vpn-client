import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import type { ZodType } from "@mglaroute/contracts";

/** Safe, secret-free codes mirrored from Rust `CommandError`. */
const SAFE_CODES = [
  "busy",
  "not_found",
  "invalid_input",
  "storage",
  "fetch",
  "core",
  "proxy",
  "probe",
  "unsupported",
] as const;

export type CommandErrorCode = (typeof SAFE_CODES)[number];

export class CommandInvocationError extends Error {
  readonly code: CommandErrorCode | null;

  constructor(command: string, code: CommandErrorCode | null) {
    super(`Tauri command "${command}" failed`);
    this.name = "CommandInvocationError";
    this.code = code;
  }
}

/** Extract only whitelisted Rust error codes; never forward native error text. */
export function commandErrorCode(error: unknown): CommandErrorCode | null {
  if (error instanceof CommandInvocationError) return error.code;
  if (typeof error === "string" && SAFE_CODES.includes(error as CommandErrorCode)) {
    return error as CommandErrorCode;
  }
  if (error && typeof error === "object") {
    for (const code of SAFE_CODES) {
      if (code in error && (error as Record<string, unknown>)[code] === null) {
        return code;
      }
    }
  }
  return null;
}

/**
 * Invoke a Tauri command and validate its payload against a shared contract.
 * Errors name only the command and a whitelisted code: payloads may carry
 * subscription URLs or secrets.
 */
export async function invoke<T>(
  command: string,
  schema: ZodType<T>,
  args?: InvokeArgs,
): Promise<T> {
  let result: unknown;
  try {
    result = await tauriInvoke(command, args);
  } catch (error) {
    throw new CommandInvocationError(command, commandErrorCode(error));
  }

  const parsed = schema.safeParse(result);
  if (!parsed.success) {
    throw new CommandInvocationError(command, null);
  }
  return parsed.data;
}
