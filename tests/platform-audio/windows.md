# Windows 10 22H2 audio acceptance

**Physical acceptance: PENDING — not run.** No Windows host was available in the implementation environment. The signed endpoint package and redistribution license are currently absent; `resources/audio/windows/install.ps1` exits 78. Do not bypass the packaging gate or substitute an unsigned driver.

## Machine and package preparation

1. Use a clean Windows 10 22H2 machine (record exact build from `winver`) with the intended USB microphone/headset and a second machine for the test call. Record all fields below. Install development dependencies only if running the Cargo diagnostic from source; use the actual release installer for installation acceptance.
2. In Settings → System → Sound, record default input/output names. Save an endpoint inventory in PowerShell:

   ```powershell
   Get-PnpDevice -Class AudioEndpoint | Select-Object Status, FriendlyName, InstanceId | Format-List
   ```

3. Once a licensed signed package is supplied, record its exact filename/version and run `Get-FileHash -Algorithm SHA256 <package-path>` and `Get-AuthenticodeSignature <package-path>`. Verify the released manifest matches. Run the released Vox Bridge installer, record the UAC dialog and verified publisher, approve the documented administrator request, and restart only when requested. A denied UAC request must leave a visible actionable state. Missing package/signature/license is a blocker.
4. Reopen Sound → Manage sound devices and `mmsys.cpl` (Recording/Playback tabs). Verify **VoxBridge Input** as M1 and **VoxBridge Output** as M2, ready and correctly directed. Save `Get-PnpDevice` output again with InstanceIds for each side of the virtual pair and physical devices. Verify Windows defaults match the preinstall inventory.
5. Run `--list-devices` from the [shared instructions](README.md); record CPAL/WASAPI names alongside Windows InstanceIds. M1's app render endpoint and M2's app capture endpoint must be available explicitly; if not, record the blocker.

## Call, tone, translation and recovery

Follow all eight steps in [the shared acceptance sequence](README.md#acceptance-sequence-and-evidence). Set the test-call microphone to **VoxBridge Input** and speaker to **VoxBridge Output**. Run the opt-in physical diagnostic with exact CPAL input/output names and `--seconds 1 --timeout-seconds 5`, retain JSON and a remote recording, and verify 1 kHz with the recorder's spectrum view. Send the exact spoken phrase in both translation directions and record the resulting audio/transcripts.

Unplug/reconnect the selected USB device while audio is active. Observe the paused state, five seconds of silence after reconnect, failed resume while unavailable, and explicit successful resume after health verification. Repeat virtual endpoint loss using the approved package's lifecycle controls. Do not disable unrelated devices or alter the Windows defaults to make a test pass. If the application lacks these controls/provider wiring, retain PENDING with that blocker.

## Evidence record

| Field | Observed value |
|---|---|
| Status / UTC / tester / commit | PENDING / not run |
| OS build / hardware / call app / recorder versions | PENDING |
| Installer filename / version / SHA-256 / signing publisher / license | PENDING — package unavailable |
| UAC approval / driver status / restart logs | PENDING |
| Defaults before / after | PENDING |
| M1 microphone ID + CPAL name / app render-side ID + CPAL name | PENDING |
| M2 speaker ID + CPAL name / app capture-side ID + CPAL name | PENDING |
| Physical input/output IDs + CPAL names / rates / channels | PENDING |
| Synthetic JSON (separate, not physical proof) | PENDING on Windows |
| Physical diagnostic JSON / tone spectrum / A→B and B→A recordings | PENDING |
| Language pair / provider session ID / phrase translations | PENDING |
| Unplug / reconnect / paused-state / explicit resume evidence | PENDING |
| Restart / uninstall / unchanged defaults evidence | PENDING |
| Final physical PASS/FAIL and evidence links | PENDING |
