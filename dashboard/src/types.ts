import type { Dispatch, ReactNode, SetStateAction } from "react";

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue | undefined };
export type JsonObject = { [key: string]: JsonValue | undefined };
export type ProfileId = string | undefined;
export type OS = "Windows" | "macOS" | "Linux" | "Android";
export type EngineName = "playwright" | "patchright" | "camoufox" | "cloak";
export type ViewName = "profiles" | "health" | "engines" | "extensions" | "proxies" | "folders" | "automation" | "settings";
export type Setter<T> = Dispatch<SetStateAction<T>>;
export type Action = () => void | Promise<unknown>;

export interface Proxy extends JsonObject {
  type: string; host: string; port: string | number;
  user?: string | null | undefined; pass?: string | null | undefined; country?: string | null | undefined;
}
export interface Profile extends JsonObject {
  id?: ProfileId; name: string | number | null; folder: string; status: string; os: OS; tags: (string | number)[];
  engine: string | null; browser: string; browserVersion: string; userAgent: string;
  screen: string; cores: number; memory: number; webglVendor: string; webglRenderer: string;
  canvas: string; webrtc: string; audio: string; fonts: string; locale: string; timezone: string;
  geo: string; mediaVideo: number; mediaAudio: number; dnt: boolean; proxy: Proxy | null;
  notes: string; startupUrls: string; extensions: string; lastActive: string;
  clientRects?: string | undefined; webglImage?: string | undefined; mediaDevices?: string | undefined;
  mediaSpeakers?: number | undefined; fontList?: string | undefined; bookmarks?: string | undefined;
  launchArgs?: string | undefined; dns?: string | undefined; ipBased?: string[] | undefined;
  clearOnStart?: string[] | undefined; switchedOff?: string[] | undefined;
  _new?: boolean | undefined; _passwordSet?: boolean | undefined; _rev?: JsonValue | undefined;
}
export type ProfileWrite = { [K in keyof Profile]?: Profile[K] | undefined };
export type BulkPatch = Partial<Profile> & { addTags?: string[] | undefined };
export type ProfileSetter = (patch: Partial<Profile>) => void;
export type FieldPath = string[];
export interface Config {
  root?: string; version?: string; folders?: string[]; default_engine?: string; default_extensions?: string[]; proxyCount?: number;
}
export type Engines = Record<string, boolean>;
export interface Grade { grade?: string; score: number; passed?: number; total?: number }
export interface CheckResult { name: string; ok: boolean; detail?: string }
export interface ProbeResult extends Partial<Grade> {
  before?: Grade; after?: Grade; checks?: CheckResult[]; changed?: string[]; os?: string;
  ip?: string; city?: string | null; country?: string | null; latencyMs?: number; ja3Hash?: string | null; ja4?: string | null;
  countries?: string[]; resolvers?: JsonValue[]; error?: string; profile?: Partial<Profile>;
}
export type Probe = { kind: string; state: "running" } | { kind: string; state: "error"; data: string } | { kind: string; state: "done"; data: ProbeResult };
export interface PoolProxy extends Proxy {
  id: string; label?: string | null; rotateUrl?: string | null; archived?: boolean; profiles?: number;
  status?: string; lastCheck?: { ip?: string; country?: string | null; latencyMs?: number; error?: string };
}
export interface Schedule { id: string; profileId: ProfileId; everyHours: number; minutes: number; preset: string; enabled?: boolean }
export interface Extension { path: string; name?: string; icon?: string | null }
export interface ApiToken { id: string; name: string; created: number; lastUsed?: number | null }
export interface TokenCreated extends ApiToken { token: string }
export interface TokenList { tokens: ApiToken[]; envToken: boolean }
export interface AttachResult { port: number; headless?: boolean; wsUrl?: string; snippets?: Record<string, string> }
export interface Rotation { rotated: boolean; previousIp?: string | null; ip?: string; error?: string }
export interface HealthRow {
  id: string; name: string; engine: string; status: string; coherent: boolean; issues: string[];
  collides?: boolean; needsAttention?: boolean; lastActive?: string;
  proxy?: { set?: boolean; country?: string | null; type?: string | null } | null;
  trust?: Partial<Grade> | null; schedule?: Partial<Schedule> | null;
}
export interface ProfileHealth { profiles: HealthRow[]; summary: { total?: number; coherent?: number; withProxy?: number; scheduled?: number; colliding?: number; needsAttention?: number } }
export interface Filters { status: string; engine: string; os: string; proxy: string; tag: string }
export interface Confirmation { title: string; body: ReactNode; label?: ReactNode; danger?: boolean; run: Action }
export interface ProfileViewProps { profiles: Profile[]; live: boolean; refresh: () => Promise<unknown> }
export const errorMessage = (error: unknown): string => error instanceof Error ? error.message : String(error);
