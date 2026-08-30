/// <reference path="../bun-test.d.ts" />
import { describe, expect, test } from "bun:test";
import {
	MODEL_DISPLAY_CONFIG,
	PARAKEET_MODEL_CONFIGS,
	getParakeetModelSections,
} from "../../src/lib/parakeet";

describe("Parakeet CTC ES beta metadata", () => {
	test("adds beta display metadata for the new model", () => {
		expect(MODEL_DISPLAY_CONFIG["parakeet-ctc-es-0.6b-int8"]).toEqual(
			expect.objectContaining({
				beta: true,
				friendlyName: "Spanish Beta",
			}),
		);

		expect(PARAKEET_MODEL_CONFIGS["parakeet-ctc-es-0.6b-int8"]).toEqual(
			expect.objectContaining({
				quantization: "Int8",
			}),
		);
	});

	test("keeps TDT as the recommended model while surfacing CTC ES separately", () => {
		const sections = getParakeetModelSections([
			{ name: "parakeet-ctc-es-0.6b-int8" },
			{ name: "parakeet-tdt-0.6b-v3-int8" },
			{ name: "parakeet-tdt-0.6b-v2-int8" },
		]);

		expect(sections.recommended?.name).toBe("parakeet-tdt-0.6b-v3-int8");
		expect(sections.others.map((model) => model.name)).toEqual([
			"parakeet-ctc-es-0.6b-int8",
			"parakeet-tdt-0.6b-v2-int8",
		]);
	});
});
