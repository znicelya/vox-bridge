# Ubuntu 22.04 audio acceptance

**Physical acceptance: PENDING — neither backend was run.** The development environment is not an Ubuntu 22.04 acceptance machine. No setup script, driver, audio-server route, or system default was changed during implementation. The manifest ships user-session scripts (not a bundled Linux driver binary).

Run two independent clean-machine acceptance cases: Ubuntu 22.04 with PipeWire plus its PulseAudio compatibility server, and Ubuntu 22.04 with native PulseAudio. Do not count one backend as evidence for the other or switch servers in the middle of a case.

## Machine and package preparation

1. Record `cat /etc/os-release`, `uname -r`, `pactl --version`, `pactl info`, USB microphone/headset details, call app/recorder versions and the released application package SHA-256/version. `pactl info` must identify which server is actually running. On PipeWire also record `pipewire --version`, `wpctl --version`, `wpctl status` and `pw-dump`; on PulseAudio record `pulseaudio --version`.
2. Before installation, save `pactl info`, `pactl list short sinks`, `pactl list short sources`, and `pactl list short modules`. Record default source/sink from `pactl info`. Install the released `.deb`/AppImage by its documented process, retaining any visible package-manager privilege prompt. The endpoint scripts themselves run as the logged-in desktop user, without `sudo`.
3. In the actual user audio session, run exactly the script for the selected case from the installed, verified resources (the source paths below are for development diagnosis only):

   ```sh
   # PipeWire case only:
   sh src-tauri/resources/audio/linux/setup-pipewire.sh
   # OR native PulseAudio case only:
   sh src-tauri/resources/audio/linux/setup-pulseaudio.sh
   ```

   Record the script SHA-256 (`sha256sum <script-path>`), commit/release version, exit code and stdout/stderr. These scripts explicitly replace matching VoxBridge modules; run them only on the designated acceptance session. Record a missing module/server as FAIL or blocker; do not silently fall back to the other backend.
4. Verify the installed modules and endpoint directions using `pactl list sinks`, `pactl list sources` and `pactl list short modules`. Expected pair mapping:

   | Logical role | Server endpoint |
   |---|---|
   | M1 call microphone | source `VoxBridge_Input`, description `VoxBridge Input` |
   | App render side feeding M1 | sink `VoxBridge_M1`, description `VoxBridge M1 Backend` |
   | M2 call speaker | sink `VoxBridge_Output`, description `VoxBridge Output` |
   | App capture side receiving M2 | monitor source `VoxBridge_Output.monitor` |

   Record module IDs, source/sink names, descriptions, PipeWire node/object IDs where applicable, physical ALSA/Pulse device identity, formats/rates and CPAL names. Verify default source/sink still match the before-install record.
5. Run `--list-devices` from [the shared instructions](README.md). CPAL 0.15 on Linux uses ALSA and may expose only generic server devices rather than the virtual sources/sinks. Do not pass a logical PulseAudio ID that is absent from CPAL, or use the default device as substitute evidence. If the intended pair is not individually addressable by the current primitive, record this integration blocker; the physical Cargo check cannot establish that pair. A call app/recorder with explicit PulseAudio/PipeWire selection can provide separate manual endpoint evidence, but it does not prove the app's CPAL binding.

## Call, tone, translation and recovery

Complete all eight steps in [the shared acceptance sequence](README.md#acceptance-sequence-and-evidence) independently for each backend. Select call microphone **VoxBridge Input** and speaker **VoxBridge Output** in the call app (or per-application streams in `pavucontrol` if required; record the explicit selections, without changing defaults). Run the opt-in physical diagnostic only when exact input/output CPAL names exist, using `--seconds 1 --timeout-seconds 5`; record JSON and verify the tone independently in the selected recorder/peer. Send the specified phrase in both translation directions, and record the correct destinations and translated content.

Unplug/reconnect the selected USB device during audio; verify pause, no output for at least five seconds after reconnect, failed resume while unavailable, and output only after explicit health-checked resume. For virtual endpoint loss, unload only the recorded numeric VoxBridge module IDs and reinstall with the same case's script; do not unload unrelated modules or restart the user's entire audio server. Product pause/resume UI and provider routing are not yet complete, so those integrated checks remain PENDING.

For cleanup use the install manifest's explicit uninstall procedure: inspect `pactl list short modules`, verify exact VoxBridge arguments, then unload only those numeric module IDs (dependent remap source before its backing sink). Record post-cleanup inventory and unchanged defaults.

## Evidence record (fill each column independently)

| Field | PipeWire + compatibility server | Native PulseAudio |
|---|---|---|
| Status / UTC / tester / commit | PENDING / not run | PENDING / not run |
| Ubuntu build / kernel / hardware | PENDING | PENDING |
| Server name / version / package versions | PENDING | PENDING |
| App installer version / SHA-256 / call app / recorder | PENDING | PENDING |
| Setup script SHA-256 / exit / module IDs | PENDING | PENDING |
| Privilege/session availability evidence | PENDING | PENDING |
| Defaults before / after | PENDING | PENDING |
| M1 source ID + CPAL name / render-side sink ID + CPAL name | PENDING | PENDING |
| M2 sink ID + CPAL name / capture-side monitor ID + CPAL name | PENDING | PENDING |
| Physical IDs + CPAL names / rates / channels | PENDING | PENDING |
| Synthetic JSON (separate, not physical proof) | PENDING on target | PENDING on target |
| Physical JSON / spectrum / destination recordings | PENDING | PENDING |
| Language pair / provider session ID / phrase translations | PENDING | PENDING |
| Disconnect / reconnect / explicit resume | PENDING | PENDING |
| Restart / uninstall / unchanged defaults | PENDING | PENDING |
| Final physical PASS/FAIL and evidence links | PENDING | PENDING |
