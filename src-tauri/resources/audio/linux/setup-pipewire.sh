#!/bin/sh
set -eu
command -v pactl >/dev/null 2>&1 || { echo "pactl is required for PipeWire's PulseAudio compatibility layer" >&2; exit 69; }
pactl load-module module-null-sink sink_name=VoxBridge_Output sink_properties=device.description='VoxBridge Output'
pactl load-module module-remap-source master=VoxBridge_Output.monitor source_name=VoxBridge_Input source_properties=device.description='VoxBridge Input'
