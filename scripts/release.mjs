import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Local release build. Output stays outside Dropbox, a synced folder gets locked while it is rewritten.
const webOut = "C:/Dev/vf-sitemanager-web";
const target = "C:/Dev/vf-sitemanager-target/release";
const out = join(import.meta.dirname, "..", "release");

const tokenFile = join(import.meta.dirname, ".mail-token");
const token = process.env.VFSM_MAIL_TOKEN || (existsSync(tokenFile) ? readFileSync(tokenFile, "utf8").trim() : "");
if (!token) console.warn("Kein VFSM_MAIL_TOKEN: Passwort-Reset per E-Mail ist in diesem Build deaktiviert.");

const overlay = join(tmpdir(), "vf-sitemanager.local.conf.json");
writeFileSync(overlay, JSON.stringify({ build: { frontendDist: webOut } }));

execSync(`npx tauri build --no-bundle --config "${overlay}"`, {
  stdio: "inherit",
  env: { ...process.env, VFSM_WEB_OUT: webOut, VFSM_MAIL_TOKEN: token, CARGO_TARGET_DIR: "C:/Dev/vf-sitemanager-target" },
});

mkdirSync(out, { recursive: true });
const files = [[join(target, "vf-sitemanager.exe"), "Dynooh SiteManager.exe"]];
let failed = false;
for (const [from, name] of files) {
  try {
    copyFileSync(from, join(out, name));
    console.log(`kopiert: release/${name}`);
  } catch (e) {
    failed = true;
    console.error(`NICHT kopiert (läuft die App noch?): release/${name} – ${e.code ?? e.message}`);
  }
}
if (failed) process.exit(1);
