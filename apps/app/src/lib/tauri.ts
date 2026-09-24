// Thin Tauri invoke adapter so UI stays testable without a WebView.
// Business logic must live in Rust crates; this file only adapts the boundary.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

export type InvokeFn = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

let invokeImpl: InvokeFn = tauriInvoke as InvokeFn;

export function setInvokeImpl(fn: InvokeFn): void {
  invokeImpl = fn;
}

export function getInvokeImpl(): InvokeFn {
  return invokeImpl;
}

export function resetInvokeImpl(): void {
  invokeImpl = tauriInvoke as InvokeFn;
}

export type UnlistenFn = () => void;
export type ListenFn = (
  event: string,
  handler: (e: { payload: unknown }) => void,
) => Promise<UnlistenFn>;

let listenImpl: ListenFn = tauriListen as ListenFn;

export function setListenImpl(fn: ListenFn): void {
  listenImpl = fn;
}

export function getListenImpl(): ListenFn {
  return listenImpl;
}

export function resetListenImpl(): void {
  listenImpl = tauriListen as ListenFn;
}

export async function greet(name: string): Promise<string> {
  const result = await invokeImpl("greet", { name });
  if (typeof result !== "string") {
    throw new Error("Unexpected greet response");
  }
  return result;
}
