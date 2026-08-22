/// <reference path="../bun-test.d.ts" />
import { describe, expect, test } from "bun:test";
import { shouldAutoScrollOnTranscriptChange } from "../../src/contexts/TranscriptContext";

describe("TranscriptContext auto-scroll gating", () => {
	test("does not auto-scroll empty-state transcript panel on initial load", () => {
		expect(shouldAutoScrollOnTranscriptChange(0, true)).toBe(false);
	});

	test("auto-scrolls when transcript content exists and user was at bottom", () => {
		expect(shouldAutoScrollOnTranscriptChange(3, true)).toBe(true);
	});

	test("does not auto-scroll when user scrolled away from bottom", () => {
		expect(shouldAutoScrollOnTranscriptChange(3, false)).toBe(false);
	});
});
