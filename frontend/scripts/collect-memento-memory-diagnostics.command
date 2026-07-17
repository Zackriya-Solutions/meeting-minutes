#!/bin/bash

# Privacy-safe macOS memory diagnostics for Memento.
#
# Keep Memento running while its memory usage is high, then run this file. The archive
# contains process memory maps, stacks, system pressure, code-signing metadata, model file
# sizes, and aggregate indexing/job counts. It deliberately excludes meeting/transcript
# text, audio, summaries, credentials, browser data, and full database contents.

set -u
umask 077

script_version="1"
timestamp="$(date '+%Y%m%d-%H%M%S')"
user_home="${HOME:?Could not resolve the current user home directory}"
temp_root="${TMPDIR:-/tmp}"
work_dir="$(mktemp -d "$temp_root/memento-memory-diagnostics.XXXXXX")" || exit 1
bundle_dir="$work_dir/memento-memory-diagnostics-$timestamp"
mkdir -p "$bundle_dir"

cleanup() {
  case "$work_dir" in
    "$temp_root"/memento-memory-diagnostics.*)
      rm -rf -- "$work_dir"
      ;;
  esac
}
trap cleanup EXIT INT TERM

output_root="$user_home/Desktop"
if [[ ! -d "$output_root" ]]; then
  output_root="$user_home/Downloads"
fi
archive_path="$output_root/memento-memory-diagnostics-$timestamp.zip"

echo "Memento memory diagnostics"
echo "Keep Memento open. Collection takes about 30 seconds."
echo "No meeting text, audio, summaries, or credentials will be collected."
echo ""

candidate_pids="$(
  {
    pgrep -f '/Memento[.]app/Contents/MacOS/(Memento|memento)' 2>/dev/null || true
    pgrep -x Memento 2>/dev/null || true
    pgrep -x memento 2>/dev/null || true
  } | sort -u
)"

selected_pid=""
largest_rss=0
for candidate_pid in $candidate_pids; do
  rss="$(ps -o rss= -p "$candidate_pid" 2>/dev/null | tr -d '[:space:]')"
  if [[ "$rss" =~ ^[0-9]+$ ]] && (( rss > largest_rss )); then
    selected_pid="$candidate_pid"
    largest_rss="$rss"
  fi
done

{
  echo "schema_version=$script_version"
  echo "collected_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  echo "process_found=$([[ -n "$selected_pid" ]] && echo yes || echo no)"
  echo "process_pid=${selected_pid:-none}"
  echo "process_initial_rss_kb=$largest_rss"
  echo "architecture=$(uname -m)"
  echo "kernel=$(uname -r)"
  sw_vers
  sysctl hw.memsize hw.logicalcpu vm.swapusage 2>&1
} > "$bundle_dir/system-summary.txt"

memory_pressure > "$bundle_dir/memory-pressure.txt" 2>&1 || true
vm_stat > "$bundle_dir/vm-stat.txt" 2>&1 || true
ps -axo pid=,ppid=,%cpu=,rss=,vsz=,state=,etime=,comm= \
  | sort -k4 -nr \
  | head -40 > "$bundle_dir/top-processes-by-rss.txt"

if [[ -n "$selected_pid" ]]; then
  echo "Found Memento PID $selected_pid (RSS $((largest_rss / 1024)) MB)."

  # comm reports the executable path without argv, so deep-link or opened-file
  # arguments cannot leak into the diagnostic archive.
  ps -p "$selected_pid" -o pid=,ppid=,user=,%cpu=,%mem=,rss=,vsz=,state=,etime=,comm= \
    > "$bundle_dir/memento-process.txt" 2>&1 || true

  executable_path="$(ps -p "$selected_pid" -o comm= 2>/dev/null | sed 's/^[[:space:]]*//')"
  app_bundle=""
  case "$executable_path" in
    *.app/Contents/MacOS/*)
      app_bundle="${executable_path%%.app/*}.app"
      ;;
  esac

  if [[ -n "$app_bundle" && -d "$app_bundle" ]]; then
    {
      echo "bundle_path=${app_bundle/$user_home/<HOME>}"
      defaults read "$app_bundle/Contents/Info.plist" CFBundleIdentifier 2>&1
      defaults read "$app_bundle/Contents/Info.plist" CFBundleShortVersionString 2>&1
      codesign -dv --verbose=4 "$app_bundle" 2>&1 \
        | grep -E '^(Identifier|Format|CodeDirectory|Signature|Authority|TeamIdentifier|Runtime Version)='
      spctl -a -t exec -vv "$app_bundle" 2>&1
      xcrun stapler validate "$app_bundle" 2>&1
    } > "$bundle_dir/app-identity.txt"
  fi

  echo "Capturing a 7-second stack sample..."
  sample "$selected_pid" 7 1 -file "$bundle_dir/memento-sample.txt" \
    > "$bundle_dir/sample-command.txt" 2>&1 || true

  echo "Capturing virtual-memory maps..."
  vmmap -summary "$selected_pid" > "$bundle_dir/memento-vmmap-summary.txt" 2>&1 || true
  vmmap "$selected_pid" > "$bundle_dir/memento-vmmap.txt" 2>&1 || true
  footprint "$selected_pid" > "$bundle_dir/memento-footprint.txt" 2>&1 || true

  echo "Capturing a 20-second memory trend..."
  echo "timestamp_utc,rss_kb,vsz_kb,cpu_percent,state,elapsed" \
    > "$bundle_dir/memento-memory-trend.csv"
  for _ in $(seq 1 10); do
    if ! kill -0 "$selected_pid" 2>/dev/null; then
      break
    fi
    process_row="$(ps -p "$selected_pid" -o rss=,vsz=,%cpu=,state=,etime= 2>/dev/null \
      | awk '{$1=$1; print}' | tr ' ' ',')"
    echo "$(date -u '+%Y-%m-%dT%H:%M:%SZ'),$process_row" \
      >> "$bundle_dir/memento-memory-trend.csv"
    sleep 2
  done

else
  echo "Memento process was not found." | tee "$bundle_dir/process-not-found.txt"
fi

app_data_dir=""
for candidate_dir in \
  "$user_home/Library/Application Support/com.meetily.ai" \
  "$user_home/Library/Application Support/Meetily" \
  "$user_home/Library/Application Support/Memento"; do
  if [[ -d "$candidate_dir" ]]; then
    app_data_dir="$candidate_dir"
    break
  fi
done

if [[ -n "$app_data_dir" ]]; then
  model_dir="$app_data_dir/models/embedding"
  if [[ -d "$model_dir" ]]; then
    find "$model_dir" -maxdepth 2 -type f -exec stat -f '%N|%z bytes' {} \; 2>/dev/null \
      | sed "s|^$model_dir/||" \
      | sort > "$bundle_dir/embedding-model-files.txt"
  fi

  database_path="$app_data_dir/meeting_minutes.sqlite"
  if [[ -f "$database_path" ]] && command -v sqlite3 >/dev/null 2>&1; then
    # immutable=1 reads the last checkpointed snapshot without creating WAL/SHM files or
    # taking a write lock in the live application directory.
    sqlite3 "file:$database_path?immutable=1" > "$bundle_dir/indexing-aggregates.txt" 2>&1 <<'SQL'
.headers on
.mode column
SELECT 'selected_model' AS metric, value
FROM app_settings_kv WHERE key='embedding.model';

SELECT embedding_status, COUNT(*) AS chunks
FROM chunks GROUP BY embedding_status ORDER BY embedding_status;

SELECT COUNT(*) AS chunks_total,
       COUNT(DISTINCT meeting_id) AS meetings_with_chunks,
       MIN(token_count) AS min_tokens,
       MAX(token_count) AS max_tokens,
       CAST(AVG(token_count) AS INTEGER) AS avg_tokens
FROM chunks;

SELECT kind, status, attempts, COUNT(*) AS jobs,
       MIN(updated_at) AS oldest_update,
       MAX(updated_at) AS newest_update
FROM jobs
WHERE kind IN ('chunk_embed', 'embedding_repair', 'backfill')
GROUP BY kind, status, attempts
ORDER BY kind, status, attempts;
SQL
  fi
fi

# Redact the current home path and its user-controlled suffix from every textual
# diagnostic after all tools finish. Stack/memory tools report mappings and symbols,
# not heap contents, but their path columns can still contain private filenames.
for diagnostic_file in "$bundle_dir"/*.txt "$bundle_dir"/*.csv; do
  [[ -f "$diagnostic_file" ]] || continue
  sed "s|$user_home|<HOME>|g" "$diagnostic_file" \
    | sed -E 's#<HOME>/.*#<HOME>/<REDACTED_PATH>#g' \
    > "$diagnostic_file.redacted"
  mv "$diagnostic_file.redacted" "$diagnostic_file"
done

ditto -c -k --norsrc --keepParent "$bundle_dir" "$archive_path"

echo ""
echo "Diagnostics saved to:"
echo "$archive_path"
echo "Please send that ZIP file for analysis."
open -R "$archive_path" >/dev/null 2>&1 || true
