'use strict';

const assert = require('assert');
const path = require('path');
const {
  daemonBinaryCandidates,
  pickDaemonBinary,
  shouldReplaceExistingDaemon,
  isDevModeArgv,
  noCoreDumpSpawn,
} = require('../electron/daemon-binary.cjs');

const base = {
  binName: 'svc',
  env: {},
  resourcesPath: '/res',
  appDir: '/app/electron',
  cwd: '/cwd',
};
const root = path.resolve('/app/electron', '..', '..', '..');
const dbg = path.join(root, 'target', 'debug', 'svc');
const rel = path.join(root, 'target', 'release', 'svc');
const mk = (files) => ({
  exists: (p) => p in files,
  statMtimeMs: (p) => files[p],
});

// override em dev => único candidato
assert.deepStrictEqual(
  daemonBinaryCandidates({ ...base, isDev: true, env: { CLEARCORE_DAEMON_BIN: '/o/svc' }, ...mk({}) }),
  ['/o/svc'],
);
assert.deepStrictEqual(
  pickDaemonBinary({ ...base, isDev: true, env: { CLEARCORE_DAEMON_BIN: '/o/svc' }, ...mk({ '/o/svc': 1, [dbg]: 5 }) }),
  { path: '/o/svc', reason: 'override' },
);
// override inexistente em dev
assert.deepStrictEqual(
  pickDaemonBinary({ ...base, isDev: true, env: { CLEARCORE_DAEMON_BIN: '/o/svc' }, ...mk({ [dbg]: 5 }) }),
  { path: null, reason: 'override_missing' },
);
// override vazio é ignorado
assert.strictEqual(
  pickDaemonBinary({ ...base, isDev: true, env: { CLEARCORE_DAEMON_BIN: '' }, ...mk({ [dbg]: 5 }) }).path,
  dbg,
);
// debug mais novo que release
assert.deepStrictEqual(
  pickDaemonBinary({ ...base, isDev: true, ...mk({ [dbg]: 200, [rel]: 100 }) }),
  { path: dbg, reason: 'newest-dev-build' },
);
// release mais novo
assert.strictEqual(pickDaemonBinary({ ...base, isDev: true, ...mk({ [dbg]: 100, [rel]: 200 }) }).path, rel);
// só um existente
assert.strictEqual(pickDaemonBinary({ ...base, isDev: true, ...mk({ [rel]: 1 }) }).path, rel);
assert.strictEqual(pickDaemonBinary({ ...base, isDev: true, ...mk({ [dbg]: 1 }) }).path, dbg);
// ordem dev: debug antes de release
const devList = daemonBinaryCandidates({ ...base, isDev: true, ...mk({}) });
assert.ok(devList.indexOf(dbg) < devList.indexOf(rel));

// empacotado: override ignorado e ordem original
const pk = daemonBinaryCandidates({ ...base, isDev: false, env: { CLEARCORE_DAEMON_BIN: '/o/svc' }, ...mk({}) });
assert.deepStrictEqual(pk, [
  path.join('/res', 'bin', 'svc'),
  path.join('/res', 'svc'),
  path.resolve('/app/electron', '..', 'bin', 'svc'),
  path.resolve('/app/electron', '..', '..', '..', 'bin', 'svc'),
  rel,
  path.resolve('/cwd', 'target', 'release', 'svc'),
  dbg,
  path.resolve('/cwd', 'target', 'debug', 'svc'),
]);
// empacotado: primeiro existente, sem comparar mtime
assert.deepStrictEqual(
  pickDaemonBinary({ ...base, isDev: false, env: { CLEARCORE_DAEMON_BIN: '/o/svc' }, ...mk({ '/o/svc': 9, [dbg]: 999, [rel]: 1 }) }),
  { path: rel, reason: 'packaged-order' },
);
// nenhum existente
assert.strictEqual(pickDaemonBinary({ ...base, isDev: true, ...mk({}) }).path, null);
assert.strictEqual(pickDaemonBinary({ ...base, isDev: false, ...mk({}) }).path, null);

// posse
const own = { CLEARCORE_DEV_OWN_DAEMON: '1' };
assert.strictEqual(shouldReplaceExistingDaemon({ env: own, spawnedByApp: false, responsive: true }), true);
assert.strictEqual(shouldReplaceExistingDaemon({ env: own, spawnedByApp: true, responsive: true }), false);
assert.strictEqual(shouldReplaceExistingDaemon({ env: own, spawnedByApp: false, responsive: false }), false);
assert.strictEqual(shouldReplaceExistingDaemon({ env: {}, spawnedByApp: false, responsive: true }), false);
assert.strictEqual(shouldReplaceExistingDaemon({ env: { CLEARCORE_DEV_OWN_DAEMON: '0' }, spawnedByApp: false, responsive: true }), false);

assert.strictEqual(isDevModeArgv(['electron', 'electron/main.cjs', '--dev']), true);
assert.strictEqual(isDevModeArgv(['electron', 'electron/main.cjs']), false);

// core dump desligado: o daemon segura PCM cru em memória durante o cadastro de voz
const withPrlimit = noCoreDumpSpawn('/b/svc', ['--run'], { platform: 'linux', exists: (p) => p === '/usr/bin/prlimit' });
assert.deepStrictEqual(withPrlimit, { command: '/usr/bin/prlimit', args: ['--core=0', '--', '/b/svc', '--run'] });
const viaSh = noCoreDumpSpawn('/b/a b/svc', ['--run'], { platform: 'darwin', exists: () => false });
assert.strictEqual(viaSh.command, '/bin/sh');
// O caminho e os argumentos vão como $0/$@ (nunca interpolados no script).
assert.deepStrictEqual(viaSh.args, ['-c', 'ulimit -c 0; exec "$0" "$@"', '/b/a b/svc', '--run']);
assert.deepStrictEqual(noCoreDumpSpawn('C:\\svc.exe', ['--run'], { platform: 'win32', exists: () => true }), { command: 'C:\\svc.exe', args: ['--run'] });
// Os argumentos de entrada não são alterados.
const inArgs = ['--run'];
noCoreDumpSpawn('/b/svc', inArgs, { platform: 'linux', exists: () => false });
assert.deepStrictEqual(inArgs, ['--run']);

console.log('daemon-binary selftest OK');
