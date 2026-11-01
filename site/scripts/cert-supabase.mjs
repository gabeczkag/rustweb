const DEVICE_KEY = "byrust.device.v1";

function seedNode() {
  return document.getElementById("cert-seed");
}

export function supabaseConfig() {
  const node = seedNode();
  return {
    url: (node?.dataset.supabaseUrl || "").replace(/\/+$/, ""),
    key: node?.dataset.supabaseKey || "",
  };
}

export function supabaseConfigured() {
  const { url, key } = supabaseConfig();
  return Boolean(url && key);
}

function toHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function sha256Hex(value) {
  const bytes = new TextEncoder().encode(value);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return toHex(new Uint8Array(digest));
}

function deviceId() {
  try {
    const existing = localStorage.getItem(DEVICE_KEY);
    if (existing) return existing;
    const created = crypto.randomUUID();
    localStorage.setItem(DEVICE_KEY, created);
    return created;
  } catch {
    return crypto.randomUUID();
  }
}

export async function deviceIdentity() {
  const hwid = deviceId();
  const signals = [
    navigator.userAgent,
    navigator.language,
    navigator.hardwareConcurrency || 0,
    screen.width || 0,
    screen.height || 0,
    screen.colorDepth || 0,
    Intl.DateTimeFormat().resolvedOptions().timeZone || "",
    hwid,
  ].join("|");

  return { hwid, fingerprint: await sha256Hex(signals) };
}

function errorMessage(data, response) {
  return (
    data?.message ||
    data?.details ||
    data?.hint ||
    data?.error ||
    `HTTP ${response.status}`
  );
}

export function normalizePublicEntry(entry) {
  if (!entry) return entry;

  const normalized = { ...entry };
  if (!normalized.ciphertext && normalized.encrypted_key) {
    normalized.ciphertext = normalized.encrypted_key;
  }
  return normalized;
}

export async function fetchPublicEntries() {
  const { url, key } = supabaseConfig();
  if (!url || !key) {
    return { status: "unconfigured", entries: [] };
  }

  try {
    const response = await fetch(`${url}/rest/v1/rpc/get_certinfo`, {
      method: "POST",
      headers: {
        apikey: key,
        "content-type": "application/json",
        accept: "application/json",
      },
      body: "{}",
    });

    const data = await response.json().catch(() => null);
    if (!response.ok) {
      return {
        status: response.status === 404 ? "offline" : "error",
        entries: [],
        error: errorMessage(data, response),
      };
    }

    return {
      status: "ok",
      entries: Array.isArray(data) ? data.map(normalizePublicEntry) : [],
    };
  } catch (error) {
    return { status: "offline", entries: [], error: String(error) };
  }
}

export async function publishEntry(entry) {
  const { url, key } = supabaseConfig();
  if (!url || !key) {
    return { status: "unconfigured", error: "SUPABASE_NOT_CONFIGURED" };
  }

  try {
    const identity = await deviceIdentity();
    const response = await fetch(`${url}/rest/v1/rpc/register_certification`, {
      method: "POST",
      headers: {
        apikey: key,
        "content-type": "application/json",
        accept: "application/json",
      },
      body: JSON.stringify({
        payload: entry,
        hwid: identity.hwid,
        fingerprint: identity.fingerprint,
      }),
    });

    const data = await response.json().catch(() => null);
    if (response.ok) {
      return { status: "ok", entry: normalizePublicEntry(data) };
    }

    const message = errorMessage(data, response);
    if (message.includes("LIMIT_REACHED")) {
      return { status: "limit", error: message };
    }
    if (response.status === 404 || response.status === 401 || response.status === 403) {
      return { status: "offline", error: message };
    }
    return { status: "rejected", error: message };
  } catch (error) {
    return { status: "offline", error: String(error) };
  }
}
