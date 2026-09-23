import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
import { setListenImpl } from "./lib/tauri";

// jsdom does not implement scrollTo; TanStack Router calls it on navigation.
window.scrollTo = vi.fn();
setListenImpl(async () => () => {});
