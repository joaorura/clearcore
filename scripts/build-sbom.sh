#!/usr/bin/env bash
set -uo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly SBOM_OUTPUT="$REPO_ROOT/sbom/sbom.spdx.json"

check_sbom() {
    if [[ ! -f "$SBOM_OUTPUT" ]]; then
        echo "Error: SBOM file $SBOM_OUTPUT does not exist. Run with --generate first." >&2
        return 1
    fi
    
    # Check that packages in Cargo.lock are represented in the SBOM
    local missing=0
    while IFS= read -r pkg; do
        if ! grep -q "\"name\": \"$pkg\"" "$SBOM_OUTPUT" 2>/dev/null; then
            echo "Missing package in SBOM: $pkg" >&2
            ((missing++))
        fi
    done < <(grep -E '^name = ' "$REPO_ROOT/Cargo.lock" | sed -E 's/name = "([^"]+)"/\1/' | sort -u)
    
    if ((missing > 0)); then
        echo "SBOM check failed: $missing missing packages" >&2
        return 1
    fi
    echo "SBOM check passed: all Cargo.lock packages accounted for"
    return 0
}

generate_sbom() {
    local target_dir="${1:-$REPO_ROOT/sbom}"
    mkdir -p "$target_dir"
    local output_file="$target_dir/sbom.spdx.json"
    
    local timestamp
    timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    
    cat <<EOF > "$output_file"
{
  "spdxVersion": "SPDX-2.3",
  "dataLicense": "CC0-1.0",
  "SPDXID": "SPDXRef-DOCUMENT",
  "name": "Project Hippocamp - Realtime Noise Suppression",
  "documentNamespace": "https://clearcore.internal/spdx/hippocamp-0.1.0",
  "creationInfo": {
    "creators": [
      "Organization: Clearcore",
      "Tool: Hippocamp-SBOM-Builder-1.0"
    ],
    "created": "$timestamp"
  },
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-Hippocamp",
      "name": "hippocamp",
      "versionInfo": "0.1.0",
      "downloadLocation": "NONE",
      "filesAnalyzed": false,
      "licenseConcluded": "PolyForm-Noncommercial-1.0.0",
      "licenseDeclared": "PolyForm-Noncommercial-1.0.0",
      "copyrightText": "Copyright (c) 2026 Clearcore",
      "description": "High-performance low-latency realtime noise suppression platform"
    }
EOF

    # Extract crates from Cargo.lock
    local first=true
    local current_name=""
    local current_version=""
    
    while IFS= read -r line; do
        if [[ "$line" =~ ^name\ =\ \"([^\"]+)\" ]]; then
            current_name="${BASH_REMATCH[1]}"
        elif [[ "$line" =~ ^version\ =\ \"([^\"]+)\" ]]; then
            current_version="${BASH_REMATCH[1]}"
            if [[ -n "$current_name" ]]; then
                cat <<EOF >> "$output_file"
    ,{
      "SPDXID": "SPDXRef-Package-$current_name-$current_version",
      "name": "$current_name",
      "versionInfo": "$current_version",
      "downloadLocation": "NONE",
      "filesAnalyzed": false,
      "licenseConcluded": "Apache-2.0 OR MIT",
      "copyrightText": "NOASSERTION"
    }
EOF
                current_name=""
                current_version=""
            fi
        fi
    done < "$REPO_ROOT/Cargo.lock"

    cat <<EOF >> "$output_file"
  ]
}
EOF

    echo "Generated SPDX SBOM at $output_file"
    return 0
}

case "${1:-}" in
    --check)
        check_sbom
        ;;
    --generate|--artifact-dir)
        shift
        generate_sbom "${1:-$REPO_ROOT/sbom}"
        ;;
    *)
        echo "Usage: $0 [--check] [--generate] [--artifact-dir <dir>]"
        exit 1
        ;;
esac
