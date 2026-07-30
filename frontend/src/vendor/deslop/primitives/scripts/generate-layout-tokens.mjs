import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const sourcePath = `${root}/layout.json`;
const outputPaths = {
  css: `${root}/layout.css`,
  js: `${root}/layout.js`,
};
const checkOnly = process.argv.includes("--check");
const referencePattern = /^\{(spacing|radius)\.([^}]+)\}$/;
const dimensionPattern = /^\d+(?:\.\d+)?px$/;
const namePattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

const sourceTokens = JSON.parse(await readFile(sourcePath, "utf8"));
const groupNames = [
  "spacing",
  "radius",
  "semanticSpacing",
];

for (const groupName of groupNames) {
  const group = sourceTokens[groupName];

  if (!group || Array.isArray(group) || typeof group !== "object") {
    throw new Error(`layout.json is missing the ${groupName} object`);
  }

  for (const [name, value] of Object.entries(group)) {
    if (!namePattern.test(name)) {
      throw new Error(`Invalid token name: ${groupName}.${name}`);
    }

    if (typeof value !== "string") {
      throw new Error(`${groupName}.${name} must be a string`);
    }

    const reference = referencePattern.exec(value);
    if (reference) {
      const [, targetGroup, targetName] = reference;
      if (!sourceTokens[targetGroup]?.[targetName]) {
        throw new Error(
          `${groupName}.${name} references missing ${targetGroup}.${targetName}`,
        );
      }
    } else if (!dimensionPattern.test(value)) {
      throw new Error(`${groupName}.${name} must be a px value or reference`);
    }
  }
}

const resolveValue = (value) => {
  const reference = referencePattern.exec(value);
  if (!reference) return value;

  const [, targetGroup, targetName] = reference;
  return sourceTokens[targetGroup][targetName];
};

const toPixels = (group) =>
  Object.fromEntries(
    Object.entries(group).map(([name, value]) => [
      name,
      Number.parseFloat(resolveValue(value)),
    ]),
  );

const referenceToCss = (value) => {
  const reference = referencePattern.exec(value);
  if (!reference) return value;

  const [, group, name] = reference;
  const prefix = group === "spacing" ? "space" : "radius";
  return `var(--ui-${prefix}-${name})`;
};

const cssDeclarations = [
  ...Object.entries(sourceTokens.spacing).map(
    ([name, value]) => `    --ui-space-${name}: ${value};`,
  ),
  ...Object.entries(sourceTokens.radius).map(
    ([name, value]) => `    --ui-radius-${name}: ${value};`,
  ),
  ...Object.entries(sourceTokens.semanticSpacing).map(
    ([name, value]) => `    --ui-layout-${name}: ${referenceToCss(value)};`,
  ),
];

const cssOutput = [
  "/* Generated from layout.json. Do not edit directly. */",
  "",
  ":root {",
  ...cssDeclarations,
  "}",
  "",
].join("\n");

const resolvedTokens = Object.fromEntries(
  groupNames.map((groupName) => [
    groupName,
    Object.fromEntries(
      Object.entries(sourceTokens[groupName]).map(([name, value]) => [
        name,
        resolveValue(value),
      ]),
    ),
  ]),
);

const jsDeclarations = [
  ["spacingTokens", resolvedTokens.spacing],
  ["radiusTokens", resolvedTokens.radius],
  ["semanticSpacingTokens", resolvedTokens.semanticSpacing],
  ["spacingPixels", toPixels(sourceTokens.spacing)],
  ["radiusPixels", toPixels(sourceTokens.radius)],
  ["semanticSpacingPixels", toPixels(sourceTokens.semanticSpacing)],
];

const jsOutput = [
  "/* Generated from layout.json. Do not edit directly. */",
  "",
  ...jsDeclarations.flatMap(([name, value]) => [
    `export const ${name} = Object.freeze(${JSON.stringify(value, null, 4)})`,
    "",
  ]),
  "export const layoutTokens = Object.freeze({",
  "    spacing: spacingTokens,",
  "    radius: radiusTokens,",
  "    semanticSpacing: semanticSpacingTokens,",
  "})",
  "",
].join("\n");

const outputs = {
  css: cssOutput,
  js: jsOutput,
};

if (checkOnly) {
  for (const [type, outputPath] of Object.entries(outputPaths)) {
    const current = await readFile(outputPath, "utf8");
    if (current !== outputs[type]) {
      throw new Error(`layout.${type} is out of sync with layout.json`);
    }
  }
} else {
  await Promise.all(
    Object.entries(outputPaths).map(([type, outputPath]) =>
      writeFile(outputPath, outputs[type]),
    ),
  );
}
