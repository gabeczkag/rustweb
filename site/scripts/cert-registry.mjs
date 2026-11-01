const DEVICE_KEY = "byrust.device.v1";

function seedNode() {
  return document.getElementById("cert-seed");
}

export function registryConfig() {
  const node = seedNode();
  return {
    url: (node?.dataset.registryUrl || "").replace(/\/+$/, ""),
    key: node?.dataset.registryKey || "",
  };
}

export function registryConfigured() {
  const { url, key } = registryConfig();
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

async function localRequest(path, options = {}) {
  try {
    const response = await fetch(path, {
      ...options,
      headers: {
        accept: "application/json",
        ...(options.headers || {}),
      },
    });
    const contentType = response.headers.get("content-type") || "";
    const data = await response.json().catch(() => null);

    if (contentType.includes("application/json") && data !== null) {
      return { available: true, ok: response.ok, data, response };
    }

    return { available: false, response };
  } catch {
    return { available: false };
  }
}

export async function fetchPublicEntries() {
  const local = await localRequest("/certinfo/", { method: "GET" });
  if (local.available) {
    if (local.ok && Array.isArray(local.data)) {
      return {
        status: "ok",
        entries: local.data.map(normalizePublicEntry),
      };
    }
    if (local.ok && Array.isArray(local.data?.entries)) {
      return {
        status: "ok",
        entries: local.data.entries.map(normalizePublicEntry),
      };
    }
    if (!local.ok) {
      return {
        status: "error",
        entries: [],
        error: errorMessage(local.data, local.response),
      };
    }
  }

  const { url, key } = registryConfig();
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
  const identity = await deviceIdentity();
  const payload = JSON.stringify({
    payload: entry,
    hwid: identity.hwid,
    fingerprint: identity.fingerprint,
  });

  const local = await localRequest("/certinfo/", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: payload,
  });

  if (local.available) {
    if (local.ok) {
      return { status: "ok", entry: normalizePublicEntry(local.data) };
    }

    const message = errorMessage(local.data, local.response);
    if (message.includes("LIMIT_REACHED")) {
      return { status: "limit", error: message };
    }
    return { status: "rejected", error: message };
  }

  const { url, key } = registryConfig();
  if (!url || !key) {
    return { status: "unconfigured", error: "REGISTRY_NOT_CONFIGURED" };
  }

  try {
    const response = await fetch(`${url}/rest/v1/rpc/register_certification`, {
      method: "POST",
      headers: {
        apikey: key,
        "content-type": "application/json",
        accept: "application/json",
      },
      body: payload,
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
