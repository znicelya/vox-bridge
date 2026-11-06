#!/bin/sh
set -eu
command -v pactl >/dev/null 2>&1 || { echo "PulseAudio pactl is required" >&2; exit 69; }
# Listing must succeed before any mutation. Sources depend on the M1 sink.
modules=$(pactl list short modules)
module_ids=$(printf '%s\n' "$modules" | awk '
  $1 ~ /^[0-9]+$/ {
    for (i = 3; i <= NF; i++) {
      if ($2 == "module-remap-source" && $i == "source_name=VoxBridge_Input") source = 1
      if ($2 == "module-null-sink" && ($i == "sink_name=VoxBridge_M1" || $i == "sink_name=VoxBridge_Output")) sink = 1
    }
    if (source) sources[++ns] = $1
    else if (sink) sinks[++nk] = $1
    source = sink = 0
  }
  END {
    for (i = 1; i <= ns; i++) print sources[i]
    for (i = 1; i <= nk; i++) print sinks[i]
  }')
for module_id in $module_ids; do
  pactl unload-module "$module_id"
done
# pactl joins argv into module arguments. Keep quotes for both the module-args
# parser and the nested PulseAudio property-list parser.
pactl load-module module-null-sink sink_name=VoxBridge_M1 "sink_properties=\"device.description='VoxBridge M1 Backend'\""
pactl load-module module-remap-source master=VoxBridge_M1.monitor source_name=VoxBridge_Input "source_properties=\"device.description='VoxBridge Input'\""
pactl load-module module-null-sink sink_name=VoxBridge_Output "sink_properties=\"device.description='VoxBridge Output'\""
