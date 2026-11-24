import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import type { ZodType } from "@mglaroute/contracts";

/**
 * Invoke a Tauri command and validate its payload against a shared contract.
 * Errors name only the command: payloads may carry subscription URLs or secrets.
 */
export async function invoke<T>(
  command: string,
  schema: ZodType<T>,
  args?: InvokeArgs,
): Promise<T> {
  let result: unknown;
  try {
    result = await tauriInvoke(command, args);
  } catch {
    throw new Error(`Tauri command "${command}" failed`);
  }

  const parsed = schema.safeParse(result);
  if (!parsed.success) {
    throw new Error(`Tauri command "${command}" returned an invalid response`);
  }
  return parsed.data;
}
