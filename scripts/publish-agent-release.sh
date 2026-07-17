#!/usr/bin/env bash
# VALUEOS: publish an agent release to ValueOS (VALUEOS_AGENT_API.md §8).
#
# Machine-to-machine — authenticated with the x-api-key (AGENT_API_KEY), NOT a user/agent
# OAuth token. Installers must already be uploaded to the private S3 bucket by CI; this script
# reads the per-platform artifact metadata and POSTs the release to /api/agent/releases.
# ValueOS assigns the version (calendar YYYY.MM.DD.<seq>) and marks it current; releases are
# IMMUTABLE. Once published, the Sales download button + admin Agent Usage tab light up for
# feat_agent tenants.
#
# Env:
#   AGENT_API_KEY  (required)  the x-api-key (CI secret — never commit)
#   API_BASE       (default https://d2luofz0a4v7f3.cloudfront.net)
#   META_DIR       (default meta)  directory of *.json files, one per platform, each:
#                  { "platform", "s3_key", "size_bytes", "checksum", "content_type" }
#   GIT_REF        (optional)  git ref/sha recorded on the release
#   NOTES          (optional)  release notes
#   DRY_RUN        (optional)  when "1", print the request and skip the POST
#
# Requires: curl, jq (preinstalled on GitHub runners).
set -euo pipefail

: "${AGENT_API_KEY:?AGENT_API_KEY is required (the x-api-key)}"
API_BASE="${API_BASE:-https://d2luofz0a4v7f3.cloudfront.net}"
META_DIR="${META_DIR:-meta}"
GIT_REF="${GIT_REF:-}"
NOTES="${NOTES:-}"
DRY_RUN="${DRY_RUN:-0}"

shopt -s nullglob
files=("${META_DIR}"/*.json)
if [ ${#files[@]} -eq 0 ]; then
  echo "::error::no artifact metadata JSON files found in ${META_DIR}" >&2
  exit 1
fi

# Merge the per-platform metadata files into the artifacts[] array, then build the request body
# (dropping empty optional fields).
artifacts="$(jq -s '.' "${files[@]}")"
# jq filter kept in a variable (not inline in $()) so every bash parses the substitution cleanly.
jq_filter='{git_ref: ($git_ref | select(. != "")), notes: ($notes | select(. != "")), artifacts: $artifacts} | with_entries(select(.value != null))'
body="$(jq -n --arg git_ref "${GIT_REF}" --arg notes "${NOTES}" --argjson artifacts "${artifacts}" "${jq_filter}")"

count="$(jq 'length' <<<"${artifacts}")"
echo "Publishing ${count} artifacts to ${API_BASE}/api/agent/releases"
echo "${body}" | jq .

if [ "${DRY_RUN}" = "1" ]; then
  echo "DRY_RUN=1 — not posting."
  exit 0
fi

resp="$(curl -sS -w $'\n%{http_code}' -X POST "${API_BASE}/api/agent/releases" \
  -H "x-api-key: ${AGENT_API_KEY}" \
  -H 'content-type: application/json' \
  --data-binary @- <<<"${body}")"
code="$(printf '%s' "${resp}" | tail -n1)"
payload="$(printf '%s' "${resp}" | sed '$d')"

echo "HTTP ${code}"
printf '%s\n' "${payload}" | jq . 2>/dev/null || printf '%s\n' "${payload}"
if [ "${code}" -lt 200 ] || [ "${code}" -ge 300 ]; then
  echo "::error::publish failed (HTTP ${code})" >&2
  exit 1
fi

version="$(printf '%s' "${payload}" | jq -r '.result.version // .version // "see response"')"
echo "✅ Published. ValueOS assigned version: ${version}"
