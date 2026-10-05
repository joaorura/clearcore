'use strict';

const path = require('path');
const fs = require('fs');

/**
 * Lista ordenada de candidatos ao binário do daemon.
 *
 * - Dev com CLEARCORE_DAEMON_BIN: único candidato (nunca cai em outro binário).
 * - Dev sem override: debug antes de release (a escolha final usa o mtime).
 * - Empacotado: ordem histórica preservada; o override é ignorado (variável só de dev).
 */
function daemonBinaryCandidates({ binName, env = {}, resourcesPath, appDir, cwd, isDev }) {
  const override = env.CLEARCORE_DAEMON_BIN;
  if (isDev && typeof override === 'string' && override !== '') {
    return [override];
  }
  const repoRoot = path.resolve(appDir, '..', '..', '..');
  const rel = [path.resolve(repoRoot, 'target', 'release', binName), path.resolve(cwd, 'target', 'release', binName)];
  const dbg = [path.resolve(repoRoot, 'target', 'debug', binName), path.resolve(cwd, 'target', 'debug', binName)];
  const head = [
    path.join(resourcesPath || '', 'bin', binName),
    path.join(resourcesPath || '', binName),
    path.resolve(appDir, '..', 'bin', binName),
    path.resolve(appDir, '..', '..', '..', 'bin', binName),
  ];
  if (isDev) {
    return [...head, ...dbg, ...rel];
  }
  return [...head, ...rel, ...dbg];
}

/**
 * Escolhe o binário. Em dev (sem override), entre os candidatos existentes fica
 * o de MAIOR mtime: um release antigo em target/release não pode vencer um
 * debug recém-compilado (nem o contrário), evitando rodar daemon velho.
 * Devolve { path, reason }.
 */
function pickDaemonBinary(opts) {
  const exists = opts.exists || fs.existsSync;
  const statMtimeMs = opts.statMtimeMs || ((p) => fs.statSync(p).mtimeMs);
  const override = opts.env && opts.env.CLEARCORE_DAEMON_BIN;
  const hasOverride = opts.isDev && typeof override === 'string' && override !== '';
  const candidates = daemonBinaryCandidates(opts);
  const found = [...new Set(candidates)].filter((c) => exists(c));

  if (hasOverride) {
    return found.length ? { path: found[0], reason: 'override' } : { path: null, reason: 'override_missing' };
  }
  if (found.length === 0) {
    return { path: null, reason: 'not_found' };
  }
  if (!opts.isDev) {
    return { path: found[0], reason: 'packaged-order' };
  }
  let best = found[0];
  let bestM = statMtimeMs(best);
  for (const c of found.slice(1)) {
    const m = statMtimeMs(c);
    if (m > bestM) {
      best = c;
      bestM = m;
    }
  }
  return { path: best, reason: 'newest-dev-build' };
}

/** Em dev com posse do daemon (dev.sh), um daemon alheio já ativo deve ser substituído. */
function shouldReplaceExistingDaemon({ env = {}, spawnedByApp, responsive }) {
  return env.CLEARCORE_DEV_OWN_DAEMON === '1' && Boolean(responsive) && !spawnedByApp;
}

module.exports = { daemonBinaryCandidates, pickDaemonBinary, shouldReplaceExistingDaemon };
