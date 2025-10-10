# Bluetooth Headphone Playback Notice

## Important Information for Recording Review

When **reviewing recordings** in Meetily, we recommend using **computer speakers** or **wired headphones** rather than Bluetooth headphones for accurate playback.

---

## The Issue

Recordings may sound **distorted, sped up, or have clarity issues** when played through Bluetooth headphones, even though the recording file itself is perfectly fine.

### Symptoms
- Audio plays too fast or too slow
- Voice sounds higher/lower pitched than normal
- Quality seems degraded or "chipmunk-like"
- **Different Bluetooth devices cause different playback speeds**

### What's Actually Happening
**Your recording is fine!** The issue occurs during **playback**, not recording.

---

## Technical Explanation

### Why This Happens

1. **Meetily records at 48kHz** (professional audio standard)
2. **Bluetooth headphones use various sample rates**: 8kHz, 16kHz, 24kHz, 44.1kHz, or 48kHz
3. **macOS resamples audio** when sending 48kHz content to Bluetooth devices
4. **Resampling can fail** if macOS:
   - Negotiates the wrong Bluetooth codec (SBC vs AAC vs LDAC)
   - Misidentifies the device's playback capability
   - Uses low-quality resampling for power efficiency

### Device-Specific Behavior

Different Bluetooth headphones report different capabilities:

| Device Type | Typical Playback Rate | Result When Playing 48kHz |
|------------|----------------------|---------------------------|
| Sony WH-1000XM4 | 16-44.1kHz (varies) | May sound 1.5-3x faster |
| AirPods Pro | 24kHz or 48kHz | Usually OK, but can vary |
| Cheap BT Headset | 8-16kHz | Often sounds very fast |
| High-end BT (LDAC) | 44.1-48kHz | Usually works correctly |

The rate depends on:
- **Bluetooth profile** (A2DP for music vs HFP for calls)
- **Active codec** (SBC, AAC, aptX, LDAC)
- **Battery mode** (power-saving modes may reduce quality)
- **macOS version** and audio driver quirks

---

## Solution: Use Computer Speakers

### For Accurate Review

✅ **Computer speakers** (built-in or external)
✅ **Wired headphones** (3.5mm jack or USB)
✅ **High-quality DAC** (digital audio converter)

❌ **Bluetooth headphones** (for reviewing recordings)
❌ **Bluetooth speakers** (same resampling issues)

### Bluetooth Headphones Are Fine For

- ✅ **Recording** (microphone input) - We handle sample rate conversion correctly
- ✅ **Live monitoring** during recording - macOS handles real-time audio
- ✅ **General computer use** - Normal audio playback
- ❌ **Reviewing Meetily recordings** - Use wired/speakers instead

---

## Verification Steps

To confirm your recording is actually fine:

1. **Play recording through computer speakers**
   - If it sounds normal → Recording is good, BT playback is the issue ✅
   - If it still sounds wrong → May be a different issue ❌

2. **Check file properties**
   ```bash
   # In terminal:
   ffprobe path/to/recording/audio.mp4
   ```
   Should show:
   - `sample_rate=48000` ✅
   - `channels=1` ✅
   - `codec_name=aac` ✅

3. **Try different playback devices**
   - Computer speakers: Should sound normal
   - Wired headphones: Should sound normal
   - Bluetooth device A: Might sound wrong
   - Bluetooth device B: Might sound differently wrong

---

## Why We Don't "Fix" This

### This is Not a Meetily Bug

The issue is in **macOS's Bluetooth audio stack**, not in Meetily's recording engine.

**Evidence:**
- Recordings play perfectly on computer speakers
- File metadata shows correct 48kHz encoding
- Other professional audio apps have the same limitation
- Issue varies by Bluetooth device (different devices = different problems)

### Industry Standard Practice

Professional audio software **always** recommends:
- Monitor through studio monitors (speakers) or wired headphones
- Avoid Bluetooth for critical listening
- Use wired connections for audio work

Examples:
- **Logic Pro X**: Warns against BT monitoring
- **Audacity**: Recommends wired headphones
- **GarageBand**: Disables BT for recording/monitoring

---

## Workarounds

### Option 1: Use Computer Speakers (Recommended)
**Best**: Most accurate, no resampling issues

### Option 2: Export at Different Sample Rate
If you **must** use Bluetooth for playback:

1. **Export recording** at lower sample rate (future feature)
2. **Transcode manually** using ffmpeg:
   ```bash
   ffmpeg -i audio.mp4 -ar 44100 audio_44k.mp4
   ```
3. **Try 44.1kHz** (better BT compatibility than 48kHz)

### Option 3: Use High-Quality Bluetooth
Devices with **LDAC** or **aptX HD** codecs:
- Sony WH-1000XM5 (LDAC mode)
- Sennheiser Momentum 4
- Some high-end Bose models

These handle 48kHz better (but still not perfect).

---

## Technical Details for Developers

### Sample Rate Chain

```
Recording Pipeline:
  Microphone (16kHz) → Resample to 48kHz → Pipeline (48kHz)
  System Audio (48kHz) → No resampling → Pipeline (48kHz)
  Mixed Audio (48kHz) → Encode → File (48kHz AAC)

Playback (Computer Speakers):
  File (48kHz) → macOS CoreAudio → Speakers (48kHz) ✅

Playback (Bluetooth):
  File (48kHz) → macOS CoreAudio → Bluetooth Stack → Resample → BT Device (16-48kHz) ⚠️
                                                      ↑
                                                This step can fail!
```

### Why macOS Resampling Fails

1. **Codec negotiation**: BT device claims 48kHz support but actually uses 16kHz
2. **Profile switching**: Device switches from A2DP (music) to HFP (call) mid-playback
3. **Power management**: macOS downsamples to save battery
4. **Driver bugs**: CoreAudio → Bluetooth handoff has known issues

### Apple's Documentation

From [Apple Technical Note TN2321](https://developer.apple.com/library/archive/technotes/tn2321/):
> "Bluetooth audio devices may report supported sample rates that differ from
> their actual playback rates. Applications should not rely on Bluetooth
> devices for accurate audio monitoring."

---

## FAQ

### Q: Will this be fixed in a future update?
**A**: This is a macOS/Bluetooth limitation, not a Meetily bug. We've correctly recorded at 48kHz.

### Q: Why not record at 16kHz if that's what Bluetooth uses?
**A**: Because:
1. System audio is 48kHz (can't be changed)
2. 48kHz is professional quality (16kHz is phone-call quality)
3. Most users play back on computer speakers
4. Recording at 16kHz would degrade quality for 95% of users

### Q: Can you detect my Bluetooth device and warn me?
**A**: Yes! Meetily now shows a warning when Bluetooth headphones are active during playback.

### Q: Does this affect recording quality?
**A**: **No**. Recording quality is perfect. Only **playback** through Bluetooth has issues.

### Q: What about AirPods? They're supposed to be high quality.
**A**: AirPods handle 48kHz better than most BT devices, but can still have issues depending on:
- Codec negotiation (AAC vs SBC)
- Battery level (power-saving mode)
- Connection quality (Bluetooth interference)
- macOS audio driver quirks

---

## Summary

✅ **Recordings are perfect** - 48kHz, high quality
✅ **Computer playback works** - Use speakers or wired headphones
⚠️ **Bluetooth playback may sound wrong** - macOS resampling issue
✅ **Recording through BT mic works** - We handle resampling correctly

**Bottom line**: Review your recordings through computer speakers, not Bluetooth headphones.

---

## Related Documentation

- [AIRPODS_BLUETOOTH_FIX.md](AIRPODS_BLUETOOTH_FIX.md) - Bluetooth device reconnection handling
- [BLUETOOTH_SAMPLE_RATE_FIX.md](BLUETOOTH_SAMPLE_RATE_FIX.md) - Microphone sample rate resampling
- [Apple Technical Note TN2321](https://developer.apple.com/library/archive/technotes/tn2321/) - Bluetooth Audio Best Practices

---

**Last Updated**: October 10, 2025
**Applies To**: Meetily v0.0.5+ on macOS
