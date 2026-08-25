import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, render, screen, fireEvent, waitFor, renderHook } from "@testing-library/react";
import { LocaleProvider, useLocale } from "../../src/hooks/useLocale";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
const mockInvoke = vi.mocked(invoke);

function LocaleReader() {
  const { locale, setLocale } = useLocale();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <button data-testid="switch" onClick={() => setLocale("zh-CN")}>
        switch
      </button>
    </div>
  );
}

describe("useLocale", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_ui_language") return null;
      if (cmd === "set_ui_language") return undefined;
      return null;
    });
  });
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("exposes the initial locale from provider", () => {
    render(
      <LocaleProvider initial="en-US">
        <LocaleReader />
      </LocaleProvider>
    );
    expect(screen.getByTestId("locale").textContent).toBe("en-US");
  });

  it("updates locale in-memory when setLocale succeeds", async () => {
    render(
      <LocaleProvider initial="en-US">
        <LocaleReader />
      </LocaleProvider>
    );
    fireEvent.click(screen.getByTestId("switch"));
    await waitFor(() => {
      expect(screen.getByTestId("locale").textContent).toBe("zh-CN");
    });
  });

  it("calls invoke set_ui_language with the new locale", async () => {
    render(
      <LocaleProvider initial="en-US">
        <LocaleReader />
      </LocaleProvider>
    );
    fireEvent.click(screen.getByTestId("switch"));
    await waitFor(() => {
      expect(screen.getByTestId("locale").textContent).toBe("zh-CN");
    });
    expect(mockInvoke).toHaveBeenCalledWith("set_ui_language", { language: "zh-CN" });
  });

  it("rejects unsupported locales without invoking", async () => {
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <LocaleProvider initial="en-US">{children}</LocaleProvider>
    );
    const { result } = renderHook(() => useLocale(), { wrapper });
    await expect(result.current.setLocale("xx-XX" as any)).rejects.toThrow(/Unsupported locale/);
    const setCalls = mockInvoke.mock.calls.filter((c) => c[0] === "set_ui_language");
    expect(setCalls.length).toBe(0);
  });

  it("propagates set_ui_language failure to caller", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "set_ui_language") throw new Error("disk full");
      return null;
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <LocaleProvider initial="en-US">{children}</LocaleProvider>
    );
    const { result } = renderHook(() => useLocale(), { wrapper });
    await expect(result.current.setLocale("zh-CN")).rejects.toThrow("disk full");
  });
});
