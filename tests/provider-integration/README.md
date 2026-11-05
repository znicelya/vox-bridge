# Volcengine service acceptance

This test sends one synthetic English phrase to the real paid Volcengine AST service. It never captures a microphone or plays audio. The paid test is ignored by default. Two default tests exercise its shared runtime/deadline wrapper with harmless and pending futures; normal Cargo tests do not read a service key, open the native keyring or contact the service.

An operator must have a key authorized for `volc.service_type.10053`, Internet access to the official WSS endpoint, and an unlocked native credential store. Read the service's current pricing and account limits before invoking it. Each invocation sends exactly two seconds of audio; automatic transport retries may create additional sessions.

Set `VOX_VOLC_API_KEY` securely in the invoking process environment, then run from the repository root:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test volcengine real_volcengine_translates_two_seconds_of_speech -- --ignored --exact --nocapture
```

Do not put a real key in a command saved to shell history, in a URL, in a repository file or in test output. The test writes the supplied key only into an isolated temporary encrypted application directory and resolves it through the production UUID-based SecretStore identity. The shared application data-encryption key is protected by the native keyring and may be created there on first use. The temporary encrypted provider entry is removed afterward. An unavailable/locked native keyring fails the test; there is no plaintext or mock fallback.

Production credential backends are Windows Credential Manager, macOS Keychain, and the Linux Secret Service over the user's session D-Bus. Linux builds require the D-Bus development package (`libdbus-1-dev` on Ubuntu); runtime needs a compatible unlocked secret service such as GNOME Keyring or KWallet. Ordinary unit tests inject their own memory key backend and need no desktop keyring.

The test waits for SessionStarted before sending 25 PCM16 mono 16 kHz packets at 80 ms intervals. A concurrent receiver requires nonempty final source text, nonempty final translated text, normalized 24 kHz PCM16 audio, and successful SessionFinished. A reconnect gap or terminal failure fails acceptance. No transcript, key, raw PCM or remote failure body is printed. Async work has a 45-second deadline; a separate 60-second process watchdog also bounds native keyring/shutdown stalls (exit 124). A failure does not establish service or audio-route acceptance.

The PCM fixture and its provenance are in [fixtures/README.md](fixtures/README.md). Record the commit, UTC time and pass/fail result in your acceptance evidence. OS audio routing and cross-platform driver acceptance remain separate checks in [platform-audio](../platform-audio/README.md).
