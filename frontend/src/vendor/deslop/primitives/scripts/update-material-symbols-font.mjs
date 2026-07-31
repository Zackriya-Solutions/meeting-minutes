import { readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const root = fileURLToPath(new URL("../", import.meta.url));
const config = JSON.parse(await readFile(`${root}/material-symbols.json`, "utf8"));
const approvedNames = [...new Set(config.names)].sort();
const names = approvedNames.join(",");
const family = "Material Symbols Rounded:opsz,wght,FILL,GRAD@20..48,100..700,0..1,-50..200";
const cssUrl = new URL("https://fonts.googleapis.com/css2");
cssUrl.searchParams.set("family", family);
cssUrl.searchParams.set("icon_names", names);
cssUrl.searchParams.set("display", "block");

const cssResponse = await fetch(cssUrl, {
  headers: {
    "user-agent": "Mozilla/5.0 AppleWebKit/537.36 Chrome/150 Safari/537.36",
  },
});
if (!cssResponse.ok) {
  throw new Error(`Google Fonts CSS request failed: ${cssResponse.status}`);
}

const css = await cssResponse.text();
const fontUrl = css.match(/src:\s*url\((https:\/\/fonts\.gstatic\.com\/[^)]+)\)/)?.[1];
if (!fontUrl) throw new Error("Google Fonts CSS did not contain a WOFF2 font URL");

const [fontResponse, licenseResponse] = await Promise.all([
  fetch(fontUrl),
  fetch("https://raw.githubusercontent.com/google/material-design-icons/master/LICENSE"),
]);
if (!fontResponse.ok) throw new Error(`Material Symbols font request failed: ${fontResponse.status}`);
if (!licenseResponse.ok) throw new Error(`Material Symbols license request failed: ${licenseResponse.status}`);

const font = Buffer.from(await fontResponse.arrayBuffer());
if (font.subarray(0, 4).toString("ascii") !== "wOF2") {
  throw new Error("Downloaded Material Symbols asset is not a WOFF2 font");
}

const digest = (value) => createHash("sha256").update(value).digest("hex");
const manifest = {
  family: config.family,
  axes: {
    fill: [0, 1],
    grade: [-50, 200],
    opticalSize: [20, 48],
    weight: [100, 700],
  },
  names: approvedNames.length,
  registryHash: digest(JSON.stringify(approvedNames)),
  fontHash: digest(font),
  byteLength: font.length,
};

await Promise.all([
  writeFile(`${root}/fonts/MaterialSymbolsRounded-Variable.woff2`, font),
  writeFile(`${root}/MATERIAL_SYMBOLS_LICENSE.txt`, await licenseResponse.text()),
  writeFile(
    `${root}/material-symbols-font.json`,
    `${JSON.stringify(manifest, null, 2)}\n`,
  ),
]);

await rm(`${root}/fonts/MaterialSymbolsOutlined-Variable.woff2`, { force: true });

console.log(`Updated Material Symbols subset: ${config.names.length} names, ${font.length} bytes.`);
