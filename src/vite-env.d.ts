/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly EXPO_PUBLIC_MOBILE_RELAY_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
