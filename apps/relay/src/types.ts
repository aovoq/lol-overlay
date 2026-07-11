export interface Env {
  SESSIONS: DurableObjectNamespace;
  PAIRING_CODES: DurableObjectNamespace;
  RATE_LIMITS: DurableObjectNamespace;
  ASSETS: Fetcher;
  MOBILE_APP_URL?: string;
  MOBILE_RELAY_CREATE_SECRET?: string;
  /** Legacy binding kept so an existing deployment can rotate without downtime. */
  SESSION_CREATE_SECRET?: string;
}

export interface SessionMetadata {
  producerTokenHash: string;
  viewerTokenHash: string;
  expiresAt: number;
}

export interface PairingCodeMetadata {
  viewerUrl: string;
  expiresAt: number;
}

export interface RateLimitRequest {
  key: string;
  limit: number;
  windowMs: number;
}
