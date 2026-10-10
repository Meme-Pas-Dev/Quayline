export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let next = 0;
export const toasts = $state<Toast[]>([]);

export function toast(message: string, kind: ToastKind = "info", ms = 3500) {
  const id = ++next;
  toasts.push({ id, kind, message });
  setTimeout(() => {
    const i = toasts.findIndex((t) => t.id === id);
    if (i >= 0) toasts.splice(i, 1);
  }, ms);
}
