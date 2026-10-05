#!/usr/bin/env bash
# Generates static DNF (RPM) and APT (DEB) repository metadata
# for hosting on GitHub Pages (https://joaorura.github.io/clearcore)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ASSETS_DIR="${1:-${REPO_ROOT}/release-assets}"
OUT_DIR="${2:-${REPO_ROOT}/repo-site}"

echo "=========================================================="
echo " Building DNF (RPM) and APT (DEB) Repository Metadata"
echo " Source Assets: ${ASSETS_DIR}"
echo " Output Target: ${OUT_DIR}"
echo "=========================================================="

rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}/rpm/x86_64" "${OUT_DIR}/rpm/aarch64"
mkdir -p "${OUT_DIR}/apt/pool/main" "${OUT_DIR}/apt/dists/stable/main/binary-amd64" "${OUT_DIR}/apt/dists/stable/main/binary-arm64"
mkdir -p "${OUT_DIR}/winget"

# ----------------------------------------------------
# 1. PROCESS RPM PACKAGES (FEDORA / RHEL / OPENSUSE)
# ----------------------------------------------------
echo "📦 Organizing RPM packages for DNF..."
shopt -s nullglob
x86_rpms=("${ASSETS_DIR}"/*x86_64.rpm "${ASSETS_DIR}"/*x64.rpm)
arm_rpms=("${ASSETS_DIR}"/*aarch64.rpm "${ASSETS_DIR}"/*arm64.rpm)

for rpm in "${x86_rpms[@]}"; do
    echo "  -> Copying x86_64 RPM: $(basename "${rpm}")"
    cp "${rpm}" "${OUT_DIR}/rpm/x86_64/"
done

for rpm in "${arm_rpms[@]}"; do
    echo "  -> Copying aarch64 RPM: $(basename "${rpm}")"
    cp "${rpm}" "${OUT_DIR}/rpm/aarch64/"
done

if command -v createrepo_c >/dev/null 2>&1; then
    echo "🔨 Running createrepo_c for x86_64 and aarch64..."
    if [ -d "${OUT_DIR}/rpm/x86_64" ] && [ "$(ls -A "${OUT_DIR}/rpm/x86_64")" ]; then
        createrepo_c --general-compress-type=gz "${OUT_DIR}/rpm/x86_64"
    fi
    if [ -d "${OUT_DIR}/rpm/aarch64" ] && [ "$(ls -A "${OUT_DIR}/rpm/aarch64")" ]; then
        createrepo_c --general-compress-type=gz "${OUT_DIR}/rpm/aarch64"
    fi
else
    echo "⚠️ createrepo_c not found. Skipping repodata generation."
fi

# Generate clearcore.repo file for DNF
cat << 'EOF' > "${OUT_DIR}/rpm/clearcore.repo"
[clearcore]
name=Clearcore Realtime AI Noise Suppression Virtual Mic
baseurl=https://joaorura.github.io/clearcore/rpm/$basearch/
enabled=1
gpgcheck=0
repo_gpgcheck=0
metadata_expire=1d
type=rpm-md
EOF

# ----------------------------------------------------
# 2. PROCESS DEB PACKAGES (DEBIAN / UBUNTU / LINUX MINT)
# ----------------------------------------------------
echo "📦 Organizing DEB packages for APT..."
deb_files=("${ASSETS_DIR}"/*.deb)
for deb in "${deb_files[@]}"; do
    echo "  -> Copying DEB: $(basename "${deb}")"
    cp "${deb}" "${OUT_DIR}/apt/pool/main/"
done

cd "${OUT_DIR}/apt"
if command -v dpkg-scanpackages >/dev/null 2>&1; then
    echo "🔨 Scanning DEB packages with dpkg-scanpackages..."
    dpkg-scanpackages --arch amd64 pool/main > dists/stable/main/binary-amd64/Packages 2>/dev/null || true
    gzip -9c dists/stable/main/binary-amd64/Packages > dists/stable/main/binary-amd64/Packages.gz

    dpkg-scanpackages --arch arm64 pool/main > dists/stable/main/binary-arm64/Packages 2>/dev/null || true
    gzip -9c dists/stable/main/binary-arm64/Packages > dists/stable/main/binary-arm64/Packages.gz

    # Generate Release file
    cat << 'EOF' > dists/stable/Release
Origin: Clearcore
Label: Clearcore
Suite: stable
Codename: stable
Architectures: amd64 arm64
Components: main
Description: Official Clearcore Realtime AI Noise Suppression APT Repository
EOF
else
    echo "⚠️ dpkg-scanpackages not found. Skipping APT Packages index."
fi
cd "${REPO_ROOT}"

# ----------------------------------------------------
# 3. WINGET MANIFEST (MICROSOFT WINGET)
# ----------------------------------------------------
echo "📦 Generating WinGet manifest template..."
VERSION="0.1.0-beta.3"
if [ -f "${REPO_ROOT}/crates/app-tauri/package.json" ]; then
    VERSION="$(node -p "require('${REPO_ROOT}/crates/app-tauri/package.json').version")"
fi

WIN_EXE="$(ls "${ASSETS_DIR}"/Clearcore-Setup.exe 2>/dev/null | head -n1 || true)"
EXE_SHA256="UNKNOWN"
if [ -n "${WIN_EXE}" ] && [ -f "${WIN_EXE}" ]; then
    EXE_SHA256="$(sha256sum "${WIN_EXE}" | awk '{print $1}')"
fi

cat << EOF > "${OUT_DIR}/winget/joaorura.Clearcore.yaml"
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.1.6.0.schema.json
PackageIdentifier: joaorura.Clearcore
PackageVersion: ${VERSION}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
EOF

cat << EOF > "${OUT_DIR}/winget/joaorura.Clearcore.installer.yaml"
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.1.6.0.schema.json
PackageIdentifier: joaorura.Clearcore
PackageVersion: ${VERSION}
Platform:
  - Windows.Desktop
MinimumOSVersion: 10.0.17763.0
InstallerType: nullsoft
Scope: user
InstallModes:
  - interactive
  - silent
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/joaorura/clearcore/releases/download/v${VERSION}/Clearcore-Setup.exe
    InstallerSha256: ${EXE_SHA256}
ManifestType: installer
ManifestVersion: 1.6.0
EOF

cat << EOF > "${OUT_DIR}/winget/joaorura.Clearcore.locale.en-US.yaml"
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.1.6.0.schema.json
PackageIdentifier: joaorura.Clearcore
PackageVersion: ${VERSION}
PackageLocale: en-US
Publisher: João Rura
PublisherUrl: https://github.com/joaorura
PackageName: Clearcore
PackageUrl: https://github.com/joaorura/clearcore
License: PolyForm Noncommercial 1.0.0
ShortDescription: Realtime AI noise-suppression virtual microphone powered by DeepFilterNet3.
Tags:
  - noise-suppression
  - microphone
  - deepfilternet
  - audio
  - ai
ManifestType: defaultLocale
ManifestVersion: 1.6.0
EOF

# ----------------------------------------------------
# 4. LANDING PAGE FOR GITHUB PAGES
# ----------------------------------------------------
cat << 'EOF' > "${OUT_DIR}/index.html"
<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Clearcore - Repositório Oficial de Pacotes (DNF, APT, WinGet)</title>
  <style>
    :root {
      --bg: #0b0f19;
      --card-bg: #151d30;
      --text: #e2e8f0;
      --accent: #38bdf8;
      --accent-glow: rgba(56, 189, 248, 0.2);
      --border: #22304e;
      --code-bg: #070a12;
    }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background-color: var(--bg);
      color: var(--text);
      margin: 0;
      padding: 2rem 1rem;
      display: flex;
      justify-content: center;
    }
    .container {
      max-width: 860px;
      width: 100%;
    }
    header {
      text-align: center;
      margin-bottom: 2.5rem;
    }
    h1 {
      font-size: 2.5rem;
      color: #fff;
      margin-bottom: 0.5rem;
      letter-spacing: -0.03em;
    }
    h1 span {
      color: var(--accent);
    }
    p.lead {
      color: #94a3b8;
      font-size: 1.15rem;
    }
    .card {
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 1.5rem;
      margin-bottom: 1.5rem;
      box-shadow: 0 4px 20px rgba(0,0,0,0.3);
    }
    h2 {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      font-size: 1.4rem;
      margin-top: 0;
      color: #fff;
    }
    pre {
      background: var(--code-bg);
      border: 1px solid var(--border);
      padding: 1rem;
      border-radius: 8px;
      overflow-x: auto;
      color: #38bdf8;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 0.95rem;
    }
    .badge {
      display: inline-block;
      padding: 0.25rem 0.5rem;
      border-radius: 6px;
      font-size: 0.75rem;
      font-weight: bold;
      text-transform: uppercase;
      background: var(--accent-glow);
      color: var(--accent);
      border: 1px solid var(--accent);
    }
    footer {
      text-align: center;
      margin-top: 3rem;
      color: #64748b;
      font-size: 0.9rem;
    }
    a { color: var(--accent); text-decoration: none; }
    a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <h1>Clear<span>core</span></h1>
      <p class="lead">Repositório Oficial de Pacotes para Fedora (DNF), Ubuntu/Debian (APT) e Windows (WinGet)</p>
      <span class="badge">DeepFilterNet3 • PipeWire • WaveRT • CoreAudio</span>
    </header>

    <div class="card">
      <h2>🔵 Fedora / RHEL / CentOS (DNF)</h2>
      <p>Instalação direta e atualizações automáticas via <code>dnf</code>:</p>
      <pre><code># 1. Adicionar o repositório
sudo dnf config-manager addrepo --from-repofile=https://joaorura.github.io/clearcore/rpm/clearcore.repo

# 2. Instalar o Clearcore
sudo dnf install -y clearcore</code></pre>
    </div>

    <div class="card">
      <h2>🟠 Debian / Ubuntu / Linux Mint (APT)</h2>
      <p>Instalação nativa via <code>apt</code>:</p>
      <pre><code># 1. Adicionar o repositório do Clearcore
echo "deb [trusted=yes] https://joaorura.github.io/clearcore/apt stable main" | sudo tee /etc/apt/sources.list.d/clearcore.list

# 2. Atualizar índices e instalar
sudo apt update && sudo apt install -y clearcore</code></pre>
    </div>

    <div class="card">
      <h2>🪟 Windows (WinGet)</h2>
      <p>Instalação de comando único no Windows 10/11:</p>
      <pre><code>winget install joaorura.Clearcore</code></pre>
    </div>

    <footer>
      <p>Clearcore • Código aberto no <a href="https://github.com/joaorura/clearcore" target="_blank">GitHub</a></p>
    </footer>
  </div>
</body>
</html>
EOF

chmod +x "${OUT_DIR}/index.html" || true
echo "✅ Static repository metadata and landing page generated in ${OUT_DIR}!"
