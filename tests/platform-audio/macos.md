# macOS 13 Ventura audio acceptance

**Physical acceptance: PENDING — not run.** No macOS host was available in the implementation environment. The signed audio extension and redistribution license are currently absent; `resources/audio/macos/install.sh` exits 78. No extension was installed or approved.

## Machine and package preparation

1. Use a clean macOS 13 machine and record `sw_vers`, CPU architecture (`uname -m`), USB microphone/headset, call application and recording application versions. Record system default input/output in System Settings → Sound before installation.
2. Once the licensed, signed package exists, record its name/version and run `shasum -a 256 <package-path>` and `pkgutil --check-signature <package-path>` for the released `.pkg`. Compare with the release manifest. Run the released installer, record the verified developer identity and administrator prompt, then follow its documented approval/restart flow.
3. If the actual released package uses a system extension, approve the identified developer in System Settings → Privacy & Security when prompted and record `systemextensionsctl list`. If it uses an AudioServerPlugIn instead, record its documented registration/approval evidence; do not invent a system-extension result. Restart only if requested. Denied approval must remain visible and actionable.
4. Grant the app microphone permission in System Settings → Privacy & Security → Microphone when requested; record the denied-then-approved behavior without changing unrelated app permissions. In Audio MIDI Setup → Audio Devices, verify **VoxBridge Input** (M1) and **VoxBridge Output** (M2), direction, rates and channels. Save `system_profiler SPAudioDataType -json` plus screenshots. Verify defaults are unchanged.
5. Run `--list-devices` from the [shared instructions](README.md), and record CPAL/CoreAudio names plus the release diagnostic's CoreAudio device UIDs for physical devices and both sides of M1/M2. `system_profiler` may not expose stable UIDs; do not mislabel its names as UIDs. If no UID diagnostic is available, record the missing evidence and keep that step pending.

## Call, tone, translation and recovery

Complete all eight steps in [the shared acceptance sequence](README.md#acceptance-sequence-and-evidence). Select **VoxBridge Input** as call microphone and **VoxBridge Output** as call speaker. Run the opt-in physical diagnostic with exact CPAL input/output names and `--seconds 1 --timeout-seconds 5`. Retain its JSON and the independently recorded 1 kHz tone at the intended peer/output. Send **“The bridge test is complete. One, two, three.”** in both source-language directions and verify the translated audio at the corresponding destination.

During active audio, unplug/reconnect the selected USB microphone/headset. Verify a paused state, at least five seconds without resumed output after reconnect, rejected resume while the device is absent, and restart only after explicit resume and fresh health checks. Repeat virtual endpoint loss using the package's supported lifecycle controls. Do not restart CoreAudio or unload unrelated extensions as a shortcut. Product UI/provider/lifecycle controls that do not yet exist remain PENDING.

## Evidence record

| Field | Observed value |
|---|---|
| Status / UTC / tester / commit | PENDING / not run |
| macOS build / architecture / hardware / call app / recorder versions | PENDING |
| Package version / SHA-256 / signer / license | PENDING — package unavailable |
| Administrator / extension or plug-in approval / microphone permission / restart | PENDING |
| Defaults before / after | PENDING |
| M1 microphone UID + CPAL name / app render UID + CPAL name | PENDING |
| M2 speaker UID + CPAL name / app capture UID + CPAL name | PENDING |
| Physical input/output UIDs + CPAL names / rates / channels | PENDING |
| Synthetic JSON (separate, not physical proof) | PENDING on macOS |
| Physical JSON / 1 kHz spectrum / recordings for each destination | PENDING |
| Language pair / provider session ID / phrase translations | PENDING |
| Unplug / reconnect / paused-state / explicit resume evidence | PENDING |
| Restart / uninstall / unchanged defaults evidence | PENDING |
| Final physical PASS/FAIL and evidence links | PENDING |
