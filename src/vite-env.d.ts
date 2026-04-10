/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MONARCH_DEBUG_DESYNC?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
