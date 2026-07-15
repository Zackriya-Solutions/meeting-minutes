#!/usr/bin/env node
// VALUEOS: emit a single Tauri `--config` overlay for CI = branding + CI-only overrides.
// Combines valueos/branding/tauri.valueos.json (the rename) with
// bundle.createUpdaterArtifacts:false (so the unsigned CI build needs no signing key).
// Keeps the branding overlay reusable for local builds (where you may want updater
// artifacts on). Writes to the path given as argv[2] (default: valueos-ci.config.json,
// relative to the current working directory — call it from frontend/).
const fs = require("fs");
const path = require("path");

const overlay = JSON.parse(
  fs.readFileSync(path.join(__dirname, "tauri.valueos.json"), "utf8")
);
delete overlay.$comment;
overlay.bundle = Object.assign({}, overlay.bundle, { createUpdaterArtifacts: false });

const out = process.argv[2] || "valueos-ci.config.json";
fs.writeFileSync(out, JSON.stringify(overlay, null, 2));
console.log("Wrote Tauri config overlay to " + out);
