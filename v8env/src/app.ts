export interface AppRelease {
  name: string;
  version: number;
  env: string;
  region?: string;
  config: unknown
}