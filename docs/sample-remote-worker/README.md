# Sample Remote ASR Worker

A minimal, vendor-neutral HTTP worker implementing the RemoteProvider JSON contract.

## Run

```bash
# Terminal 1: start the worker
python handler.py 8080

# Terminal 2: run smoke test
python smoke_test.py 8080
```

The worker listens on `POST /transcribe` and returns `{"segments": [{"start", "end", "text", "speaker"?}]}`.