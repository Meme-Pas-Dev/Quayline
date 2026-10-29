import { Channel, invoke } from "@tauri-apps/api/core";

export interface ApiError {
  code: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as ApiError).message);
  return String(e);
}

export function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) return String((e as ApiError).code);
  return undefined;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export type HostKind = "ssh" | "sftp";

export interface Host {
  id: string;
  label: string;
  /** Ce qu'ouvre un clic sur la tuile ; l'autre type reste accessible. */
  kind: HostKind;
  group: string | null;
  tags: string[];
  hostname: string;
  port: number;
  username: string | null;
  identityId: string | null;
  color: string | null;
  notes: string;
}

export type HostInput = Omit<Host, "id"> & { id: string | null };

export interface KeyInfo {
  algorithm: string;
  fingerprint: string;
  publicKey: string;
  encrypted: boolean;
}

export interface Identity {
  id: string;
  label: string;
  username: string | null;
  hasPassword: boolean;
  hasPrivateKey: boolean;
  hasPassphrase: boolean;
  key: KeyInfo | null;
}

/** Les secrets existants ne sont jamais renvoyés : on indique quoi en faire. */
export type SecretField = { action: "keep" } | { action: "clear" } | { action: "set"; value: string };

export interface IdentityInput {
  id: string | null;
  label: string;
  username: string | null;
  password: SecretField;
  privateKey: SecretField;
  keyPassphrase: SecretField;
}

export interface Settings {
  auto_lock_minutes: number;
  clipboard_clear_seconds: number;
  terminal_font_size: number;
  terminal_theme: string;
  lock_closes_sessions: boolean;
  lock_on_system_events: boolean;
  /** null : pas encore demandé. */
  check_updates: boolean | null;
  /** Délai de grâce du fournisseur d'identité, en minutes ; 0 = à chaque déverrouillage. */
  oidc_reauth_minutes: number;
}

export interface UpdateInfo {
  version: string;
  /** AAAA-MM-JJ */
  published: string | null;
  /** Installation possible depuis l'application (macOS). */
  installable: boolean;
}

export type InstallProgress = { step: "download"; done: number; total: number } | { step: "verify" } | { step: "install" };

export interface ExportSelection {
  hosts: boolean;
  identities: boolean;
  knownHosts: boolean;
  settings: boolean;
}

export interface ImportCounts {
  new: number;
  duplicates: number;
}

export interface ImportPreview {
  createdAt: number;
  appVersion: string;
  hosts: ImportCounts;
  identities: ImportCounts;
  knownHosts: ImportCounts;
  knownHostsIdentical: number;
  settings: boolean;
  hostKeys: HostKeyChange[];
  securityChanges: SettingChange[];
}

export interface HostKeyChange {
  hostname: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  replaces: boolean;
}

export interface SettingChange {
  setting: "autoLockMinutes" | "clipboardClearSeconds" | "lockClosesSessions" | "lockOnSystemEvents" | "oidcReauthMinutes";
  current: number | boolean;
  imported: number | boolean;
  weaker: boolean;
}

export type Duplicates = "skip" | "replace" | "keepBoth";

export interface ImportReport {
  added: number;
  replaced: number;
  skipped: number;
  settings: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  build: string;
  commit: string;
  flatpak: boolean;
  /** Mises à jour livrées par une boutique (Flathub) : pas de recherche. */
  updates_by_store: boolean;
}

export interface Fido2Key {
  id: string;
  label: string;
}

export interface Fido2Device {
  state: "none" | "multiple" | "unsupported" | "ready" | "windows";
  product: string | null;
  hasPin: boolean;
  minPinLength: number;
  pinRetries: number | null;
}

export type SessionEvent =
  | { type: "status"; state: "connecting" | "authenticating" }
  | {
      type: "hostKey";
      promptId: number;
      changed: boolean;
      hostname: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      previousFingerprint: string | null;
    }
  | {
      type: "input";
      promptId: number;
      title: string;
      message: string;
      fields: { label: string; secret: boolean }[];
      /** « hôte:port » si les textes viennent du serveur (keyboard-interactive). */
      fromServer: string | null;
    }
  | { type: "connected" }
  | { type: "sftpReady"; home: string }
  | { type: "closed"; error: string | null };

export interface FileEntry {
  name: string;
  path: string;
  kind: "dir" | "file" | "link";
  isDir: boolean;
  size: number;
  modified: number | null;
  permissions: string | null;
  /** Bits de permission (`0o7777`). */
  mode: number | null;
  owner: string | null;
  group: string | null;
}

/** Date de modification et taille d'un fichier distant, comparées avant l'envoi. */
export interface RemoteVersion {
  modified: number | null;
  size: number;
}

export interface TextEncoding {
  charset: "utf-8" | "utf-16le" | "utf-16be" | "windows-1252";
  bom: boolean;
}

export interface RemoteText {
  content: string;
  version: RemoteVersion;
  encoding: TextEncoding;
}

export type EditEvent =
  | { kind: "opened"; id: string; sftpId: string; name: string; remote: string }
  | { kind: "synced"; id: string }
  | { kind: "conflict"; id: string }
  | { kind: "failed"; id: string; message: string }
  | { kind: "closed"; id: string };

export type TransferEvent =
  | { type: "started"; id: string; sftpId: string; direction: "upload" | "download"; name: string; destination: string }
  | { type: "progress"; id: string; done: number; total: number; files: number }
  | { type: "finished"; id: string; error: string | null; cancelled: boolean; skipped: number };

export type OidcProviderId = "pocket-id" | "authentik" | "authelia" | "keycloak" | "entra" | "google" | "generic";

export interface OidcStatus {
  provider: OidcProviderId;
  providerName: string;
  issuer: string;
  clientId: string;
  redirectPort: number;
  redirectUri: string;
  requiredGroup: string | null;
  scopes: string;
  groupsClaim: string;
  hasClientSecret: boolean;
  /** Connexion par téléphone (QR code) proposée. */
  phone: boolean;
  account: string;
}

export interface QrMatrix {
  size: number;
  /** Modules ligne par ligne : true pour un module sombre. */
  modules: boolean[];
}

export interface OidcDeviceStart {
  /** À comparer avec le code affiché sur le téléphone. */
  userCode: string;
  verificationUri: string;
  qr: QrMatrix;
  expiresIn: number;
}

export interface OidcIdentity {
  subject: string;
  name: string | null;
  email: string | null;
  groups: string[];
  /** Le fournisseur n'indique pas l'heure de connexion (réauthentification non vérifiable). */
  reauthUnverified: boolean;
}

export interface OidcEnrollInput {
  provider: OidcProviderId;
  issuer: string;
  clientId: string;
  redirectPort: number;
  requiredGroup: string | null;
  scopes: string;
  groupsClaim: string;
  clientSecret: string | null;
}

export interface KnownHost {
  hostname: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  addedAt: number;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  setLanguage: (language: string) => invoke<void>("set_language", { language }),
  updateCheck: () => invoke<UpdateInfo | null>("update_check"),
  updateOpen: () => invoke<void>("update_open"),
  updateInstall: (onProgress: (p: InstallProgress) => void) => {
    const progress = new Channel<InstallProgress>();
    progress.onmessage = onProgress;
    return invoke<void>("update_install", { progress });
  },
  updateRestart: () => invoke<void>("update_restart"),
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  vaultCreate: (password: string) => invoke<void>("vault_create", { password }),
  vaultUnlock: (password: string) => invoke<void>("vault_unlock", { password }),
  vaultLock: () => invoke<void>("vault_lock"),
  /** Efface toutes les données ; renvoie les fichiers restés en place. */
  appReset: (password: string) => invoke<string[]>("app_reset", { password }),
  vaultTouch: () => invoke<void>("vault_touch"),
  vaultChangePassword: (current: string, next: string) =>
    invoke<void>("vault_change_password", { current, new: next }),

  /** Renvoie le nom du fichier écrit, ou null si l'utilisateur a annulé. */
  vaultExport: (password: string, passphrase: string, selection: ExportSelection) =>
    invoke<string | null>("vault_export", { password, passphrase, selection }),
  vaultImportPick: () => invoke<string | null>("vault_import_pick"),
  vaultImportPreview: (passphrase: string) => invoke<ImportPreview>("vault_import_preview", { passphrase }),
  vaultImportApply: (duplicates: Duplicates, importKnownHosts: boolean, importSettings: boolean) =>
    invoke<ImportReport>("vault_import_apply", { duplicates, importKnownHosts, importSettings }),
  vaultImportCancel: () => invoke<void>("vault_import_cancel"),

  hostsList: () => invoke<Host[]>("hosts_list"),
  hostSave: (host: HostInput) => invoke<Host>("host_save", { host }),
  hostDelete: (id: string) => invoke<void>("host_delete", { id }),

  identitiesList: () => invoke<Identity[]>("identities_list"),
  identitySave: (identity: IdentityInput) => invoke<Identity>("identity_save", { identity }),
  identityDelete: (id: string) => invoke<void>("identity_delete", { id }),
  /** Copie le mot de passe (par le backend) ; renvoie le délai d'effacement en secondes. */
  identityCopyPassword: (id: string) => invoke<number>("identity_copy_password", { id }),
  identityGenerateKey: (label: string, username: string | null) =>
    invoke<Identity>("identity_generate_key", { label, username }),

  sshOpen: (
    hostId: string,
    cols: number,
    rows: number,
    onEvent: (e: SessionEvent) => void,
    onData: (bytes: Uint8Array) => void,
  ) => {
    const events = new Channel<SessionEvent>();
    events.onmessage = onEvent;
    const data = new Channel<ArrayBuffer | number[]>();
    data.onmessage = (chunk) => onData(chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : Uint8Array.from(chunk));
    return invoke<string>("ssh_open", { hostId, cols, rows, events, data });
  },
  sshWrite: (sessionId: string, data: string) => invoke<void>("ssh_write", { sessionId, data }),
  /** Colle le presse-papiers dans la session (lu par le backend, jamais par l'interface). */
  sshPaste: (sessionId: string, bracketed: boolean) => invoke<void>("ssh_paste", { sessionId, bracketed }),
  sshResize: (sessionId: string, cols: number, rows: number) => invoke<void>("ssh_resize", { sessionId, cols, rows }),
  sshClose: (sessionId: string) => invoke<void>("ssh_close", { sessionId }),
  sshPromptAnswer: (promptId: number, answer: string[] | null) =>
    invoke<void>("ssh_prompt_answer", { promptId, answer }),
  /** Terminal local possible (pas dans le Flatpak). */
  localTermAvailable: () => invoke<boolean>("local_term_available"),
  localTermOpen: (
    cols: number,
    rows: number,
    onEvent: (e: SessionEvent) => void,
    onData: (bytes: Uint8Array) => void,
  ) => {
    const events = new Channel<SessionEvent>();
    events.onmessage = onEvent;
    const data = new Channel<ArrayBuffer | number[]>();
    data.onmessage = (chunk) => onData(chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : Uint8Array.from(chunk));
    return invoke<string>("local_term_open", { cols, rows, events, data });
  },
  localTermWrite: (id: string, data: string) => invoke<void>("local_term_write", { id, data }),
  localTermPaste: (id: string, bracketed: boolean) => invoke<void>("local_term_paste", { id, bracketed }),
  localTermResize: (id: string, cols: number, rows: number) => invoke<void>("local_term_resize", { id, cols, rows }),
  localTermClose: (id: string) => invoke<void>("local_term_close", { id }),
  sftpOpen: (hostId: string, onEvent: (e: SessionEvent) => void) => {
    const events = new Channel<SessionEvent>();
    events.onmessage = onEvent;
    return invoke<string>("sftp_open", { hostId, events });
  },
  sftpClose: (sftpId: string) => invoke<void>("sftp_close", { sftpId }),
  sftpList: (sftpId: string, path: string) => invoke<FileEntry[]>("sftp_list", { sftpId, path }),
  sftpRealpath: (sftpId: string, path: string) => invoke<string>("sftp_realpath", { sftpId, path }),
  sftpMkdir: (sftpId: string, dir: string, name: string) => invoke<void>("sftp_mkdir", { sftpId, dir, name }),
  sftpRename: (sftpId: string, path: string, newName: string) =>
    invoke<void>("sftp_rename", { sftpId, path, newName }),
  sftpDelete: (sftpId: string, paths: string[]) => invoke<void>("sftp_delete", { sftpId, paths }),
  sftpUpload: (sftpId: string, localPaths: string[], remoteDir: string, overwrite: boolean) =>
    invoke<string[]>("sftp_upload", { sftpId, localPaths, remoteDir, overwrite }),
  sftpDownload: (sftpId: string, remotePaths: string[], localDir: string, overwrite: boolean) =>
    invoke<string[]>("sftp_download", { sftpId, remotePaths, localDir, overwrite }),
  localHome: () => invoke<string>("local_home"),
  localList: (path: string) => invoke<FileEntry[]>("local_list", { path }),
  localRealpath: (path: string) => invoke<string>("local_realpath", { path }),
  localMkdir: (dir: string, name: string) => invoke<void>("local_mkdir", { dir, name }),
  localRename: (path: string, newName: string) => invoke<void>("local_rename", { path, newName }),
  localTrash: (paths: string[]) => invoke<void>("local_trash", { paths }),
  sftpStat: (sftpId: string, path: string) => invoke<FileEntry>("sftp_stat", { sftpId, path }),
  sftpChmod: (sftpId: string, path: string, mode: number) => invoke<void>("sftp_chmod", { sftpId, path, mode }),
  localStat: (path: string) => invoke<FileEntry>("local_stat", { path }),
  localChmod: (path: string, mode: number) => invoke<void>("local_chmod", { path, mode }),
  sftpReadText: (sftpId: string, path: string) => invoke<RemoteText>("sftp_read_text", { sftpId, path }),
  /** `expected` null : remplacement forcé (après un conflit). */
  sftpWriteText: (sftpId: string, path: string, content: string, encoding: TextEncoding, expected: RemoteVersion | null) =>
    invoke<RemoteVersion>("sftp_write_text", { sftpId, path, content, encoding, expected }),
  editsSubscribe: (onEvent: (e: EditEvent) => void) => {
    const events = new Channel<EditEvent>();
    events.onmessage = onEvent;
    return invoke<void>("edits_subscribe", { events });
  },
  sftpOpenExternal: (sftpId: string, path: string) => invoke<string>("sftp_open_external", { sftpId, path }),
  editExternalOverwrite: (id: string) => invoke<void>("edit_external_overwrite", { id }),
  editExternalStop: (id: string) => invoke<void>("edit_external_stop", { id }),
  transfersSubscribe: (onEvent: (e: TransferEvent) => void) => {
    const events = new Channel<TransferEvent>();
    events.onmessage = onEvent;
    return invoke<void>("transfers_subscribe", { events });
  },
  transferCancel: (transferId: string) => invoke<void>("transfer_cancel", { transferId }),
  oidcStatus: () => invoke<OidcStatus | null>("oidc_status"),
  oidcRedirectUri: (port: number) => invoke<string>("oidc_redirect_uri", { port }),
  oidcEnroll: (input: OidcEnrollInput, password: string) => invoke<OidcIdentity>("oidc_enroll", { input, password }),
  oidcDisable: () => invoke<void>("oidc_disable"),
  oidcUnlock: () => invoke<void>("oidc_unlock"),
  oidcCancel: () => invoke<void>("oidc_cancel"),
  oidcDeviceStart: () => invoke<OidcDeviceStart>("oidc_device_start"),
  oidcDeviceWait: () => invoke<void>("oidc_device_wait"),
  fido2List: () => invoke<Fido2Key[]>("fido2_list"),
  fido2Detect: () => invoke<Fido2Device>("fido2_detect"),
  fido2SetPin: (pin: string) => invoke<void>("fido2_set_pin", { pin }),
  fido2Enroll: (label: string, pin: string, password: string, onTouch: (touch: number) => void) => {
    const progress = new Channel<{ touch: number }>();
    progress.onmessage = (m) => onTouch(m.touch);
    return invoke<Fido2Key>("fido2_enroll", { label, pin, password, progress });
  },
  fido2Remove: (id: string) => invoke<void>("fido2_remove", { id }),
  fido2Unlock: (pin: string) => invoke<void>("fido2_unlock", { pin }),
  knownHostsList: () => invoke<KnownHost[]>("known_hosts_list"),
  knownHostDelete: (hostname: string, port: number) => invoke<void>("known_host_delete", { hostname, port }),

  settingsGet: () => invoke<Settings>("settings_get"),
  settingsSave: (settings: Settings) => invoke<Settings>("settings_save", { settings }),
};
