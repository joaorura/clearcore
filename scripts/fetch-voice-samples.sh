#!/usr/bin/env bash
# Downloads the voice/noise test samples into fixtures/voice-samples/ (git-ignored) and
# verifies SHA-256 against scripts/voice-samples.sha256. Tutorial: docs/testing/voice-samples.md
#
# Usage: scripts/fetch-voice-samples.sh [--with-tagarela] [--from-dir DIR] [--verify-only]
#   --with-tagarela  also fetch the TAGARELA test shard (CC BY-NC-SA 4.0, local test data only)
#   --from-dir DIR   take already-downloaded files from DIR instead of the network
#                    (DIR/<name> or DIR/speech/<name> or DIR/noise/<name>)
#   --verify-only    download nothing; only check the hashes of the files present
# Environment: FIXTURES_DIR (default <repo>/fixtures/voice-samples), HASH_FILE.
# Needs: curl, sha256sum (or shasum), ffmpeg/ffprobe (noise clips), python3 with venv (VCTK range download).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES_DIR="${FIXTURES_DIR:-$ROOT/fixtures/voice-samples}"
HASH_FILE="${HASH_FILE:-$ROOT/scripts/voice-samples.sha256}"
VENV_DIR="$FIXTURES_DIR/.venv"
REMOTEZIP_VERSION="0.12.6"
USER_AGENT="clearcore-voice-samples/1.0"

VCTK_ZIP_URL="https://datashare.ed.ac.uk/server/api/core/bitstreams/535f4286-e54c-4038-838c-a02285e32cb2/content"
VCTK_PREFIX="wav48_silence_trimmed"
SPEECH_FILES=(p225_003 p225_008 p225_011 p225_022 p226_008 p226_016)

# name|url|range_bytes (0 = whole file)|start_seconds|raw_extension
NOISE_CLIPS=(
  "street_traffic_rain_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/8/8c/Urban_Street_on_a_Rainy_Afternoon.flac|6000000|5|flac"
  "forest_rain_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/b/b6/Light_Rain_Distant_Thunder_July_5th_2016.wav|5000000|0|wav"
  "ac_fan_cc0_10s|https://upload.wikimedia.org/wikipedia/commons/d/df/Resident_air-conditioned_out_door_unit.ogg|0|5|ogg"
)

TAGARELA_URL="https://huggingface.co/datasets/freds0/TAGARELA/resolve/main/data/test-00000-of-00001.parquet"
TAGARELA_REL="tagarela/test-00000-of-00001.parquet"
TAGARELA_BYTES=78277756

WITH_TAGARELA=0
FROM_DIR=""
VERIFY_ONLY=0
WARNINGS=0

die() { echo "ERROR: $*" >&2; exit 1; }
warn() { echo "WARNING: $*" >&2; WARNINGS=$((WARNINGS + 1)); }

usage() { sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --with-tagarela) WITH_TAGARELA=1 ;;
    --verify-only) VERIFY_ONLY=1 ;;
    --from-dir)
      [ $# -ge 2 ] || die "--from-dir needs a directory"
      FROM_DIR="$2"
      shift
      ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
  shift
done

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

expected_hash() { # $1 = path relative to FIXTURES_DIR
  [ -f "$HASH_FILE" ] || die "hash list not found: $HASH_FILE"
  awk -v p="$1" '$0 !~ /^#/ && $2 == p { print $1 }' "$HASH_FILE"
}

# Returns 0 = hash matches, 1 = mismatch, 2 = file missing or no expected hash.
check_file() {
  local rel="$1" want got
  [ -f "$FIXTURES_DIR/$rel" ] || return 2
  want="$(expected_hash "$rel")"
  [ -n "$want" ] || return 2
  got="$(sha256_of "$FIXTURES_DIR/$rel")"
  [ "$got" = "$want" ]
}

find_in_from_dir() { # $1 = file name, $2 = subfolder
  local candidate
  [ -n "$FROM_DIR" ] || return 1
  for candidate in "$FROM_DIR/$1" "$FROM_DIR/$2/$1"; do
    if [ -f "$candidate" ]; then echo "$candidate"; return 0; fi
  done
  return 1
}

# Originals must match exactly: a mismatch aborts.
require_original() {
  local rel="$1"
  check_file "$rel" || {
    rm -f -- "${FIXTURES_DIR:?}/$rel"
    die "SHA-256 mismatch for $rel (file removed)"
  }
  echo "ok   $rel"
}

# Derived clips depend on the ffmpeg version: a mismatch only warns.
check_derived() {
  local rel="$1" status=0
  check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then
    echo "ok   $rel"
  else
    warn "derived clip $rel differs from the recorded SHA-256 (depends on the ffmpeg version); keeping it"
  fi
}

write_attribution() {
  cat >"$FIXTURES_DIR/ATTRIBUTION.txt" <<'EOF'
Speech (speech/*.flac): CSTR VCTK Corpus v0.92, University of Edinburgh, The Centre for Speech
Technology Research (CSTR); Yamagishi et al. License CC BY 4.0 (attribution required).
https://datashare.ed.ac.uk/handle/10283/3443

Noise (noise/*.wav): 10 s mono 48 kHz PCM s16le excerpts of Wikimedia Commons recordings, CC0:
- street_traffic_rain_cc0_10s: https://commons.wikimedia.org/wiki/File:Urban_Street_on_a_Rainy_Afternoon.flac (5 s to 15 s)
- forest_rain_cc0_10s: https://commons.wikimedia.org/wiki/File:Light_Rain_Distant_Thunder_July_5th_2016.wav (0 s to 10 s)
- ac_fan_cc0_10s: https://commons.wikimedia.org/wiki/File:Resident_air-conditioned_out_door_unit.ogg (5 s to 15 s)

tagarela/ (only with --with-tagarela): freds0/TAGARELA test shard, CC BY-NC-SA 4.0. Local test
data only: never commit, never redistribute, never use to train distributed weights.
EOF
}

mkdir -p "$FIXTURES_DIR/speech" "$FIXTURES_DIR/noise"
TMP_DIR=""
cleanup() { if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then rm -rf -- "$TMP_DIR"; fi; }
trap cleanup EXIT

if [ "$VERIFY_ONLY" -eq 0 ]; then
  TMP_DIR="$(mktemp -d "$FIXTURES_DIR/.tmp.XXXXXX")"
fi

# ---------------------------------------------------------------- speech (VCTK)
missing_members=()
for id in "${SPEECH_FILES[@]}"; do
  name="${id}_mic1.flac"
  rel="speech/$name"
  if [ "$VERIFY_ONLY" -eq 1 ]; then
    status=0; check_file "$rel" || status=$?
    [ "$status" -eq 0 ] || die "$rel missing or with wrong SHA-256"
    echo "ok   $rel"
    continue
  fi
  status=0; check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then echo "ok   $rel (already present)"; continue; fi
  if src="$(find_in_from_dir "$name" speech)"; then
    cp -- "$src" "$FIXTURES_DIR/$rel"
    require_original "$rel"
  else
    missing_members+=("$VCTK_PREFIX/${id%%_*}/$name")
  fi
done

if [ "${#missing_members[@]}" -gt 0 ]; then
  command -v python3 >/dev/null 2>&1 || die "python3 is required for the VCTK range download"
  if [ ! -x "$VENV_DIR/bin/remotezip" ]; then
    echo "creating venv $VENV_DIR (remotezip $REMOTEZIP_VERSION)"
    python3 -m venv "$VENV_DIR" || die "python3 -m venv failed (on Debian/Ubuntu install python3-venv)"
    "$VENV_DIR/bin/pip" install --quiet "remotezip==$REMOTEZIP_VERSION"
  fi
  echo "fetching ${#missing_members[@]} VCTK files by HTTP Range (the full zip is 11.7 GB; it is NOT downloaded)"
  "$VENV_DIR/bin/remotezip" -d "$TMP_DIR/vctk" "$VCTK_ZIP_URL" "${missing_members[@]}"
  for member in "${missing_members[@]}"; do
    name="$(basename "$member")"
    [ -f "$TMP_DIR/vctk/$member" ] || die "remotezip did not extract $member"
    mv -- "$TMP_DIR/vctk/$member" "$FIXTURES_DIR/speech/$name"
    require_original "speech/$name"
  done
fi

# ---------------------------------------------------------------- noise clips
for spec in "${NOISE_CLIPS[@]}"; do
  IFS='|' read -r name url range_bytes start_seconds raw_ext <<<"$spec"
  rel="noise/$name.wav"
  if [ "$VERIFY_ONLY" -eq 1 ]; then
    [ -f "$FIXTURES_DIR/$rel" ] || die "$rel missing"
    status=0; check_file "$rel" || status=$?
    if [ "$status" -eq 0 ]; then echo "ok   $rel"; else warn "derived clip $rel differs from the recorded SHA-256"; fi
    continue
  fi
  status=0; check_file "$rel" || status=$?
  if [ "$status" -eq 0 ]; then echo "ok   $rel (already present)"; continue; fi
  if src="$(find_in_from_dir "$name.wav" noise)"; then
    cp -- "$src" "$FIXTURES_DIR/$rel"
    check_derived "$rel"
    continue
  fi
  for tool in ffmpeg ffprobe; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is required to cut the noise clips"
  done
  raw="$TMP_DIR/$name.$raw_ext"
  if [ "$range_bytes" -gt 0 ]; then
    curl -fsSL --retry 3 -A "$USER_AGENT" -r "0-$((range_bytes - 1))" -o "$raw" "$url"
  else
    curl -fsSL --retry 3 -A "$USER_AGENT" -o "$raw" "$url"
  fi
  ffmpeg -nostdin -v error -y -ss "$start_seconds" -t 10 -i "$raw" -ac 1 -ar 48000 -c:a pcm_s16le "$FIXTURES_DIR/$rel"
  duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$FIXTURES_DIR/$rel")"
  awk -v d="$duration" 'BEGIN { exit !(d >= 9.99 && d <= 10.01) }' || die "clip $rel has duration $duration s, expected 10 s"
  check_derived "$rel"
done

# ---------------------------------------------------------------- TAGARELA (opt-in)
if [ "$WITH_TAGARELA" -eq 1 ]; then
  echo "TAGARELA is CC BY-NC-SA 4.0: local test data only, never commit it (no audio is extracted)."
  dest="$FIXTURES_DIR/$TAGARELA_REL"
  if [ "$VERIFY_ONLY" -eq 0 ]; then
    mkdir -p "$FIXTURES_DIR/tagarela"
    if [ ! -f "$dest" ] || [ "$(wc -c <"$dest" | tr -d ' ')" != "$TAGARELA_BYTES" ]; then
      curl -fSL --progress-bar --retry 3 -A "$USER_AGENT" -C - -o "$dest.part" "$TAGARELA_URL"
      mv -- "$dest.part" "$dest"
    fi
  fi
  [ -f "$dest" ] || die "$TAGARELA_REL missing"
  [ "$(wc -c <"$dest" | tr -d ' ')" = "$TAGARELA_BYTES" ] || die "$TAGARELA_REL has the wrong size (expected $TAGARELA_BYTES bytes)"
  check_file "$TAGARELA_REL" || die "SHA-256 mismatch for $TAGARELA_REL"
  echo "ok   $TAGARELA_REL"
fi

write_attribution
if [ "$WARNINGS" -gt 0 ]; then
  echo "done with $WARNINGS warning(s)"
else
  echo "done"
fi
