import numpy as np

def load(p):
    return np.fromfile(p, dtype=np.float32)

clean = load("fixtures/watchshop_60s_16k.f32")
dev = load("fixtures/device_60s_16k.f32")
n = min(len(clean), len(dev))
clean, dev = clean[:n], dev[:n]

# Cross-correlate the device capture against the clean source. If the same signal
# reached the app twice - once through system audio, once through the air into the
# microphone - there are two peaks, separated by the acoustic travel time.
N = 1 << (2 * n - 1).bit_length()
xc = np.fft.irfft(np.fft.rfft(dev, N) * np.conj(np.fft.rfft(clean, N)), N)
xc = xc[: 16000 * 3]                      # search the first 3 seconds of lag
xc /= np.abs(xc).max()

peak = int(np.argmax(xc))
print(f"strongest alignment at {peak/16.0:.1f} ms  (score {xc[peak]:.3f})")

# Look for further copies within 500 ms after the first arrival.
window = xc[peak : peak + 8000]
order = np.argsort(window)[::-1]
shown = 0
print("further copies after it:")
for i in order:
    if i == 0 or shown >= 4:
        continue
    if window[i] < 0.15:
        break
    if any(abs(int(i) - s) < 160 for s in getattr(load, "seen", [])):
        continue
    print(f"   +{i/16.0:7.1f} ms   relative strength {window[i]/window[0]:.2f}")
    shown += 1

def band(x, lo, hi):
    f = np.fft.rfft(x * np.hanning(len(x)))
    fr = np.fft.rfftfreq(len(x), 1 / 16000)
    return float(np.sum(np.abs(f[(fr >= lo) & (fr < hi)]) ** 2))

print("\nband energy, device relative to clean (dB):")
for lo, hi in [(80, 300), (300, 1000), (1000, 3000), (3000, 6000), (6000, 8000)]:
    d, c = band(dev, lo, hi), band(clean, lo, hi)
    print(f"  {lo:5d}-{hi:5d} Hz  {10*np.log10(d/c):+6.1f}")
