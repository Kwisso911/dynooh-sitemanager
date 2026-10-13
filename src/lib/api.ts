import { invoke } from "@tauri-apps/api/core";

export interface Site {
  id: string;
  name: string;
  localPath: string;
  host: string;
  port: number;
  user: string;
  remotePath: string;
  passive: boolean;
  exclude: string[];
  webUrl: string;
  uploadOnSave: boolean;
  hasPassword: boolean;
  source: string;
  logins: Login[];
  connections: number;
  group: string;
}

export interface Login {
  id: string;
  label: string;
  url: string;
  user: string;
  note: string;
  hasPassword: boolean;
}

export interface Entry {
  name: string;
  isDir: boolean;
  size: number;
  mtime: number;
  link?: boolean;
}

export type SyncStatus =
  | "same"
  | "probablySame"
  | "different"
  | "upload"
  | "download"
  | "conflict"
  | "localOnly"
  | "remoteOnly"
  | "localDeleted"
  | "remoteDeleted";

export interface SyncItem {
  rel: string;
  status: SyncStatus;
  localSize: number | null;
  localMtime: number | null;
  remoteSize: number | null;
  remoteMtime: number | null;
}

export interface GitStatus {
  isRepo: boolean;
  root: string;
  branch: string;
  upstream: string;
  ahead: number;
  behind: number;
  remoteUrl: string;
  files: { code: string; path: string }[];
}

export interface ImportResult {
  added: number;
  updated: number;
  skipped: number;
}

export type Side = "local" | "remote";

export interface MasterStatus {
  configured: boolean;
  unlocked: boolean;
  /** Full address while unlocked, masked otherwise. */
  email: string;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  url: string;
  newer: boolean;
}

export interface JobInfo {
  id: number;
  siteId: string;
  label: string;
  state: "scan" | "running" | "done" | "error" | "cancelled";
  scanning: boolean;
  scanDir: string;
  total: number;
  done: number;
  failed: number;
  bytes: number;
  current: string[];
  errors: string[];
}

export interface FsChanged {
  siteId: string;
  side: Side;
  dir: string;
}

export interface Progress {
  siteId: string;
  kind: "scan" | "upload" | "download" | "copy" | "done";
  rel: string;
  index: number;
  total: number;
  error: string | null;
}

export const api = {
  appReady: () => invoke<void>("app_ready"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  masterStatus: () => invoke<MasterStatus>("master_status"),
  masterSetup: (password: string, email: string) => invoke<void>("master_setup", { password, email }),
  masterUnlock: (password: string) => invoke<void>("master_unlock", { password }),
  masterLock: () => invoke<void>("master_lock"),
  masterChange: (current: string, password: string | null, email: string | null) =>
    invoke<void>("master_change", { current, password, email }),
  masterForgot: (lang: string) => invoke<string>("master_forgot", { lang }),
  masterReset: (code: string, password: string) => invoke<void>("master_reset", { code, password }),
  sitesList: () => invoke<Site[]>("sites_list"),
  siteSave: (site: Site, password?: string) => invoke<Site>("site_save", { site, password: password || null }),
  siteDelete: (id: string) => invoke<void>("site_delete", { id }),
  siteSetGroup: (ids: string[], group: string) => invoke<void>("site_set_group", { ids, group }),
  siteTest: (site: Site, password?: string) => invoke<string>("site_test", { site, password: password || null }),
  loginSave: (siteId: string, login: Login, password?: string) =>
    invoke<Site>("login_save", { siteId, login, password: password || null }),
  loginDelete: (siteId: string, loginId: string) => invoke<Site>("login_delete", { siteId, loginId }),
  loginPassword: (siteId: string, loginId: string) => invoke<string>("login_password", { siteId, loginId }),
  ftpPassword: (siteId: string) => invoke<string>("ftp_password", { siteId }),
  importDreamweaver: () => invoke<ImportResult>("import_dreamweaver"),
  importFolders: (root: string) => invoke<ImportResult>("import_folders", { root }),
  exportSites: (ids: string[], path: string, password: string) => invoke<number>("export_sites", { ids, path, password }),
  importSites: (path: string, password: string) => invoke<ImportResult>("import_sites", { path, password }),
  langInstall: (path: string) => invoke<string>("lang_install", { path }),
  langTemplate: (strings: string[]) => invoke<string>("lang_template", { strings }),

  localList: (siteId: string, rel: string) => invoke<Entry[]>("local_list", { siteId, rel }),
  remoteList: (siteId: string, rel: string) => invoke<Entry[]>("remote_list", { siteId, rel }),
  upload: (siteId: string, rels: string[]) => invoke<number>("upload", { siteId, rels }),
  download: (siteId: string, rels: string[]) => invoke<number>("download", { siteId, rels }),
  dropPaths: (siteId: string, paths: string[], side: Side, dest: string) =>
    invoke<number>("drop_paths", { siteId, paths, side, dest }),
  cancelJob: (id: number) => invoke<void>("cancel_job", { id }),
  cancelAllJobs: () => invoke<void>("cancel_all_jobs"),
  remoteDelete: (siteId: string, rel: string, isDir: boolean) => invoke<void>("remote_delete", { siteId, rel, isDir }),
  remoteMkdir: (siteId: string, rel: string) => invoke<void>("remote_mkdir", { siteId, rel }),
  remoteRename: (siteId: string, from: string, to: string) => invoke<void>("remote_rename", { siteId, from, to }),
  localDelete: (siteId: string, rels: string[]) => invoke<void>("local_delete", { siteId, rels }),
  localMkdir: (siteId: string, rel: string) => invoke<void>("local_mkdir", { siteId, rel }),
  localRename: (siteId: string, from: string, to: string) => invoke<void>("local_rename", { siteId, from, to }),
  copyItems: (siteId: string, from: Side, items: { rel: string; isDir: boolean }[], to: Side, dest: string) =>
    invoke<number>("copy_items", { siteId, from, items, to, dest }),
  revealPath: (siteId: string, rel: string) => invoke<string>("reveal_path", { siteId, rel }),
  openFolder: (siteId: string) => invoke<void>("open_folder", { siteId }),
  disconnect: (siteId: string) => invoke<void>("disconnect", { siteId }),

  syncCompare: (siteId: string, rel = "") => invoke<SyncItem[]>("sync_compare", { siteId, rel }),
  syncApply: (siteId: string, uploads: string[], downloads: string[], deletes: { rel: string; side: Side; forget: boolean }[] = []) =>
    invoke<number>("sync_apply", { siteId, uploads, downloads, deletes }),
  syncMarkSame: (siteId: string, rels: string[]) => invoke<number>("sync_mark_same", { siteId, rels }),

  fileRead: (siteId: string, rel: string) => invoke<string>("file_read", { siteId, rel }),
  fileWrite: (siteId: string, rel: string, content: string) => invoke<void>("file_write", { siteId, rel, content }),
  localPath: (siteId: string, rel: string) => invoke<string>("local_path", { siteId, rel }),

  gitStatus: (siteId: string) => invoke<GitStatus>("git_status", { siteId }),
  gitCommit: (siteId: string, message: string) => invoke<string>("git_commit", { siteId, message }),
  gitPush: (siteId: string) => invoke<string>("git_push", { siteId }),
  gitPull: (siteId: string) => invoke<string>("git_pull", { siteId }),
  gitInit: (siteId: string) => invoke<string>("git_init", { siteId }),
};

export function joinRel(base: string, name: string): string {
  return base ? `${base}/${name}` : name;
}

/** Folder names hosting providers use as document root; the URL path starts below them. */
const DOC_ROOTS = new Set(["httpdocs", "httpsdocs", "public_html", "htdocs", "www", "web", "html"]);

const segments = (path: string) => path.split(/[\\/]+/).filter(Boolean);

/**
 * Browser address of a server path. When the FTP folder lies below the document root
 * (e.g. /httpdocs/river/), that part is added to the web URL unless it already ends with it.
 */
export function webUrlFor(site: Site, rel = ""): string | null {
  const raw = site.webUrl.trim();
  if (!raw) return null;
  let url: URL;
  try {
    url = new URL(/^[a-z][a-z\d+.-]*:\/\//i.test(raw) ? raw : `https://${raw}`);
  } catch {
    return null;
  }
  let base = segments(decodeURIComponent(url.pathname));
  if (base.length && DOC_ROOTS.has(base[0].toLowerCase())) base = base.slice(1);
  const remote = segments(site.remotePath);
  const root = remote.findIndex((s) => DOC_ROOTS.has(s.toLowerCase()));
  const below = root >= 0 ? remote.slice(root + 1) : [];
  const covered = below.length <= base.length && below.every((s, i) => base[base.length - below.length + i] === s);
  const path = [...base, ...(covered ? [] : below), ...segments(rel)];
  return `${url.origin}/${path.map(encodeURIComponent).join("/")}`;
}
