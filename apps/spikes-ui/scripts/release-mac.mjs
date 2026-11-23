// Builds the macOS app and disk image: signed and notarized when the Apple credentials are in the environment,
// unsigned (for this machine only) otherwise.
//
//   APPLE_SIGNING_IDENTITY  "Developer ID Application: Name (TEAMID)" (from `security find-identity -v -p codesigning`)
//   Notarization, either:   APPLE_ID, APPLE_PASSWORD (an app-specific password), APPLE_TEAM_ID
//                  or:      APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH (an App Store Connect API key)
//
// Tauri reads these itself; this script says which kind of build you're getting, and where it went.
import { spawnSync } from "node:child_process";

const env = process.env;
const signed = Boolean(env.APPLE_SIGNING_IDENTITY);
const notarized = signed && Boolean((env.APPLE_ID && env.APPLE_PASSWORD && env.APPLE_TEAM_ID) || (env.APPLE_API_ISSUER && env.APPLE_API_KEY && env.APPLE_API_KEY_PATH));
console.log(
  signed
    ? `Signing as ${env.APPLE_SIGNING_IDENTITY}; ${notarized ? "notarizing" : "not notarizing (no APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID or API key)"}.`
    : "No APPLE_SIGNING_IDENTITY: an unsigned build, which opens only on this machine (others: right-click > Open).",
);
const bundles = process.argv.includes("--app-only") ? "app" : "app,dmg";
const result = spawnSync("npx", ["tauri", "build", "--bundles", bundles], { stdio: "inherit", env });
if (result.status === 0) console.log("Built: target/release/bundle/macos (and dmg/)");
process.exit(result.status ?? 1);
