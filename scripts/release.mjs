import { execFileSync, execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";

// Local release build. Output stays outside Dropbox, a synced folder gets locked while it is rewritten.
const webOut = "C:/Dev/vf-sitemanager-web";
const target = "C:/Dev/vf-sitemanager-target/release";
const out = join(import.meta.dirname, "..", "release");

const tokenFile = join(import.meta.dirname, ".mail-token");
const token = process.env.VFSM_MAIL_TOKEN || (existsSync(tokenFile) ? readFileSync(tokenFile, "utf8").trim() : "");
if (!token) console.warn("Kein VFSM_MAIL_TOKEN: Passwort-Reset per E-Mail ist in diesem Build deaktiviert.");

const overlay = join(tmpdir(), "vf-sitemanager.local.conf.json");
// Relative on purpose: Tauri parses "C:/…" as a URL with scheme "c" and then embeds no frontend.
const frontendDist = relative(join(import.meta.dirname, "..", "src-tauri"), webOut);
writeFileSync(overlay, JSON.stringify({ build: { frontendDist } }));

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

// Signing is optional: without a certificate the build stays unsigned.
// VFSM_SIGN_THUMBPRINT selects a certificate explicitly, otherwise the newest valid
// "Open Source Developer" code signing certificate in CurrentUser\My is used (Certum, via SimplySign Desktop).
function signtool() {
  const kits = "C:/Program Files (x86)/Windows Kits/10/bin";
  if (!existsSync(kits)) return null;
  const versions = readdirSync(kits).filter((d) => /^10\./.test(d)).sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  for (const v of versions) {
    const exe = join(kits, v, "x64", "signtool.exe");
    if (existsSync(exe)) return exe;
  }
  return null;
}

function thumbprint() {
  if (process.env.VFSM_SIGN_THUMBPRINT) return process.env.VFSM_SIGN_THUMBPRINT.replace(/\s/g, "");
  const ps =
    "Get-ChildItem Cert:\\CurrentUser\\My -CodeSigningCert | " +
    "Where-Object { $_.Subject -like '*Open Source Developer*' -and $_.NotAfter -gt (Get-Date) -and $_.HasPrivateKey } | " +
    "Sort-Object NotAfter -Descending | Select-Object -First 1 -ExpandProperty Thumbprint";
  try {
    return execFileSync("powershell", ["-NoProfile", "-Command", ps], { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

const thumb = thumbprint();
const tool = thumb ? signtool() : null;
if (!thumb) {
  console.warn("Kein Signaturzertifikat gefunden: release/ bleibt unsigniert.");
} else if (!tool) {
  console.error("Zertifikat vorhanden, aber signtool.exe fehlt (Windows SDK installieren).");
  process.exit(1);
} else {
  for (const [, name] of files) {
    const file = join(out, name);
    execFileSync(tool, ["sign", "/sha1", thumb, "/fd", "sha256", "/tr", "http://time.certum.pl", "/td", "sha256", "/d", "Dynooh SiteManager", "/du", "https://apps.dynooh.com", file], { stdio: "inherit" });
    execFileSync(tool, ["verify", "/pa", file], { stdio: "inherit" });
    console.log(`signiert: release/${name}`);
  }
}
