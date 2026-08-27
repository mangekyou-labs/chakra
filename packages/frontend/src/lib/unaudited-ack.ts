/**
 * Unaudited-contract acknowledgement (T6.3).
 *
 * Key: `chakra:unaudited-ack:v1` → ISO timestamp.
 * The modal appears once before the first swap; user must Ack to proceed.
 */

export const UNAUDITED_ACK_KEY = 'chakra:unaudited-ack:v1';

function safeStorage(): Storage | null {
  try {
    if (typeof localStorage !== 'undefined') return localStorage;
  } catch {
    // SSR / node / storage-disabled
  }
  return null;
}

/** True when the user has acknowledged the unaudited-contract warning. */
export function hasAck(): boolean {
  const storage = safeStorage();
  if (!storage) return false;
  try {
    const raw = storage.getItem(UNAUDITED_ACK_KEY);
    if (!raw) return false;
    // Validate it's a parseable ISO timestamp
    return Number.isFinite(Date.parse(raw));
  } catch {
    return false;
  }
}

/** Record the user's acknowledgement (once-only gate). */
export function recordAck(): void {
  const storage = safeStorage();
  if (!storage) return;
  try {
    storage.setItem(UNAUDITED_ACK_KEY, new Date().toISOString());
  } catch {
    // Storage full — silently ignore.
  }
}
