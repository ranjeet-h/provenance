import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// jsdom does not implement scrollTo; TanStack Router calls it on navigation.
window.scrollTo = vi.fn();
