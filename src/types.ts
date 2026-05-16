export interface BridgeConfig {
  host: string;
  port: number;
  upstreamBaseUrl: string;
  apiKeyEnv: string;
  apiKey?: string;
  providerId: string;
  providerName: string;
}

export type ApiKeySource = "environment" | "config";

export interface PublicBridgeConfig extends Omit<BridgeConfig, "apiKey"> {
  apiKeySaved: boolean;
}

export interface PathOptions {
  env?: NodeJS.ProcessEnv;
  platform?: NodeJS.Platform;
}

export interface InstallOptions {
  codexConfigPath?: string;
  bridgeHome?: string;
  config?: BridgeConfig;
}

export interface InstallResult {
  codexConfigPath: string;
  bridgeConfigPath: string;
  backupPath: string;
}

export interface RestoreOptions {
  codexConfigPath?: string;
  bridgeHome?: string;
}

export interface RestoreResult {
  codexConfigPath: string;
  backupPath: string;
}

export interface DoctorResult {
  bridgeConfigPath: string;
  codexConfigPath: string;
  config: PublicBridgeConfig;
  apiKeyPresent: boolean;
  apiKeySource: ApiKeySource | null;
  portOpen: boolean;
  daemon: DaemonStatus;
  startup: StartupStatus;
  codexConfigExists: boolean;
  bridgeProviderConfigured: boolean;
  modelProviderIsBridge: boolean;
  loginStatus?: string;
}

export interface DaemonState {
  pid: number;
  startedAt: string;
  command: string;
  logPath: string;
  host: string;
  port: number;
}

export interface DaemonStatus {
  statePath: string;
  logPath: string;
  running: boolean;
  pid?: number;
  stale: boolean;
}

export type StartupMethod = "windows-task-scheduler" | "systemd-user";

export interface StartupStatus {
  supported: boolean;
  installed: boolean;
  method?: StartupMethod;
  taskName?: string;
  scriptPath?: string;
  serviceName?: string;
  servicePath?: string;
  detail?: string;
}

export interface StartupInstallResult extends StartupStatus {
  supported: true;
  installed: true;
  method: StartupMethod;
}

export interface StartupUninstallResult extends StartupStatus {
  removed: boolean;
}

export interface StartDaemonResult {
  alreadyRunning: boolean;
  pid?: number;
  statePath: string;
  logPath: string;
  url: string;
}

export interface StopDaemonResult {
  stopped: boolean;
  wasRunning: boolean;
  pid?: number;
  statePath: string;
}
