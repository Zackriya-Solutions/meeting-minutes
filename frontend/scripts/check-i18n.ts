// Validates that all locale JSONs are in sync. Reports:
//  (a) any key present in one locale but missing in another
//  (b) any empty string values (a translation in progress would be invisible)
//
// Known limitations:
//  - Does NOT cross-check code usage.
//
//  - Add a key in any locale => all locales must be updated, or this script fails.

import { readFileSync, readdirSync } from "fs";
import { join, resolve } from "path";
import { LOCALES } from "../src/i18n/config";

const FRONTEND_DIR = resolve(__dirname, "..");
const LOCALES_DIR = join(FRONTEND_DIR, "locales");

type KeyTree = { [k: string]: string | KeyTree };

function loadLocale(locale: string): KeyTree {
  const dir = join(LOCALES_DIR, locale);
  const merged: KeyTree = {};
  for (const file of readdirSync(dir)) {
    if (!file.endsWith(".json")) continue;
    const data = JSON.parse(readFileSync(join(dir, file), "utf-8")) as KeyTree;
    merged[file.replace(".json", "")] = data;
  }
  return merged;
}

function flatten(obj: KeyTree, prefix: string, out: string[] = []): string[] {
  for (const [k, v] of Object.entries(obj)) {
    const key = `${prefix}.${k}`;
    if (typeof v === "object" && v !== null) {
      flatten(v, key, out);
    } else {
      out.push(key);
    }
  }
  return out;
}

let errors = 0;
const allKeys = new Map<string, Set<string>>();
for (const locale of LOCALES) {
  allKeys.set(locale, new Set(flatten(loadLocale(locale), "")));
}

const union = new Set<string>();
for (const keys of allKeys.values()) {
  for (const k of keys) union.add(k);
}
for (const k of union) {
  for (const locale of LOCALES) {
    if (!allKeys.get(locale)!.has(k)) {
      console.error(`ERROR: locale "${locale}" missing key "${k}"`);
      errors++;
    }
  }
}

for (const locale of LOCALES) {
  const tree = loadLocale(locale);
  function walk(obj: KeyTree, prefix: string) {
    for (const [k, v] of Object.entries(obj)) {
      const key = `${prefix}.${k}`;
      if (typeof v === "object" && v !== null) walk(v, key);
      else if (typeof v === "string" && v.trim() === "") {
        console.error(`ERROR: locale "${locale}" has empty value for "${key}"`);
        errors++;
      }
    }
  }
  walk(tree, "");
}

if (errors > 0) {
  console.error(`\n${errors} i18n issue(s) found`);
  process.exit(1);
}
console.log("i18n check passed");
