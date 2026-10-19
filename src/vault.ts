// Mirrors the backend's secret classifier (src-tauri/src/vault.rs). Kept in sync
// by hand because the two sides must agree on which fields are write-only.
export const VAULT_MASK = '••••••••';

export const isSecretName = (name: string): boolean =>
  /token|key|secret|password|auth|bearer/i.test(name);
