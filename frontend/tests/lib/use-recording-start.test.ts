/// <reference path="../bun-test.d.ts" />
import { describe, expect, mock, test } from "bun:test";
import { checkRecordingTranscriptionReady } from "../../src/hooks/useRecordingStart";

describe("recording start readiness", () => {
	test("uses backend selected-model validation for parakeet configs", async () => {
		const commands: string[] = [];
		const invokeFn = mock(async (command: string) => {
			commands.push(command);
			if (command === "api_get_transcript_config") {
				return { provider: "parakeet", model: "parakeet-ctc-es-0.6b-int8" };
			}
			if (command === "parakeet_init") {
				return null;
			}
			if (command === "parakeet_validate_model_ready") {
				return "parakeet-ctc-es-0.6b-int8";
			}
			throw new Error(`unexpected command: ${command}`);
		});

		const result = await checkRecordingTranscriptionReady(invokeFn as any);

		expect(result).toEqual({ ready: true, downloading: false });
		expect(commands).toEqual([
			"api_get_transcript_config",
			"parakeet_init",
			"parakeet_validate_model_ready",
		]);
	});

	test("preserves non-parakeet path without requiring parakeet readiness", async () => {
		const commands: string[] = [];
		const invokeFn = mock(async (command: string) => {
			commands.push(command);
			if (command === "api_get_transcript_config") {
				return { provider: "localWhisper", model: "large-v3-turbo" };
			}
			throw new Error(`unexpected command: ${command}`);
		});

		const result = await checkRecordingTranscriptionReady(invokeFn as any);

		expect(result).toEqual({ ready: true, downloading: false });
		expect(commands).toEqual(["api_get_transcript_config"]);
	});
});
