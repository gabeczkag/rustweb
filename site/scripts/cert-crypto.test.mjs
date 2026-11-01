import { registerApp, checkEntry, MARKERS } from "./cert-crypto.mjs";

const { entry, password } = await registerApp("NodeTest", "");

if (password.length !== 128) {
  throw new Error(`password length ${password.length}`);
}

if ("uuid" in entry) {
  throw new Error("UUID leaked into the public entry");
}

const result = await checkEntry(entry, password);

if (!result.valid) {
  throw new Error(`valid check failed: ${result.error}`);
}

if (!MARKERS.includes(result.marker)) {
  throw new Error(`unexpected marker ${result.marker}`);
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
