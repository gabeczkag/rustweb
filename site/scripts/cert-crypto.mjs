export const ITERATIONS = 210000;
export const MARKERS = ["POPRAWNE", "as10sign edi5nbaseApro3ved"];

const ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

export function randomBytes(length) {
  return crypto.getRandomValues(new Uint8Array(length));
}

export function toBase64(bytes) {
  let binary = "";
  const chunk = 0x8000;

  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk));
  }

  return btoa(binary);
}

export function fromBase64(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);

  for (let index = 0; index < binary.length; index++) {
    bytes[index] = binary.charCodeAt(index);
  }

  return bytes;
}

export function toHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function sha256(value) {
  const bytes =
    typeof value === "string" ? new TextEncoder().encode(value) : value;
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

export function generatePassword() {
  const bytes = randomBytes(128);
  let password = "";

  for (const byte of bytes) {
    password += ALPHABET[byte & 63];
  }

  return password;
}

async function deriveKey(password, salt, iterations = ITERATIONS) {
  const material = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(password),
    "PBKDF2",
    false,
    ["deriveKey"],
  );

  return crypto.subtle.deriveKey(
    {
      name: "PBKDF2",
      salt,
      iterations,
      hash: "SHA-256",
    },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

export function validUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    value,
  );
}

export async function registerApp(appName, uuidInput = "") {
  const name = String(appName || "").trim();
  if (!name) throw new Error("APP_NAME_REQUIRED");

  let uuid = String(uuidInput || "").trim().toLowerCase();
  if (!uuid) uuid = crypto.randomUUID();
  if (!validUuid(uuid)) throw new Error("UUID_INVALID");

  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );

  const publicKey = new Uint8Array(
    await crypto.subtle.exportKey("spki", pair.publicKey),
  );
  const privateKey = new Uint8Array(
    await crypto.subtle.exportKey("pkcs8", pair.privateKey),
  );

  const password = generatePassword();
  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = await deriveKey(password, salt);
  const marker = MARKERS[crypto.getRandomValues(new Uint8Array(1))[0] & 1];
  const appId = crypto.randomUUID();
  const payload = {
    uuid,
    private_key: toBase64(privateKey),
    marker,
  };
  const ciphertext = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: "AES-GCM", iv },
      key,
      new TextEncoder().encode(JSON.stringify(payload)),
    ),
  );

  const saltBase64 = toBase64(salt);
  const fingerprint = toHex(await sha256(publicKey));
  const commitment = toHex(
    await sha256(`byrust-uuid-v1:${uuid}:${saltBase64}`),
  );

  return {
    password,
    entry: {
      version: 1,
      app_id: appId,
      app: name,
      public_key: toBase64(publicKey),
      key_fingerprint: fingerprint,
      uuid_commitment: commitment,
      salt: saltBase64,
      iv: toBase64(iv),
      iterations: ITERATIONS,
      ciphertext: toBase64(ciphertext),
      status: "SEALED",
      created: new Date().toISOString(),
      trust: "app",
      algo: "AES-256-GCM/PBKDF2-SHA256/ECDSA-P-256",
    },
  };
}

export async function checkEntry(entry, password) {
  try {
    const salt = fromBase64(entry.salt);
    const iv = fromBase64(entry.iv);
    const iterations = Number(entry.iterations || ITERATIONS);
    const key = await deriveKey(String(password || ""), salt, iterations);
    const plaintext = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv },
      key,
      fromBase64(entry.ciphertext),
    );
    const payload = JSON.parse(new TextDecoder().decode(plaintext));

    if (!MARKERS.includes(payload.marker)) {
      return { valid: false, error: "MARKER_INVALID" };
    }

    const commitment = toHex(
      await sha256(`byrust-uuid-v1:${payload.uuid}:${entry.salt}`),
    );
    if (commitment !== entry.uuid_commitment) {
      return { valid: false, error: "COMMITMENT_MISMATCH" };
    }

    const fingerprint = toHex(
      await sha256(fromBase64(entry.public_key)),
    );
    if (fingerprint !== entry.key_fingerprint) {
      return { valid: false, error: "FINGERPRINT_MISMATCH" };
    }

    const privateKey = await crypto.subtle.importKey(
      "pkcs8",
      fromBase64(payload.private_key),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["sign"],
    );
    const publicKey = await crypto.subtle.importKey(
      "spki",
      fromBase64(entry.public_key),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    const challenge = new TextEncoder().encode(
      `byrust-check:${entry.uuid_commitment}`,
    );
    const signature = new Uint8Array(
      await crypto.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        privateKey,
        challenge,
      ),
    );
    const valid = await crypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      publicKey,
      signature,
      challenge,
    );

    return {
      valid,
      error: valid ? "" : "SIGNATURE_INVALID",
      uuid: payload.uuid,
      marker: payload.marker,
      fingerprint,
      private_key: payload.private_key,
    };
  } catch (error) {
    return {
      valid: false,
      error:
        error?.name === "OperationError"
          ? "WRONG_PASSWORD"
          : String(error),
    };
  }
}

export async function selfTest() {
  const { entry, password } = await registerApp("SelfTest", crypto.randomUUID());
  const result = await checkEntry(entry, password);
  return { entry, password, ...result };
}
