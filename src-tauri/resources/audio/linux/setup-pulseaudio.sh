#!/bin/sh
set -eu
command -v pactl >/dev/null 2>&1 || { echo "PulseAudio pactl is required" >&2; exit 69; }
pactl list short modules | awk '$2 == "module-null-sink" && ($0 ~ /sink_name=VoxBridge_M1/ || $0 ~ /sink_name=VoxBridge_Output/) || $2 == "module-remap-source" && $0 ~ /source_name=VoxBridge_Input/ { print $1 }' | while read -r module_id; do
  pactl unload-module "$module_id"
done
pactl load-module module-null-sink sink_name=VoxBridge_M1 sink_properties=device.description='VoxBridge M1 Backend'
pactl load-module module-remap-source master=VoxBridge_M1.monitor source_name=VoxBridge_Input source_properties=device.description='VoxBridge Input'
pactl load-module module-null-sink sink_name=VoxBridge_Output sink_properties=device.description='VoxBridge Output'
