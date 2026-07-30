import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { typographyStyles } from "../tokens.js";

const root = fileURLToPath(new URL("../", import.meta.url));
const [markdown, css] = await Promise.all([
  readFile(`${root}/TYPOGRAPHY.md`, "utf8"),
  readFile(`${root}/typography.css`, "utf8"),
]);

const errors = [];
const tokenKey = (name) => name.toLowerCase().replaceAll(" ", "-");
const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const normalizeValue = (value) => (value === "0em" ? "0" : value);
const cssVariables = new Map(
  [...css.matchAll(/^\s*(--ui-[a-z0-9-]+):\s*([^;]+);$/gm)].map(
    ([, name, value]) => [name, value.trim()],
  ),
);

for (const style of typographyStyles) {
  const name = tokenKey(style.name);
  const entry = markdown.match(
    new RegExp(`^${escapeRegExp(name)}:\\n((?:  [a-z-]+: .*\\n?)+)`, "m"),
  );

  if (!entry) {
    errors.push(`TYPOGRAPHY.md is missing ${name}`);
    continue;
  }

  const documented = Object.fromEntries(
    [...entry[1].matchAll(/^  ([a-z-]+): (.+)$/gm)].map(([, key, value]) => [
      key,
      value.replace(/^"|"$/g, ""),
    ]),
  );
  const expectedValues = {
    "font-size": style.fontSize,
    "line-height": style.lineHeight,
    "font-weight": String(style.fontWeight),
    "letter-spacing": style.letterSpacing,
  };

  for (const [property, expected] of Object.entries(expectedValues)) {
    if (normalizeValue(documented[property]) !== expected) {
      errors.push(
        `TYPOGRAPHY.md ${name}.${property} is ${documented[property] ?? "missing"}; expected ${expected}`,
      );
    }
  }

  const cssProperties = {
    "font-size": `--ui-${name}-font-size`,
    "line-height": `--ui-${name}-line-height`,
    "font-weight": `--ui-${name}-font-weight`,
    "letter-spacing": `--ui-${name}-letter-spacing`,
  };

  for (const [property, cssToken] of Object.entries(cssProperties)) {
    const actual = cssVariables.get(cssToken);
    const expected = expectedValues[property];
    if (actual !== expected) {
      errors.push(
        `typography.css ${cssToken} is ${actual ?? "missing"}; expected ${expected}`,
      );
    }
  }
}

if (errors.length) {
  console.error("Typography token check failed:\n");
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  console.log("Typography tokens are in sync.");
}
