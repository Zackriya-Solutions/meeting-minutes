// Polyfill browser APIs jsdom does not implement but our UI primitives rely on.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof globalThis.ResizeObserver === "undefined") {
  (globalThis as any).ResizeObserver = ResizeObserverStub;
}

if (typeof globalThis.IntersectionObserver === "undefined") {
  (globalThis as any).IntersectionObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() { return []; }
    root = null;
    rootMargin = "";
    thresholds = [];
  };
}

if (typeof window !== "undefined" && typeof window.matchMedia === "undefined") {
  (window as any).matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  });
}

if (typeof Element !== "undefined" && !(Element.prototype as any).scrollIntoView) {
  (Element.prototype as any).scrollIntoView = function () {};
}

if (typeof HTMLElement !== "undefined" && !(HTMLElement.prototype as any).releasePointerCapture) {
  (HTMLElement.prototype as any).releasePointerCapture = function () {};
  (HTMLElement.prototype as any).setPointerCapture = function () {};
  (HTMLElement.prototype as any).hasPointerCapture = function () { return false; };
}

import '@testing-library/jest-dom/vitest';