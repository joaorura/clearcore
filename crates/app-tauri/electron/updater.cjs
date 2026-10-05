// Clearcore Auto-Updater Engine
// Checks for new GitHub releases, matches native OS packages (RPM for Fedora, DEB for Ubuntu, Setup.exe for Windows, DMG for macOS)
// and handles downloading and direct installation.

const https = require('https');
const fs = require('fs');
const path = require('path');
const os = require('os');
const { spawn, exec } = require('child_process');

const GITHUB_REPO = 'joaorura/clearcore';
const API_URL = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`;

function fetchLatestRelease() {
  return new Promise((resolve, reject) => {
    const options = {
      headers: {
        'User-Agent': 'Clearcore-Desktop-App',
        Accept: 'application/vnd.github.v3+json',
      },
    };

    https
      .get(API_URL, options, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          https
            .get(res.headers.location, options, (res2) => {
              handleResponse(res2, resolve, reject);
            })
            .on('error', reject);
          return;
        }
        handleResponse(res, resolve, reject);
      })
      .on('error', reject);
  });
}

function handleResponse(res, resolve, reject) {
  let body = '';
  res.on('data', (chunk) => (body += chunk));
  res.on('end', () => {
    try {
      if (res.statusCode !== 200) {
        return reject(new Error(`GitHub API returned HTTP ${res.statusCode}: ${body.slice(0, 100)}`));
      }
      resolve(JSON.parse(body));
    } catch (e) {
      reject(e);
    }
  });
}

// Detect Linux Distribution family (fedora, debian, etc.)
function detectLinuxDistro() {
  try {
    if (fs.existsSync('/etc/os-release')) {
      const content = fs.readFileSync('/etc/os-release', 'utf8');
      if (/ID=fedora/i.test(content) || /ID_LIKE=.*fedora/i.test(content) || /ID=rhel/i.test(content)) {
        return 'fedora';
      }
      if (/ID=ubuntu/i.test(content) || /ID=debian/i.test(content) || /ID_LIKE=.*(debian|ubuntu)/i.test(content)) {
        return 'debian';
      }
      if (/ID=arch/i.test(content)) {
        return 'arch';
      }
    }
  } catch {}
  return 'generic';
}

function selectBestAsset(assets, platform, arch) {
  if (!Array.isArray(assets) || assets.length === 0) return null;

  const isArm = arch === 'arm64' || arch === 'aarch64';

  if (platform === 'linux') {
    const distro = detectLinuxDistro();
    if (distro === 'fedora') {
      // Prioritize RPM for Fedora/RHEL
      const rpm = assets.find((a) => a.name.endsWith('.rpm') && (isArm ? a.name.includes('aarch64') : !a.name.includes('aarch64')));
      if (rpm) return { asset: rpm, format: 'rpm', installerType: 'dnf' };
    }
    if (distro === 'debian') {
      // Prioritize DEB for Ubuntu/Debian
      const deb = assets.find((a) => a.name.endsWith('.deb') && (isArm ? a.name.includes('arm64') : !a.name.includes('arm64')));
      if (deb) return { asset: deb, format: 'deb', installerType: 'apt' };
    }
    // Universal AppImage fallback
    const appImage = assets.find((a) => a.name.endsWith('.AppImage') && (isArm ? a.name.includes('arm64') : !a.name.includes('arm64')));
    if (appImage) return { asset: appImage, format: 'appimage', installerType: 'appimage' };

    // Tarball fallback
    const tar = assets.find((a) => a.name.endsWith('.tar.gz') && (isArm ? a.name.includes('arm64') : !a.name.includes('arm64')));
    if (tar) return { asset: tar, format: 'tar.gz', installerType: 'archive' };
  }

  if (platform === 'win32') {
    if (isArm) {
      const winArm = assets.find((a) => a.name.includes('win32-arm64') || a.name.includes('arm64.exe') || a.name.includes('arm64.zip'));
      if (winArm) return { asset: winArm, format: winArm.name.endsWith('.exe') ? 'exe' : 'zip', installerType: 'nsis' };
    }
    const exe = assets.find((a) => a.name.endsWith('Setup.exe') || a.name.endsWith('.exe'));
    if (exe) return { asset: exe, format: 'exe', installerType: 'nsis' };
    const zip = assets.find((a) => a.name.endsWith('.zip') && !a.name.includes('arm64'));
    if (zip) return { asset: zip, format: 'zip', installerType: 'portable' };
  }

  if (platform === 'darwin') {
    const dmg = assets.find((a) => a.name.endsWith('.dmg') && (isArm ? a.name.includes('arm64') : a.name.includes('x64')));
    if (dmg) return { asset: dmg, format: 'dmg', installerType: 'dmg' };
    const tar = assets.find((a) => a.name.endsWith('.tar.gz') && (isArm ? a.name.includes('arm64') : a.name.includes('x64')));
    if (tar) return { asset: tar, format: 'tar.gz', installerType: 'archive' };
  }

  return { asset: assets[0], format: 'unknown', installerType: 'manual' };
}

function compareVersions(vA, vB) {
  const cleanA = vA.replace(/^v/, '').split(/[-_+]/)[0];
  const cleanB = vB.replace(/^v/, '').split(/[-_+]/)[0];
  const partsA = cleanA.split('.').map(Number);
  const partsB = cleanB.split('.').map(Number);

  for (let i = 0; i < 3; i++) {
    const a = partsA[i] || 0;
    const b = partsB[i] || 0;
    if (a > b) return 1;
    if (a < b) return -1;
  }

  // If base versions are equal, check pre-release tag
  const normA = vA.replace(/^v/, '');
  const normB = vB.replace(/^v/, '');
  return normA.localeCompare(normB);
}

async function checkForUpdates(currentVersion) {
  try {
    const release = await fetchLatestRelease();
    const latestTag = release.tag_name || '';
    const hasUpdate = compareVersions(latestTag, currentVersion) > 0;

    const matched = selectBestAsset(release.assets, process.platform, process.arch);

    return {
      success: true,
      currentVersion,
      latestVersion: latestTag,
      updateAvailable: hasUpdate,
      releaseName: release.name || latestTag,
      releaseNotes: release.body || '',
      publishedAt: release.published_at,
      releaseUrl: release.html_url,
      distro: process.platform === 'linux' ? detectLinuxDistro() : null,
      asset: matched
        ? {
            name: matched.asset.name,
            downloadUrl: matched.asset.browser_download_url,
            size: matched.asset.size,
            format: matched.format,
            installerType: matched.installerType,
          }
        : null,
    };
  } catch (error) {
    return {
      success: false,
      error: error.message,
      currentVersion,
      updateAvailable: false,
    };
  }
}

function downloadFile(url, destPath, onProgress) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    const options = { headers: { 'User-Agent': 'Clearcore-Desktop-App' } };

    function makeRequest(currentUrl) {
      https
        .get(currentUrl, options, (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            return makeRequest(res.headers.location);
          }
          if (res.statusCode !== 200) {
            return reject(new Error(`Download failed with HTTP ${res.statusCode}`));
          }

          const totalBytes = parseInt(res.headers['content-length'] || '0', 10);
          let receivedBytes = 0;

          res.on('data', (chunk) => {
            receivedBytes += chunk.length;
            file.write(chunk);
            if (typeof onProgress === 'function' && totalBytes > 0) {
              onProgress({
                receivedBytes,
                totalBytes,
                percent: Math.min(100, Math.round((receivedBytes / totalBytes) * 100)),
              });
            }
          });

          res.on('end', () => {
            file.end(() => resolve(destPath));
          });
        })
        .on('error', (err) => {
          fs.unlink(destPath, () => {});
          reject(err);
        });
    }

    makeRequest(url);
  });
}

function launchInstaller(filePath, installerType) {
  return new Promise((resolve, reject) => {
    const ext = path.extname(filePath).toLowerCase();

    if (process.platform === 'linux') {
      if (ext === '.rpm') {
        // Direct install using pkexec dnf/rpm, or open in native PackageKit / GNOME Software / Discover
        exec(`xdg-open "${filePath}" || pkexec dnf install -y "${filePath}"`, (err) => {
          if (err) return reject(err);
          resolve({ status: 'launched', method: 'xdg-open/pkexec' });
        });
        return;
      }
      if (ext === '.deb') {
        exec(`xdg-open "${filePath}" || pkexec apt install -y "${filePath}"`, (err) => {
          if (err) return reject(err);
          resolve({ status: 'launched', method: 'xdg-open/pkexec' });
        });
        return;
      }
      if (ext === '.appimage') {
        fs.chmodSync(filePath, 0o755);
        spawn(filePath, [], { detached: true, stdio: 'ignore' }).unref();
        resolve({ status: 'launched', method: 'appimage' });
        return;
      }
      // Tarball: open directory
      exec(`xdg-open "${path.dirname(filePath)}"`, (err) => {
        if (err) return reject(err);
        resolve({ status: 'opened_folder' });
      });
      return;
    }

    if (process.platform === 'win32') {
      if (ext === '.exe') {
        spawn(filePath, ['/S'], { detached: true, stdio: 'ignore' }).unref();
        resolve({ status: 'launched', method: 'nsis-silent' });
        return;
      }
      exec(`explorer.exe /select,"${filePath}"`, (err) => {
        if (err) return reject(err);
        resolve({ status: 'opened_folder' });
      });
      return;
    }

    if (process.platform === 'darwin') {
      exec(`open "${filePath}"`, (err) => {
        if (err) return reject(err);
        resolve({ status: 'opened_dmg' });
      });
      return;
    }

    resolve({ status: 'unsupported_platform' });
  });
}

module.exports = {
  checkForUpdates,
  downloadFile,
  launchInstaller,
  detectLinuxDistro,
};
