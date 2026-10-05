'use strict';

const fs = require('fs');
const path = require('path');
const os = require('os');

const DEFAULT_PROFILE = Object.freeze({
  is_enrolled: false,
  active_samples_count: 0,
  embedding_dim: 192,
  neural_eq_calibrated: false,
  gain_boost_db: 1.8,
});

/**
 * Resolves the configuration directory for Clearcore.
 * Priority:
 * 1. CLEARCORE_CONFIG_DIR env var
 * 2. On Linux: $XDG_CONFIG_HOME/clearcore or ~/.config/clearcore
 * 3. Electron userData (if app is ready)
 * 4. ~/.clearcore
 */
function getClearcoreStorageDir(appInstance = null) {
  if (process.env.CLEARCORE_CONFIG_DIR) {
    const dir = path.resolve(process.env.CLEARCORE_CONFIG_DIR);
    ensureDir(dir);
    return dir;
  }

  if (process.platform === 'linux') {
    const configHome = process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config');
    const dir = path.join(configHome, 'clearcore');
    ensureDir(dir);
    return dir;
  }

  if (appInstance && typeof appInstance.getPath === 'function') {
    try {
      const dir = appInstance.getPath('userData');
      ensureDir(dir);
      return dir;
    } catch {}
  }

  try {
    const electron = require('electron');
    if (electron && electron.app && typeof electron.app.getPath === 'function') {
      const dir = electron.app.getPath('userData');
      ensureDir(dir);
      return dir;
    }
  } catch {}

  const fallback = path.join(os.homedir(), '.clearcore');
  ensureDir(fallback);
  return fallback;
}

function ensureDir(dir) {
  if (!fs.existsSync(dir)) {
    try {
      fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    } catch (err) {
      console.warn(`[VoiceProfileStore] Could not create directory ${dir}:`, err.message);
    }
  }
}

function getVoiceProfilePath(baseDir) {
  return path.join(baseDir || getClearcoreStorageDir(), 'voice_profile.json');
}

function getVoiceSamplesPath(baseDir) {
  return path.join(baseDir || getClearcoreStorageDir(), 'voice_samples.json');
}

function getCallTakesPath(baseDir) {
  return path.join(baseDir || getClearcoreStorageDir(), 'call_takes.json');
}

function readJsonFile(filePath, fallback) {
  try {
    if (fs.existsSync(filePath)) {
      const content = fs.readFileSync(filePath, 'utf8');
      return JSON.parse(content);
    }
  } catch (err) {
    console.warn(`[VoiceProfileStore] Could not read ${filePath}:`, err.message);
  }
  return fallback;
}

function writeJsonFile(filePath, data) {
  try {
    const dir = path.dirname(filePath);
    ensureDir(dir);
    fs.writeFileSync(filePath, JSON.stringify(data, null, 2), { encoding: 'utf8', mode: 0o600 });
    return true;
  } catch (err) {
    console.warn(`[VoiceProfileStore] Could not write ${filePath}:`, err.message);
    return false;
  }
}

function readVoiceProfile(baseDir) {
  const current = readJsonFile(getVoiceProfilePath(baseDir), DEFAULT_PROFILE);
  return {
    ...DEFAULT_PROFILE,
    ...current,
  };
}

function writeVoiceProfile(profileUpdates = {}, baseDir) {
  const current = readVoiceProfile(baseDir);
  const updated = {
    ...current,
    ...profileUpdates,
  };
  writeJsonFile(getVoiceProfilePath(baseDir), updated);
  return updated;
}

function readVoiceSamples(baseDir) {
  const samples = readJsonFile(getVoiceSamplesPath(baseDir), []);
  return Array.isArray(samples) ? samples : [];
}

function writeVoiceSamples(samples, baseDir) {
  const list = Array.isArray(samples) ? samples : [];
  writeJsonFile(getVoiceSamplesPath(baseDir), list);
  // Auto-sync profile status active_samples_count
  writeVoiceProfile(
    {
      active_samples_count: list.length,
      is_enrolled: list.length > 0,
      neural_eq_calibrated: list.length > 0,
    },
    baseDir
  );
  return list;
}

function addVoiceSample(sampleData, baseDir) {
  if (!sampleData || typeof sampleData !== 'object') {
    throw new Error('Invalid sample data');
  }
  const samples = readVoiceSamples(baseDir);
  const sampleId = sampleData.id || `sample-vol-${Date.now()}`;
  const newSample = {
    id: sampleId,
    title: sampleData.title || `Amostra #${samples.length + 1}`,
    category: sampleData.category || 'Adição Voluntária',
    timestamp: sampleData.timestamp || new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    durationSec: typeof sampleData.durationSec === 'number' ? sampleData.durationSec : 5.0,
    audioUrl: sampleData.audioUrl || undefined,
    isInitialStep: Boolean(sampleData.isInitialStep),
  };

  // If sample with same id already exists, update it; otherwise prepend
  const existingIdx = samples.findIndex((s) => s.id === sampleId);
  let updated;
  if (existingIdx >= 0) {
    updated = [...samples];
    updated[existingIdx] = { ...updated[existingIdx], ...newSample };
  } else {
    updated = [newSample, ...samples];
  }

  writeVoiceSamples(updated, baseDir);
  return { sample: newSample, samples: updated };
}

function deleteVoiceSample(id, baseDir) {
  if (!id) return { success: false, reason: 'ID required' };
  const samples = readVoiceSamples(baseDir);
  const updated = samples.filter((s) => s.id !== id);
  writeVoiceSamples(updated, baseDir);
  return { success: true, id, samples: updated };
}

function readCallTakes(baseDir) {
  const takes = readJsonFile(getCallTakesPath(baseDir), []);
  return Array.isArray(takes) ? takes : [];
}

function writeCallTakes(takes, baseDir) {
  const list = Array.isArray(takes) ? takes : [];
  writeJsonFile(getCallTakesPath(baseDir), list);
  return list;
}

function approveCallTake(id, name, takeData, baseDir) {
  const takes = readCallTakes(baseDir);
  const targetTake = takes.find((t) => t.id === id) || takeData;
  const updatedTakes = takes.filter((t) => t.id !== id);
  writeCallTakes(updatedTakes, baseDir);

  const newSample = {
    id: `take-approved-${id || Date.now()}`,
    title: (targetTake && targetTake.title) || name || `Chamada Aprovada (${id})`,
    category: 'Chamada Real (Intake)',
    timestamp: (targetTake && targetTake.timestamp) || new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    durationSec: (targetTake && targetTake.durationSec) || 5.0,
    audioUrl: targetTake && targetTake.audioUrl,
  };

  const sampleResult = addVoiceSample(newSample, baseDir);
  return {
    success: true,
    id,
    sample: newSample,
    samples: sampleResult.samples,
    takes: updatedTakes,
  };
}

function dismissCallTake(id, baseDir) {
  const takes = readCallTakes(baseDir);
  const updatedTakes = takes.filter((t) => t.id !== id);
  writeCallTakes(updatedTakes, baseDir);
  return { success: true, id, takes: updatedTakes };
}

module.exports = {
  DEFAULT_PROFILE,
  getClearcoreStorageDir,
  getVoiceProfilePath,
  getVoiceSamplesPath,
  getCallTakesPath,
  readVoiceProfile,
  writeVoiceProfile,
  readVoiceSamples,
  writeVoiceSamples,
  addVoiceSample,
  deleteVoiceSample,
  readCallTakes,
  writeCallTakes,
  approveCallTake,
  dismissCallTake,
};
