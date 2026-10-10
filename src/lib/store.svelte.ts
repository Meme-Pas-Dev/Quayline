import { api, errorCode, errorMessage, type Host, type Identity, type Settings, type TransferEvent } from "./api";
import { toast } from "./components/toast.svelte";

export const data = $state<{ hosts: Host[]; identities: Identity[]; settings: Settings | null; loaded: boolean }>({
  hosts: [],
  identities: [],
  settings: null,
  loaded: false,
});

export type View = "hosts" | "keychain" | "settings";
export type SessionStatus = "connecting" | "authenticating" | "connected" | "closed";

export interface SessionTab {
  key: string;
  kind: "ssh" | "sftp";
  hostId: string;
  label: string;
  color: string | null;
  status: SessionStatus;
  error: string | null;
  /** Une confirmation ou une saisie attend l'utilisateur. */
  pending: boolean;
  /** Incrémenté à chaque reconnexion pour recréer le terminal. */
  attempt: number;
  /** Incrémenté pour demander la déconnexion en gardant l'onglet ouvert. */
  disconnect: number;
}

/** Navigation : une vue (hôtes, trousseau, réglages) ou une session active. */
export const ui = $state<{ view: View; active: string | null; sessions: SessionTab[] }>({
  view: "hosts",
  active: null,
  sessions: [],
});

let onLocked: () => void = () => {};

export function setLockedHandler(fn: () => void) {
  onLocked = fn;
}

/** Erreur commune : si le coffre s'est verrouillé entre-temps, retour à l'écran de verrouillage. */
export function handleError(e: unknown) {
  if (errorCode(e) === "locked") onLocked();
  else toast(errorMessage(e), "error");
}

export async function reload() {
  try {
    const [hosts, identities, settings] = await Promise.all([
      api.hostsList(),
      api.identitiesList(),
      api.settingsGet(),
    ]);
    data.hosts = hosts;
    data.identities = identities;
    data.settings = settings;
    data.loaded = true;
  } catch (e) {
    handleError(e);
  }
}

export function resetSessionState() {
  data.hosts = [];
  data.identities = [];
  data.settings = null;
  data.loaded = false;
  ui.sessions = [];
  ui.active = null;
  ui.view = "hosts";
  transfers.length = 0;
}

export function identityName(id: string | null): string | null {
  return id ? (data.identities.find((i) => i.id === id)?.label ?? null) : null;
}

export function showView(view: View) {
  ui.view = view;
  ui.active = null;
}

export function openSession(host: Host, kind: "ssh" | "sftp" = "ssh") {
  const key = crypto.randomUUID();
  // Plusieurs sessions vers le même hôte : on numérote.
  const same = ui.sessions.filter((s) => s.hostId === host.id && s.kind === kind).length;
  ui.sessions.push({
    key,
    kind,
    hostId: host.id,
    label: same ? `${host.label} (${same + 1})` : host.label,
    color: host.color,
    status: "connecting",
    error: null,
    pending: false,
    attempt: 0,
    disconnect: 0,
  });
  ui.active = key;
}

/** Hôte d'une session (absent s'il a été supprimé depuis). */
export function sessionHost(key: string): Host | null {
  const s = ui.sessions.find((s) => s.key === key);
  return (s && data.hosts.find((h) => h.id === s.hostId)) ?? null;
}

/** Ouvre une nouvelle session vers le même hôte : même type, ou l'autre. */
export function openFromSession(key: string, kind: "same" | "other") {
  const s = ui.sessions.find((s) => s.key === key);
  const host = sessionHost(key);
  if (!s || !host) return;
  openSession(host, kind === "same" ? s.kind : s.kind === "ssh" ? "sftp" : "ssh");
}

/** Coupe la connexion mais garde l'onglet (et son contenu) pour pouvoir se reconnecter. */
export function disconnectSession(key: string) {
  const s = ui.sessions.find((s) => s.key === key);
  if (s && s.status === "connected") s.disconnect += 1;
}

export function closeSession(key: string) {
  const i = ui.sessions.findIndex((s) => s.key === key);
  if (i < 0) return;
  ui.sessions.splice(i, 1);
  if (ui.active === key) {
    ui.active = ui.sessions[Math.min(i, ui.sessions.length - 1)]?.key ?? null;
  }
}

export function reconnect(key: string) {
  const s = ui.sessions.find((s) => s.key === key);
  if (s) {
    s.status = "connecting";
    s.error = null;
    s.attempt += 1;
  }
}

// ───────────────────────────── Transferts SFTP ─────────────────────────────

export interface Transfer {
  id: string;
  sftpId: string;
  direction: "upload" | "download";
  name: string;
  destination: string;
  done: number;
  total: number;
  files: number;
  status: "running" | "done" | "error" | "cancelled";
  error: string | null;
  skipped: number;
}

export const transfers = $state<Transfer[]>([]);

/** Appelé par les explorateurs quand un transfert se termine (pour rafraîchir). */
const finishListeners = new Set<(t: Transfer) => void>();
export function onTransferFinished(fn: (t: Transfer) => void) {
  finishListeners.add(fn);
  return () => finishListeners.delete(fn);
}

export function subscribeTransfers() {
  return api.transfersSubscribe((e: TransferEvent) => {
    if (e.type === "started") {
      transfers.unshift({
        id: e.id,
        sftpId: e.sftpId,
        direction: e.direction,
        name: e.name,
        destination: e.destination,
        done: 0,
        total: 0,
        files: 0,
        status: "running",
        error: null,
        skipped: 0,
      });
      return;
    }
    const t = transfers.find((t) => t.id === e.id);
    if (!t) return;
    if (e.type === "progress") {
      t.done = e.done;
      t.total = e.total;
      t.files = e.files;
    } else {
      t.status = e.cancelled ? "cancelled" : e.error ? "error" : "done";
      t.error = e.error;
      t.skipped = e.skipped;
      finishListeners.forEach((fn) => fn(t));
    }
  });
}

export function clearFinishedTransfers(sftpId: string) {
  for (let i = transfers.length - 1; i >= 0; i--) {
    if (transfers[i].sftpId === sftpId && transfers[i].status !== "running") transfers.splice(i, 1);
  }
}
