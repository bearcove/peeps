import createDebug from "debug";

const defaultNamespaces = "moire:*";
const configured =
  (import.meta.env.VITE_MOIRE_DEBUG as string | undefined)?.trim() || defaultNamespaces;

if (typeof window !== "undefined" && import.meta.env.DEV) {
  createDebug.enable(configured);
}

export const appLog = createDebug("moire:app");
export const apiLog = createDebug("moire:api");
export const snapshotLog = createDebug("moire:snapshot");
