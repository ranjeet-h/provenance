import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
import { setListenImpl } from "./lib/tauri";

// jsdom does not implement scrollTo; TanStack Router calls it on navigation.
window.scrollTo = vi.fn();

class TestResizeObserver implements ResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}

  observe(target: Element): void {
    this.callback(
      [
        {
          target,
          contentRect: target.getBoundingClientRect(),
        } as ResizeObserverEntry,
      ],
      this,
    );
  }

  unobserve(): void {}
  disconnect(): void {}
}

window.ResizeObserver = TestResizeObserver;
setListenImpl(async () => () => {});
