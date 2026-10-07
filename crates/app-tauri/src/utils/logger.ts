import { invokeBridge } from '../bridge';

export type LogLevel = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';

class FrontendLogger {
  private log(level: LogLevel, arg1: string, arg2?: unknown, arg3?: unknown) {
    let target = 'FRONTEND';
    let message = arg1;
    let data = arg2;

    if (typeof arg2 === 'string') {
      target = arg1;
      message = arg2;
      data = arg3;
    }

    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [${level}] [${target}]`;

    // Console output for Developer Tools
    if (level === 'DEBUG') {
      console.debug(prefix, message, data !== undefined ? data : '');
    } else if (level === 'INFO') {
      console.info(prefix, message, data !== undefined ? data : '');
    } else if (level === 'WARN') {
      console.warn(prefix, message, data !== undefined ? data : '');
    } else if (level === 'ERROR') {
      console.error(prefix, message, data !== undefined ? data : '');
    }

    // Forward to Electron main process to persist in app.log
    try {
      invokeBridge('log_message', { level, target, message, data }).catch(() => {});
    } catch (_) {}
  }

  debug(message: string, data?: unknown): void;
  debug(target: string, message: string, data?: unknown): void;
  debug(arg1: string, arg2?: unknown, arg3?: unknown) {
    this.log('DEBUG', arg1, arg2, arg3);
  }

  info(message: string, data?: unknown): void;
  info(target: string, message: string, data?: unknown): void;
  info(arg1: string, arg2?: unknown, arg3?: unknown) {
    this.log('INFO', arg1, arg2, arg3);
  }

  warn(message: string, data?: unknown): void;
  warn(target: string, message: string, data?: unknown): void;
  warn(arg1: string, arg2?: unknown, arg3?: unknown) {
    this.log('WARN', arg1, arg2, arg3);
  }

  error(message: string, data?: unknown): void;
  error(target: string, message: string, data?: unknown): void;
  error(arg1: string, arg2?: unknown, arg3?: unknown) {
    this.log('ERROR', arg1, arg2, arg3);
  }
}

export const logger = new FrontendLogger();
