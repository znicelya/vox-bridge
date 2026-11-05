# Deterministic speech fixture

`hello-en-16k-s16le.pcm` contains the locally synthesized test phrase “Hello, can you hear me?” (en-US). It contains no user/customer recording. Format: raw little-endian signed 16-bit PCM, mono, 16,000 Hz; 32,000 samples / 64,000 bytes / exactly two seconds. SHA-256:

```text
5912af130915f0c1e58174a62ac2394578f990ba47017a6be4523581c4f6a8e7
```

Generated with Ubuntu eSpeak NG `1.51+dfsg-12build1`, voice `en-us`, speed `190`; source WAV was mono PCM16 at 22,050 Hz, 1.5340589569160998 seconds. SciPy `1.18.0` resampled it with `resample_poly(samples.astype(float64), 16000, 22050)`, then rounded/clipped to signed 16-bit and zero-padded to 32,000 samples. Speech was not truncated.

Reproduce in a scratch directory with those tool versions:

```sh
espeak-ng -v en-us -s190 -w hello-en.wav 'Hello, can you hear me?'
python3 - <<'PY'
import wave
import numpy as np
from scipy.signal import resample_poly
with wave.open('hello-en.wav', 'rb') as source:
    assert source.getnchannels() == 1 and source.getsampwidth() == 2
    rate = source.getframerate()
    samples = np.frombuffer(source.readframes(source.getnframes()), dtype='<i2')
converted = np.clip(np.rint(resample_poly(samples.astype(np.float64), 16000, rate)), -32768, 32767).astype('<i2')
assert len(converted) <= 32000
np.pad(converted, (0, 32000 - len(converted))).astype('<i2').tofile('hello-en-16k-s16le.pcm')
PY
sha256sum hello-en-16k-s16le.pcm
```

The phrase is authored for this repository and the fixture is synthetic tool output, not an external recording. eSpeak NG's code and voice-data packages have their own licenses recorded in their distribution copyright files; those tools/packages are not bundled in the application or this fixture directory. Generating the WAV/PCM writes files only and does not play or capture hardware.
