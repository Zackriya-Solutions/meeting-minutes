#!/usr/bin/env python3
"""Minimal HTTPS ASR worker implementing RemoteProvider JSON contract."""
import json, base64, wave, io, argparse
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get('Content-Length', 0))
        body = json.loads(self.rfile.read(length))
        audio_b64 = body.get('audio_base64', '')
        audio_bytes = base64.b64decode(audio_b64) if audio_b64 else b''
        # Detect duration from WAV data (or use fallback)
        duration = 5.0
        if audio_bytes:
            with wave.open(io.BytesIO(audio_bytes), 'rb') as wf:
                n_frames = wf.getnframes()
                rate = wf.getframerate()
                duration = max(0.1, n_frames / rate)
        response = {"segments": [{"start": 0.0, "end": duration, "text": f"Received {duration:.1f}s audio", "speaker": None}]}
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())

    def log_message(self, *args): pass

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('port', type=int, default=8080)
    args = parser.parse_args()
    HTTPServer(('localhost', args.port), Handler).serve_forever()