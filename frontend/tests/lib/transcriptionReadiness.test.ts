import { describe, expect, test } from "bun:test";
import {
  isConfiguredTranscriptionModelReady,
  type InvokeFn,
} from "../../src/lib/transcriptionReadiness";

// Build a fake Tauri `invoke` from a { command: value | fn } map, recording call order.
// Unknown commands throw so a test fails loudly if the wrong engine is queried.
function fakeInvoke(handlers: Record<string, unknown>): { invoke: InvokeFn; calls: string[] } {
  const calls: string[] = [];
  const invoke = (async (cmd: string, args?: Record<string, unknown>) => {
    calls.push(cmd);
    if (!(cmd in handlers)) throw new Error(`unexpected command: ${cmd}`);
    const h = handlers[cmd];
    return typeof h === "function" ? (h as (a?: unknown) => unknown)(args) : h;
  }) as InvokeFn;
  return { invoke, calls };
}

describe("isConfiguredTranscriptionModelReady (provider-aware recording gate)", () => {
  test("gigaam: ready only when the model is loaded", async () => {
    const loaded = fakeInvoke({
      api_get_transcript_config: { provider: "gigaam" },
      gigaam_status: { model_present: true, loaded: true },
    });
    expect(await isConfiguredTranscriptionModelReady(loaded.invoke)).toBe(true);
    expect(loaded.calls).toContain("gigaam_status");
    // Regression guard: GigaAM must NOT be gated on the Parakeet check (the original bug).
    expect(loaded.calls).not.toContain("parakeet_has_available_models");

    const notLoaded = fakeInvoke({
      api_get_transcript_config: { provider: "gigaam" },
      gigaam_status: { model_present: true, loaded: false },
    });
    expect(await isConfiguredTranscriptionModelReady(notLoaded.invoke)).toBe(false);
  });

  test("parakeet: gated on parakeet_has_available_models", async () => {
    const ready = fakeInvoke({
      api_get_transcript_config: { provider: "parakeet" },
      parakeet_init: null,
      parakeet_has_available_models: true,
    });
    expect(await isConfiguredTranscriptionModelReady(ready.invoke)).toBe(true);
    expect(ready.calls).toContain("parakeet_has_available_models");

    const missing = fakeInvoke({
      api_get_transcript_config: { provider: "parakeet" },
      parakeet_init: null,
      parakeet_has_available_models: false,
    });
    expect(await isConfiguredTranscriptionModelReady(missing.invoke)).toBe(false);
  });

  test("localWhisper: gated on whisper_has_available_models", async () => {
    const f = fakeInvoke({
      api_get_transcript_config: { provider: "localWhisper" },
      whisper_has_available_models: false,
    });
    expect(await isConfiguredTranscriptionModelReady(f.invoke)).toBe(false);
    expect(f.calls).toContain("whisper_has_available_models");
    expect(f.calls).not.toContain("parakeet_has_available_models");
  });

  test("salutespeech: ready per backend salutespeech_is_configured (key OR managed gateway)", async () => {
    const configured = fakeInvoke({
      api_get_transcript_config: { provider: "salutespeech" },
      salutespeech_is_configured: true,
    });
    expect(await isConfiguredTranscriptionModelReady(configured.invoke)).toBe(true);
    expect(configured.calls).toContain("salutespeech_is_configured");
    // Managed build: must NOT gate on a local auth key, nor any local-engine check.
    expect(configured.calls).not.toContain("get_app_settings");
    expect(configured.calls).not.toContain("gigaam_status");
    expect(configured.calls).not.toContain("parakeet_has_available_models");

    const unavailable = fakeInvoke({
      api_get_transcript_config: { provider: "salutespeech" },
      salutespeech_is_configured: false,
    });
    expect(await isConfiguredTranscriptionModelReady(unavailable.invoke)).toBe(false);
  });

  test("cloud provider (openai): always ready, no local-model query", async () => {
    const f = fakeInvoke({ api_get_transcript_config: { provider: "openai" } });
    expect(await isConfiguredTranscriptionModelReady(f.invoke)).toBe(true);
    expect(f.calls).toEqual(["api_get_transcript_config"]);
  });

  test("unreadable config: falls back to parakeet", async () => {
    const f = fakeInvoke({
      api_get_transcript_config: () => {
        throw new Error("no config");
      },
      parakeet_init: null,
      parakeet_has_available_models: true,
    });
    expect(await isConfiguredTranscriptionModelReady(f.invoke)).toBe(true);
    expect(f.calls).toContain("parakeet_has_available_models");
  });
});
