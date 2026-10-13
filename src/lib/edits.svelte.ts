// Fichiers distants ouverts avec l'application par défaut du système : la copie locale
// est renvoyée au serveur à chaque enregistrement (voir sftp_edit.rs). Rien n'est
// affiché en permanence : le suivi s'arrête de lui-même (nouvelle ouverture du même
// fichier, fermeture de la session, verrouillage) ; seuls les envois, conflits et
// erreurs sont signalés.
import { api, errorMessage, type EditEvent } from "./api";
import { displayName } from "./format";
import { t } from "./i18n/index.svelte";
import { toast } from "./components/toast.svelte";

export type ExternalEdit = {
  id: string;
  sftpId: string;
  name: string;
  remote: string;
  status: "watching" | "synced" | "conflict" | "error";
  message: string | null;
};

export const externalEdits = $state<ExternalEdit[]>([]);

let subscribed: Promise<void> | null = null;

function onEvent(e: EditEvent) {
  if (e.kind === "opened") {
    if (!externalEdits.some((x) => x.id === e.id)) {
      externalEdits.push({ id: e.id, sftpId: e.sftpId, name: e.name, remote: e.remote, status: "watching", message: null });
      toast(t("editor.externalOpened", { name: displayName(e.name) }));
    }
    return;
  }
  const i = externalEdits.findIndex((x) => x.id === e.id);
  if (i < 0) return;
  const edit = externalEdits[i];
  switch (e.kind) {
    case "synced":
      edit.status = "synced";
      edit.message = null;
      toast(t("editor.saved", { name: displayName(edit.name) }), "success");
      break;
    case "conflict":
      edit.status = "conflict";
      toast(t("editor.externalConflictToast", { name: displayName(edit.name) }), "error", 6000);
      break;
    case "failed":
      edit.status = "error";
      edit.message = e.message;
      toast(`${displayName(edit.name)} : ${e.message}`, "error", 6000);
      break;
    case "closed":
      externalEdits.splice(i, 1);
      break;
  }
}

function ensureSubscribed() {
  subscribed ??= api.editsSubscribe(onEvent).catch((e) => {
    subscribed = null;
    throw e;
  });
  return subscribed;
}

export async function openExternal(sftpId: string, path: string) {
  try {
    await ensureSubscribed();
    await api.sftpOpenExternal(sftpId, path);
  } catch (e) {
    toast(errorMessage(e), "error", 6000);
  }
}

export function overwriteExternal(id: string) {
  const edit = externalEdits.find((x) => x.id === id);
  if (edit) edit.status = "watching";
  api.editExternalOverwrite(id).catch((e) => toast(errorMessage(e), "error"));
}

export function stopExternal(id: string) {
  api.editExternalStop(id).catch((e) => toast(errorMessage(e), "error"));
}
