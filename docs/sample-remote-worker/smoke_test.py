#!/usr/bin/env python3
"""Smoke test: POST 5s synthetic WAV to worker, assert valid response."""
import json, base64, wave, io, struct, sys, urllib.request

def make_wav(duration_sec=5, sample_rate=16000):
    frames = int(duration_sec * sample_rate)
    buf = io.BytesIO()
    with wave.open(buf, 'wb') as wf:
        wf.setnchannels(1); wf.setsampwidth(2); wf.setframerate(sample_rate)
        data = struct.pack('<' + 'h' * frames, *[(i % 100) - 50 for i in range(frames)])
        wf.writeframes(data)
    return buf.getvalue()

def main(port):
    wav_b64 = base64.b64encode(make_wav()).decode()
    payload = json.dumps({"audio_base64": wav_b64, "model": "test", "language": "en", "min_speakers": 1, "max_speakers": 2}).encode()
    req = urllib.request.Request(f'http://localhost:{port}/transcribe', data=payload, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req) as resp:
        assert resp.status == 200, f"Expected 200, got {resp.status}"
        result = json.loads(resp.read())
    assert 'segments' in result and len(result['segments']) >= 1, "Missing segments"
    seg = result['segments'][0]
    assert 'start' in seg and 'end' in seg and 'text' in seg, "Missing segment fields"
    print(f"OK: {seg['text']} (duration: {seg['end'] - seg['start']}s)")

if __name__ == '__main__':
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 8080)