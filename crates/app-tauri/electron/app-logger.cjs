'use strict';

const fs = require('fs');
const path = require('path');
const os = require('os');

const LOG_SUBDIR = path.join('clearcore', 'logs');
const CURRENT_LOG_NAME = 'app.log';
const RETENTION_MS = 24 * 3600 * 1000; // 24 hours

class AppLogger {
  constructor() {
    this.logDir = this.resolveLogDir();
    this.currentFile = path.join(this.logDir, CURRENT_LOG_NAME);
    this.activeDate = this.getUtcDateString();
    this.init();
  }

  resolveLogDir() {
    if (process.platform === 'win32') {
      const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
      return path.join(appData, LOG_SUBDIR);
    }
    if (process.platform === 'darwin') {
      return path.join(os.homedir(), 'Library', 'Application Support', LOG_SUBDIR);
    }
    const dataHome =
      process.env.XDG_DATA_HOME && path.isAbsolute(process.env.XDG_DATA_HOME)
        ? process.env.XDG_DATA_HOME
        : path.join(os.homedir(), '.local', 'share');
    return path.join(dataHome, LOG_SUBDIR);
  }

  getUtcDateString() {
    const d = new Date();
    return d.toISOString().slice(0, 10); // YYYY-MM-DD
  }

  init() {
    try {
      if (!fs.existsSync(this.logDir)) {
        fs.mkdirSync(this.logDir, { recursive: true, mode: 0o700 });
      }
      this.pruneOldLogs();
    } catch (err) {
      console.warn('[AppLogger] Initialization warning:', err.message);
    }
  }

  checkRotation() {
    const today = this.getUtcDateString();
    if (today !== this.activeDate) {
      try {
        const archiveName = `app.${this.activeDate}.log`;
        const archivePath = path.join(this.logDir, archiveName);
        if (fs.existsSync(this.currentFile) && !fs.existsSync(archivePath)) {
          fs.renameSync(this.currentFile, archivePath);
        }
        this.activeDate = today;
        this.pruneOldLogs();
      } catch (err) {
        console.warn('[AppLogger] Rotation warning:', err.message);
      }
    }
  }

  pruneOldLogs() {
    try {
      if (!fs.existsSync(this.logDir)) return;
      const files = fs.readdirSync(this.logDir);
      const now = Date.now();

      for (const file of files) {
        if (!file.startsWith('app.') || !file.endsWith('.log') || file === CURRENT_LOG_NAME) {
          continue;
        }
        const filePath = path.join(this.logDir, file);
        try {
          const stat = fs.statSync(filePath);
          if (now - stat.mtimeMs > RETENTION_MS) {
            fs.unlinkSync(filePath);
          }
        } catch (_) {}
      }
    } catch (err) {
      console.warn('[AppLogger] Prune warning:', err.message);
    }
  }

  log(level, target, message, data) {
    this.checkRotation();
    const timestamp = new Date().toISOString();
    let payload = '';
    if (data !== undefined && data !== null) {
      if (typeof data === 'string') {
        payload = ' ' + data;
      } else {
        try {
          payload = ' ' + JSON.stringify(data);
        } catch (_) {
          payload = ' [Unserializable Data]';
        }
      }
    }
    const line = `${timestamp} [${level.toUpperCase()}] [${target}] ${message}${payload}\n`;

    try {
      fs.appendFileSync(this.currentFile, line, { mode: 0o600 });
    } catch (err) {
      console.warn('[AppLogger] Write error:', err.message);
    }
  }

  debug(target, message, data) {
    this.log('DEBUG', target, message, data);
  }

  info(target, message, data) {
    this.log('INFO', target, message, data);
  }

  warn(target, message, data) {
    this.log('WARN', target, message, data);
  }

  error(target, message, data) {
    this.log('ERROR', target, message, data);
  }
}

const loggerInstance = new AppLogger();
module.exports = loggerInstance;
