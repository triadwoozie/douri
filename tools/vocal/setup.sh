#!/bin/bash
# Installs Vocal Top Line's DiffSinger singer under `vocal` in Pro Wave's settings folder
# (~/Library/Application Support/prowave-mac/vocal) on this Mac. Pro Wave sings with CHIMERA,
# built in, without it; DiffSinger is a second singer.
#
#   bash tools/vocal/setup.sh                         # the engine; then it says how to get the voice
#   bash tools/vocal/setup.sh --voicebank FILE.zip    # the engine and the voice from FILE.zip
#
# The voice, Hoshino Hanami by Team L♥VE, is not downloaded by this script: you download its zip
# yourself from its makers' page (VOICEBANK_PAGE below) after reading its terms. Its vocoder
# (AI♥dolGAN, from OpenVPI's NSF-HiFiGAN) is CC BY-NC-SA 4.0: NON-COMMERCIAL use only. The
# script prints this before it installs the voice.
#
# What it puts there (about 800 MB when done; about 3.3 GB while it builds, freed at the end):
#   engine/     the headless singer (tools/vocal/engine built against OpenUtau.Core, self-contained)
#   Library/OpenUtau/Singers/Hanami/   the Hoshino Hanami DiffSinger voicebank (Team L♥VE)
#   voices.json the installed voices, their controls and licence notes
# Nothing is started or left running. Model weights and voicebanks never go in the repository.
set -euo pipefail

OPENUTAU_REPO=https://github.com/stakira/OpenUtau.git
# The OpenUtau commit the singer is built and tested against.
OPENUTAU_COMMIT=f920e2423113f9a936ec77eebf1f192e02a7ea6a
# Where its makers publish the voicebank (open it in a browser and download the zip by hand).
VOICEBANK_PAGE='https://www.mediafire.com/file/ks3hpb5zldabo3s/Hoshino_Hanami_%257EAIdol%257E_for_DiffSinger_v1.0.zip/file'
HERE="$(cd "$(dirname "$0")" && pwd)"
VOCAL_REL="Library/Application Support/prowave-mac/vocal"
VOICEBANK_ZIP=""
if [ "${1:-}" = "--voicebank" ]; then
  VOICEBANK_ZIP="${2:?--voicebank needs the zip you downloaded}"
fi

licence_notice() {
  cat <<'TEXT'
Hoshino Hanami ~AI♥dol~ for DiffSinger, by Team L♥VE (LotteV).
  The voice: use for any purpose, commercial included, with credit to its creator (as its
  makers state it; read their terms on the download page).
  Its vocoder, AI♥dolGAN (from OpenVPI's NSF-HiFiGAN), is licensed CC BY-NC-SA 4.0:
  NON-COMMERCIAL use only. What you sing with it may not be used commercially.
TEXT
}

voices_json() {
  cat <<'JSON'
[
  {
    "id": "Hanami",
    "name": "Hoshino Hanami ~AI❤dol~ (DiffSinger)",
    "colors": ["01: Root (Normal/Core)", "02: Fragrance (Power)", "03: Nectar (Soft)"],
    "controls": ["gender", "velocity"],
    "languages": ["en", "ja", "de", "ko", "es", "zh", "fr"],
    "licence": "Hoshino Hanami by Team L❤VE (LotteV): use for any purpose, commercial included, with credit to the creator. Its AI❤dolGAN vocoder (from OpenVPI NSF-HiFiGAN) is CC BY-NC-SA 4.0, non-commercial."
  }
]
JSON
}


V="$HOME/$VOCAL_REL"
B="$V/build"
mkdir -p "$V"
free_kb=$(df -k "$V" | awk 'NR==2 {print $4}')
if [ "$free_kb" -lt 4000000 ]; then
  echo "Not enough disk space: $((free_kb / 1024)) MB free, about 3300 MB needed while building." >&2
  exit 1
fi
mkdir -p "$B"
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 NUGET_PACKAGES="$B/nuget"

echo "STEP .NET SDK"
if [ ! -x "$B/dotnet/dotnet" ]; then
  curl -sSL https://dot.net/v1/dotnet-install.sh -o "$B/dotnet-install.sh"
  bash "$B/dotnet-install.sh" --channel 10.0 --install-dir "$B/dotnet" >/dev/null
fi

echo "STEP OpenUtau $OPENUTAU_COMMIT"
if [ ! -d "$B/OpenUtau/.git" ]; then
  git init -q "$B/OpenUtau"
  git -C "$B/OpenUtau" remote add origin "$OPENUTAU_REPO"
fi
git -C "$B/OpenUtau" fetch -q --depth 1 origin "$OPENUTAU_COMMIT"
git -C "$B/OpenUtau" checkout -q FETCH_HEAD

echo "STEP singer"
mkdir -p "$B/engine-src"
cp "$HERE/engine/Program.cs" "$HERE/engine/ProwaveSing.csproj" "$B/engine-src/"
rm -rf "$V/engine.new"
"$B/dotnet/dotnet" publish "$B/engine-src/ProwaveSing.csproj" -c Release -r osx-arm64 \
  --self-contained true -p:OpenUtauDir="$B/OpenUtau" -o "$V/engine.new" >"$B/publish.log" 2>&1 \
  || { tail -20 "$B/publish.log" >&2; exit 1; }
rm -rf "$V/engine" && mv "$V/engine.new" "$V/engine"

echo "STEP voice: Hoshino Hanami"
S="$V/Library/OpenUtau/Singers"
mkdir -p "$S"
if [ ! -f "$S/Hanami/character.yaml" ]; then
  licence_notice
  if [ -z "$VOICEBANK_ZIP" ]; then
    echo
    echo "The engine is installed; the voice is not. To install it, download the voicebank's zip"
    echo "yourself from"
    echo "  $VOICEBANK_PAGE"
    echo "and run:  bash tools/vocal/setup.sh --voicebank /path/to/the.zip"
    rm -rf "$B"
    exit 0
  fi
  rm -rf "$B/hanami"
  unzip -q "$VOICEBANK_ZIP" -d "$B/hanami"
  src=$(dirname "$(find "$B/hanami" -name character.yaml -maxdepth 3 | head -1)")
  rm -rf "$S/Hanami"
  mv "$src" "$S/Hanami"
fi
voices_json > "$V/voices.json"

echo "STEP check"
cat > "$B/check.json" <<'JSON'
{"singer":"Hanami","language":"en","list_singers":true,"phrases":[]}
JSON
HOME="$V" "$V/engine/OpenUtau.Test" "$B/check.json" | tail -1

rm -rf "$B"
du -sh "$V"
echo "Installed in $V"
