import { registerApp, checkEntry, MARKERS } from "./cert-crypto.mjs";

const { entry, password } = await registerApp("NodeTest", "");

if (password.length !== 128) {
  throw new Error(`password length ${password.length}`);
}

if ("uuid" in entry) {
  throw new Error("UUID leaked into the public entry");
}

if (!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(entry.app_id)) {
  throw new Error(`invalid public app_id ${entry.app_id}`);
}

const result = await checkEntry(entry, password);

if (!result.valid) {
  throw new Error(`valid check failed: ${result.error}`);
}

if (!MARKERS.includes(result.marker)) {
  throw new Error(`unexpected marker ${result.marker}`);
}

if ("private_key" in result) {
  throw new Error("decrypted private key leaked into the checker result");
}

if ("algo" in entry) {
  throw new Error("algorithm metadata leaked into the public entry");
}

const wrong = await checkEntry(entry, "x".repeat(128));
if (wrong.valid) {
  throw new Error("wrong password accepted");
}

console.log(
  "CERT CRYPTO OK",
  result.uuid,
  result.marker,
  entry.key_fingerprint.slice(0, 16),
);
