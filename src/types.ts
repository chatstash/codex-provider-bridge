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
  codexConfigExists: boolean;
  bridgeProviderConfigured: boolean;
  modelProviderIsBridge: boolean;
  loginStatus?: string;
}
