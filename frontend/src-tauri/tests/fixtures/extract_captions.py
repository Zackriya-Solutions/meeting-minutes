"""Pull the words YouTube's auto-captions place inside a time window.

json3 carries per-word offsets, so a window can be cut without inheriting a
caption cue that straddles the boundary.

    python extract_captions.py watchshop.en-orig.json3 0 60
"""

import json
import sys


def words_in_window(path, start_ms, end_ms):
    events = json.load(open(path, encoding="utf-8"))["events"]
    out = []
    for event in events:
        base = event.get("tStartMs", 0)
        for seg in event.get("segs", []):
            text = seg.get("utf8", "")
            if not text.strip():
                continue
            at = base + seg.get("tOffsetMs", 0)
            if start_ms <= at < end_ms:
                out.append((at, text.strip()))
    out.sort()
    return out


if __name__ == "__main__":
    path, start_s, end_s = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
    words = words_in_window(path, start_s * 1000, end_s * 1000)
    print(" ".join(w for _, w in words))
    print(f"\n--- {len(words)} words between {start_s}s and {end_s}s ---", file=sys.stderr)
