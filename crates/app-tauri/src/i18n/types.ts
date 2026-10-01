export interface Translations {
  app: {
    title: string;
    subtitle: string;
    daemonOnline: string;
    daemonOffline: string;
    minimizeToTray: string;
    language: string;
    daemonUnreachable: string;
  };
  modes: {
    title: string;
    active: string;
    bypass: string;
    mute: string;
    switchFailed: string;
  };
  inputDevice: {
    title: string;
    badge: string;
    description: string;
    refresh: string;
    refreshing: string;
    refreshTitle: string;
    noDevices: string;
    activeDevice: string;
    selectAria: string;
    defaultSuffix: string;
    deviceSelectedFeedback: string;
    selectionFailed: string;
  };
  virtualMic: {
    title: string;
    description: string;
    verifyCreate: string;
    verifying: string;
    verifyTitle: string;
    isDefault: string;
    makeDefault: string;
    statusWaveRt: string;
    statusCoreAudio: string;
    statusPipeWire: string;
    statusActive: string;
    statusNotDetected: string;
    systemDefaultLabel: string;
    systemDefaultYes: string;
    systemDefaultNo: string;
    deviceLabel: string;
    formatLabel: string;
    channelsLabel: string;
    latencyLabel: string;
    channelMono: string;
    latencySamples: string;
    notDetectedWarning: string;
    windowsInstruction: string;
    macosInstruction: string;
    linuxInstruction: string;
    actionVerifying: string;
    actionSuccess: string;
    actionFailed: string;
    actionSettingDefault: string;
    actionDefaultSuccess: string;
    actionDefaultSent: string;
    systemDefault: string;
    activeStatus: string;
    permissionHint: string;
    createError: string;
    setDefaultError: string;
  };
  supervisor: {
    title: string;
    restartBtn: string;
    stateLabel: string;
    crashesLabel: string;
    restartFailed: string;
  };
  autostart: {
    title: string;
    itemTitle: string;
    itemDesc: string;
    enabled: string;
    disabled: string;
    tip: string;
    changeFailed: string;
  };
  diagnostics: {
    title: string;
    refresh: string;
    exportBtn: string;
    exportSuccess: string;
    exportError: string;
    deviceIdLabel: string;
    supervisorGen: string;
    genNumber: string;
    lastRestart: string;
    attemptNumber: string;
    noneHealthy: string;
    latenciesTitle: string;
    p50: string;
    p95: string;
    p99: string;
    budgetTitle: string;
    measured: string;
    configured: string;
    derived: string;
    unobservable: string;
    causesTitle: string;
    normalState: string;
    privacyNotice: string;
  };
}

export type LocaleCode = string;

export interface LocaleMeta {
  code: LocaleCode;
  name: string;
  flag?: string;
  translations: Translations;
}
