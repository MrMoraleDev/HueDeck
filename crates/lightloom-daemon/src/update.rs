// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings → Updates: is there a newer Lightloom release?
//!
//! Nothing here runs by itself unless the user turned the weekly check on
//! (off by default). A check is one anonymous HTTPS request to the GitHub
//! REST API, `GET /repos/MrMoraleDev/Lightloom/releases/latest` (with the beta
//! channel on, `.../releases?per_page=20`, so pre-releases count), with
//! `User-Agent: Lightloom/<version>`, no token, no cookie and no identifier
//! (docs/privacy.md). The newest release whose version is above the running
//! one is offered, with its notes as plain text and the download for this
//! system: the per-user MSI on Windows, the AppImage (else the tarball) on
//! Linux.
//!
//! *Download* happens only on the user's click. With a per-user MSI install
//! (`%LOCALAPPDATA%\Programs\Lightloom`) the daemon downloads the MSI to
//! `%TEMP%\lightloom-update`, checks its size and its SHA-256 against the
//! release's `SHA256SUMS` (refusing a release without one) and opens it with
//! `msiexec /i` (no elevation: the package is per-user). The installer stops
//! the running daemon itself before it replaces files (`lightloomw --stop`,
//! the package's `StopLightloom` action, so plugins restore their devices) and
//! starts the new version from its Finish page; cancelling it leaves this
//! version running. Elsewhere (a portable copy, Linux) the dashboard opens the
//! release page instead.
//!
//! The options and the last check's time live in `update.json` in the data
//! directory.

use lightloom_api::model::{UpdateAsset, UpdateJob, UpdateRelease, UpdateView};
use lightloom_api::{ApiError, Updates};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The repository releases come from.
pub const REPOSITORY: &str = "MrMoraleDev/Lightloom";
/// The latest stable release (GitHub leaves drafts and pre-releases out).
pub const LATEST_URL: &str = "https://api.github.com/repos/MrMoraleDev/Lightloom/releases/latest";
/// The newest releases, pre-releases included (beta channel).
pub const RELEASES_URL: &str = "https://api.github.com/repos/MrMoraleDev/Lightloom/releases?per_page=20";
/// Release pages and downloads must be under this.
const RELEASES_PREFIX: &str = "https://github.com/MrMoraleDev/Lightloom/releases/";
/// The `User-Agent` of every request.
pub const USER_AGENT: &str = concat!("Lightloom/", env!("CARGO_PKG_VERSION"));
/// How long an API request may take in all.
pub const API_TIMEOUT: Duration = Duration::from_secs(20);
/// How long an installer download may take in all.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// A weekly check runs when the last one is older than this.
pub const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
/// How often the weekly schedule looks at the clock (the first look comes
/// [`FIRST_LOOK`] after start).
const SCHEDULE_TICK: Duration = Duration::from_secs(3600);
const FIRST_LOOK: Duration = Duration::from_secs(120);

const MAX_API_BODY: u64 = 4 << 20;
const MAX_SUMS_BODY: u64 = 1 << 20;
const MAX_ASSET: u64 = 1 << 30;
/// Release notes longer than this (characters) are cut.
const MAX_NOTES: usize = 20_000;

/// A failed request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The server answered with this status (not 200).
    Status(u16),
    /// No answer (offline, DNS, TLS, timeout, body too long, ...).
    Failed(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Status(s) => write!(f, "HTTP {s}"),
            HttpError::Failed(e) => f.write_str(e),
        }
    }
}

/// The network, as the updater sees it (tests script it).
pub trait Http: Send + Sync {
    /// GET `url` (redirects followed), its body into `sink`, at most `max`
    /// bytes; `progress` gets the bytes so far. Answers with the length.
    fn get(
        &self,
        url: &str,
        accept: &str,
        max: u64,
        sink: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, HttpError>;
}

/// [`Http`] with ureq (rustls, Mozilla's root certificates; HTTPS only, the
/// `HTTPS_PROXY` / `ALL_PROXY` environment proxy).
pub struct UreqHttp {
    api: ureq::Agent,
    download: ureq::Agent,
}

impl UreqHttp {
    /// HTTPS only.
    pub fn new() -> Self {
        Self::with(true)
    }

    /// `https_only: false` lets tests talk to a plain-HTTP loopback server.
    pub fn with(https_only: bool) -> Self {
        let agent = |global: Duration| {
            ureq::Agent::config_builder()
                .https_only(https_only)
                .user_agent(USER_AGENT)
                .timeout_global(Some(global))
                .timeout_connect(Some(Duration::from_secs(15)))
                .timeout_recv_response(Some(Duration::from_secs(30)))
                .http_status_as_error(false)
                .max_redirects(5)
                .build()
                .new_agent()
        };
        Self {
            api: agent(API_TIMEOUT),
            download: agent(DOWNLOAD_TIMEOUT),
        }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn get(
        &self,
        url: &str,
        accept: &str,
        max: u64,
        sink: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, HttpError> {
        let agent = if max > MAX_API_BODY {
            &self.download
        } else {
            &self.api
        };
        let mut response = agent
            .get(url)
            .header("Accept", accept)
            .call()
            .map_err(|e| HttpError::Failed(e.to_string()))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(HttpError::Status(status));
        }
        let mut reader = response.body_mut().with_config().limit(max).reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n =
                std::io::Read::read(&mut reader, &mut buf).map_err(|e| HttpError::Failed(e.to_string()))?;
            if n == 0 {
                return Ok(total);
            }
            sink.write_all(&buf[..n])
                .map_err(|e| HttpError::Failed(format!("writing: {e}")))?;
            total += n as u64;
            progress(total);
        }
    }
}

/// GET a small body.
fn get_bytes(http: &dyn Http, url: &str, accept: &str, max: u64) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    http.get(url, accept, max, &mut body, &mut |_| {})?;
    Ok(body)
}

/// Which download fits this system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// The per-user MSI.
    Windows,
    /// The AppImage, else the tarball.
    Linux,
    /// None: the release page.
    Other,
}

impl Os {
    /// This system.
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Other
        }
    }
}

// ------------------------------------------------------------ GitHub's answer

#[derive(Clone, Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GhAsset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
}

/// What a check found: the release and where its checksums are.
#[derive(Clone, Debug, PartialEq)]
struct Found {
    release: UpdateRelease,
    /// `SHA256SUMS`, `SHA256SUMS-linux`, `<asset>.sha256` of that release.
    sums: Vec<UpdateAsset>,
}

/// `v1.2.3` / `1.2.3` → the version.
fn tag_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// A file name the update may be saved under: letters, digits, `.`, `-`, `_`, `+`.
fn plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

fn asset_ok(a: &GhAsset) -> bool {
    plain_file_name(&a.name)
        && a.browser_download_url
            .strip_prefix(RELEASES_PREFIX)
            .is_some_and(|rest| rest.starts_with("download/"))
}

/// The download for `os` among `assets` (`version` names the MSI).
fn pick_asset(os: Os, version: &Version, assets: &[GhAsset]) -> Option<UpdateAsset> {
    let assets: Vec<&GhAsset> = assets.iter().filter(|a| asset_ok(a)).collect();
    let find = |pred: &dyn Fn(&str) -> bool| assets.iter().find(|a| pred(&a.name)).copied();
    let chosen = match os {
        Os::Windows => {
            // build.ps1: Lightloom-<x.y.z>-x64.msi (English), Lightloom-<x.y.z>-x64-<culture>.msi.
            let exact = format!(
                "Lightloom-{}.{}.{}-x64.msi",
                version.major, version.minor, version.patch
            );
            find(&|n| n == exact).or_else(|| find(&|n| n.ends_with("-x64.msi")))
        }
        Os::Linux => find(&|n| n.ends_with("x86_64.AppImage"))
            .or_else(|| find(&|n| n.ends_with("-linux-x86_64.tar.xz")))
            .or_else(|| find(&|n| n.ends_with("x86_64-unknown-linux-gnu.tar.gz"))),
        Os::Other => None,
    }?;
    Some(UpdateAsset {
        name: chosen.name.clone(),
        size: chosen.size,
        url: chosen.browser_download_url.clone(),
    })
}

/// Release notes as plain text: no HTML tags or comments, no control
/// characters but newlines and tabs, at most a blank line in a row, at most
/// [`MAX_NOTES`] characters. The dashboard shows it as text, never as markup.
pub fn plain_notes(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<'
            && chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?'))
        {
            // A tag or comment: skip to its end.
            let comment = {
                let mut ahead = chars.clone();
                ahead.next() == Some('!') && ahead.next() == Some('-') && ahead.next() == Some('-')
            };
            if comment {
                let mut tail = String::new();
                for n in chars.by_ref() {
                    tail.push(n);
                    if tail.ends_with("-->") {
                        break;
                    }
                }
            } else {
                for n in chars.by_ref() {
                    if n == '>' {
                        break;
                    }
                }
            }
            continue;
        }
        match c {
            '\r' => {}
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            // Bidirectional overrides could make the text read differently than it is.
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {}
            c => out.push(c),
        }
    }
    let mut text = String::new();
    let mut blank = 0;
    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 || text.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        text.push_str(line);
        text.push('\n');
    }
    let text = text.trim_end().to_string();
    if text.chars().count() > MAX_NOTES {
        let cut: String = text.chars().take(MAX_NOTES).collect();
        format!("{}\n…", cut.trim_end())
    } else {
        text
    }
}

/// The newest release above `current` among `releases` (pre-releases only
/// with `beta`), with its download for `os`.
fn newest(releases: Vec<GhRelease>, current: &Version, beta: bool, os: Os) -> Option<Found> {
    releases
        .into_iter()
        .filter(|r| !r.draft && r.html_url.starts_with(RELEASES_PREFIX))
        .filter_map(|r| tag_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, r)| beta || (!r.prerelease && v.pre.is_empty()))
        .filter(|(v, _)| v > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, r)| {
            let asset = pick_asset(os, &version, &r.assets);
            let sums = r
                .assets
                .iter()
                .filter(|a| asset_ok(a))
                .filter(|a| a.name.starts_with("SHA256SUMS") || a.name.ends_with(".sha256"))
                .map(|a| UpdateAsset {
                    name: a.name.clone(),
                    size: a.size,
                    url: a.browser_download_url.clone(),
                })
                .collect();
            Found {
                release: UpdateRelease {
                    version: version.to_string(),
                    tag: r.tag_name.clone(),
                    name: plain_notes(r.name.as_deref().unwrap_or(&r.tag_name)),
                    notes: plain_notes(r.body.as_deref().unwrap_or("")),
                    url: r.html_url,
                    published_at: r.published_at,
                    prerelease: r.prerelease || !version.pre.is_empty(),
                    asset,
                },
                sums,
            }
        })
}

/// The SHA-256 (lowercase hex) of `file` in a `sha256sum` listing.
pub fn checksum_for(listing: &str, file: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        (name == file && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

// ------------------------------------------------------------------ state

/// What `update.json` keeps.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Options {
    check_weekly: bool,
    beta: bool,
    last_check: Option<u64>,
    /// The version a weekly check last announced (announced once).
    notified: Option<String>,
}

#[derive(Default)]
struct State {
    options: Options,
    checking: bool,
    error: Option<String>,
    found: Option<Found>,
    job: Option<UpdateJob>,
}

/// Opens a downloaded installer.
pub type Launcher = Box<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;
/// Tells the user about a release a weekly check found.
pub type Announce = Box<dyn Fn(&UpdateRelease) + Send + Sync>;

/// How an [`Updater`] works.
pub struct UpdaterOptions {
    /// The network.
    pub http: Arc<dyn Http>,
    /// `update.json`; `None`: options are not kept (ephemeral runs).
    pub file: Option<PathBuf>,
    /// The running version.
    pub current_version: String,
    /// Which download fits.
    pub os: Os,
    /// `Ok`: *Download* may download and open the installer here; `Err`:
    /// why it opens the release page instead.
    pub installable: Result<(), String>,
    /// Where downloads go.
    pub download_dir: PathBuf,
    /// Opens the verified installer.
    pub launcher: Launcher,
    /// Announces a release found by the weekly check.
    pub announce: Announce,
}

/// The update checker (one per daemon).
pub struct Updater {
    opts: UpdaterOptions,
    current: Version,
    state: Mutex<State>,
    /// One check at a time.
    checking: Mutex<()>,
    /// Itself, for the download thread.
    me: Weak<Updater>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Updater {
    /// An updater with `opts`; the options come from its file.
    pub fn new(opts: UpdaterOptions) -> Arc<Self> {
        let options = opts
            .file
            .as_deref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let current = Version::parse(&opts.current_version).unwrap_or_else(|_| Version::new(0, 0, 0));
        Arc::new_cyclic(|me| Self {
            opts,
            current,
            state: Mutex::new(State {
                options,
                ..State::default()
            }),
            checking: Mutex::new(()),
            me: me.clone(),
        })
    }

    /// The real thing: GitHub over HTTPS, this system's installer.
    pub fn system(file: Option<PathBuf>, announce: Announce) -> Arc<Self> {
        Self::new(UpdaterOptions {
            http: Arc::new(UreqHttp::new()),
            file,
            current_version: env!("CARGO_PKG_VERSION").into(),
            os: Os::current(),
            installable: installable_here(),
            download_dir: std::env::temp_dir().join("lightloom-update"),
            launcher: Box::new(launch_installer),
            announce,
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, options: &Options) {
        let Some(file) = &self.opts.file else { return };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = file.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(options).unwrap_or_default())?;
            std::fs::rename(&tmp, file)
        };
        if let Err(e) = write() {
            tracing::warn!("cannot save {}: {e}", file.display());
        }
    }

    fn view_of(&self, s: &State) -> UpdateView {
        UpdateView {
            current_version: self.current.to_string(),
            check_weekly: s.options.check_weekly,
            beta: s.options.beta,
            checking: s.checking,
            last_check: s.options.last_check,
            error: s.error.clone(),
            available: s.found.as_ref().map(|f| f.release.clone()),
            installable: self.opts.installable.is_ok(),
            install_note: self.opts.installable.clone().err(),
            job: s.job.clone(),
        }
    }

    /// Ask GitHub; the newest release above the running version, if any.
    fn fetch(&self, beta: bool) -> Result<Option<Found>, String> {
        let http = &*self.opts.http;
        let accept = "application/vnd.github+json";
        let releases: Vec<GhRelease> = if beta {
            let body = get_bytes(http, RELEASES_URL, accept, MAX_API_BODY).map_err(describe)?;
            serde_json::from_slice(&body)
                .map_err(|e| format!("GitHub's answer is not a release list: {e}"))?
        } else {
            match get_bytes(http, LATEST_URL, accept, MAX_API_BODY) {
                Ok(body) => vec![serde_json::from_slice(&body)
                    .map_err(|e| format!("GitHub's answer is not a release: {e}"))?],
                // No published release yet.
                Err(HttpError::Status(404)) => Vec::new(),
                Err(e) => return Err(describe(e)),
            }
        };
        Ok(newest(releases, &self.current, beta, self.opts.os))
    }

    fn check_now(&self) -> UpdateView {
        let _one = self.checking.lock().unwrap_or_else(|p| p.into_inner());
        let beta = {
            let mut s = self.lock();
            s.checking = true;
            s.options.beta
        };
        let result = self.fetch(beta);
        let mut s = self.lock();
        s.checking = false;
        s.options.last_check = Some(now_secs());
        match result {
            Ok(found) => {
                s.error = None;
                // A new release replaces an ended download's message.
                if s.job.as_ref().is_some_and(|j| !j.running)
                    && found.as_ref().map(|f| &f.release.version) != s.job.as_ref().map(|j| &j.version)
                {
                    s.job = None;
                }
                s.found = found;
            }
            Err(e) => {
                tracing::info!("update check: {e}");
                s.error = Some(e);
            }
        }
        let options = s.options.clone();
        let view = self.view_of(&s);
        drop(s);
        self.save(&options);
        view
    }

    /// The weekly check, when it is on and due: check, and announce a new
    /// release once.
    pub fn weekly_tick(&self, now: u64) {
        let due = {
            let s = self.lock();
            s.options.check_weekly
                && s.options
                    .last_check
                    .is_none_or(|t| now.saturating_sub(t) >= WEEK.as_secs() || t > now)
        };
        if !due {
            return;
        }
        let view = self.check_now();
        let Some(release) = view.available else { return };
        let mut s = self.lock();
        if s.options.notified.as_deref() == Some(release.version.as_str()) {
            return;
        }
        s.options.notified = Some(release.version.clone());
        let options = s.options.clone();
        drop(s);
        self.save(&options);
        (self.opts.announce)(&release);
    }

    /// Run [`Updater::weekly_tick`] on a thread of its own, now and then.
    pub fn start_schedule(self: &Arc<Self>) {
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("lightloom-update".into())
            .spawn(move || {
                std::thread::sleep(FIRST_LOOK);
                loop {
                    me.weekly_tick(now_secs());
                    std::thread::sleep(SCHEDULE_TICK);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("cannot start the weekly update check: {e}");
        }
    }

    fn set_job(&self, f: impl FnOnce(&mut UpdateJob)) {
        if let Some(job) = self.lock().job.as_mut() {
            f(job);
        }
    }

    /// Download, verify, open. The job says how it went.
    fn download_and_open(
        &self,
        release: &UpdateRelease,
        asset: &UpdateAsset,
        sums: &[UpdateAsset],
    ) -> Result<String, String> {
        let http = &*self.opts.http;
        let mut expected = None;
        for s in sums {
            let listing = get_bytes(http, &s.url, "application/octet-stream", MAX_SUMS_BODY)
                .map_err(|e| format!("cannot download {}: {}", s.name, describe(e)))?;
            if let Some(h) = checksum_for(&String::from_utf8_lossy(&listing), &asset.name) {
                expected = Some(h);
                break;
            }
        }
        let expected = expected.ok_or_else(|| {
            format!(
                "release {} publishes no SHA-256 for {}: nothing was downloaded; get it from the release page",
                release.version, asset.name
            )
        })?;
        if asset.size > MAX_ASSET {
            return Err(format!("{} is too large ({} bytes)", asset.name, asset.size));
        }
        let dir = &self.opts.download_dir;
        // Earlier downloads go (the directory is Lightloom's own).
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join(&asset.name);
        let part = dir.join(format!("{}.part", asset.name));
        let file =
            std::fs::File::create(&part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
        let mut sink = Hashing {
            file: std::io::BufWriter::new(file),
            hash: Sha256::new(),
        };
        let got = http.get(
            &asset.url,
            "application/octet-stream",
            if asset.size > 0 { asset.size } else { MAX_ASSET },
            &mut sink,
            &mut |n| self.set_job(|j| j.downloaded = n),
        );
        let finished = got
            .map_err(describe)
            .and_then(|n| sink.file.flush().map(|_| n).map_err(|e| e.to_string()));
        let Hashing { file, hash } = sink;
        drop(file);
        let check = finished.and_then(|n| {
            if asset.size > 0 && n != asset.size {
                return Err(format!("{} came incomplete ({n} of {} bytes)", asset.name, asset.size));
            }
            let actual: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
            if actual != expected {
                return Err(format!(
                    "{} does not match the release's SHA-256 (expected {expected}, got {actual}); it was deleted",
                    asset.name
                ));
            }
            Ok(())
        });
        if let Err(e) = check {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, &path).map_err(|e| format!("cannot rename {}: {e}", part.display()))?;
        (self.opts.launcher)(&path)
            .map_err(|e| format!("cannot open the installer {}: {e}", path.display()))?;
        Ok(format!(
            "Lightloom {} downloaded and verified (SHA-256). The installer is open: it stops Lightloom, \
             updates it and starts the new version when you press Finish. Cancel it to keep this version.",
            release.version
        ))
    }
}

/// The user-facing reason a request failed.
fn describe(e: HttpError) -> String {
    match e {
        HttpError::Status(403) | HttpError::Status(429) => {
            "GitHub refused the request for now (rate limit): try again later".into()
        }
        HttpError::Status(s) => format!("GitHub answered HTTP {s}"),
        HttpError::Failed(e) => format!("cannot reach GitHub: {e}"),
    }
}

/// A file that hashes what is written to it.
struct Hashing {
    file: std::io::BufWriter<std::fs::File>,
    hash: Sha256,
}

impl Write for Hashing {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.hash.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Updates for Updater {
    fn view(&self) -> UpdateView {
        self.view_of(&self.lock())
    }

    fn check(&self) -> Result<UpdateView, ApiError> {
        Ok(self.check_now())
    }

    fn set_options(&self, check_weekly: Option<bool>, beta: Option<bool>) -> Result<UpdateView, ApiError> {
        let mut s = self.lock();
        if let Some(on) = check_weekly {
            s.options.check_weekly = on;
        }
        if let Some(on) = beta {
            if on != s.options.beta {
                // What the last check found was for the other channel.
                s.found = None;
            }
            s.options.beta = on;
        }
        let options = s.options.clone();
        let view = self.view_of(&s);
        drop(s);
        self.save(&options);
        Ok(view)
    }

    fn install(&self) -> Result<UpdateView, ApiError> {
        if let Err(why) = &self.opts.installable {
            return Err(ApiError::conflict(why.clone()));
        }
        let Some(me) = self.me.upgrade() else {
            return Err(ApiError::unavailable("the daemon is shutting down"));
        };
        let (release, asset, sums) = {
            let mut s = self.lock();
            if s.job.as_ref().is_some_and(|j| j.running) {
                return Err(ApiError::conflict("a download is already running"));
            }
            let Some(found) = s.found.clone() else {
                return Err(ApiError::conflict("no newer release: check for updates first"));
            };
            let Some(asset) = found.release.asset.clone() else {
                return Err(ApiError::conflict(format!(
                    "release {} has no installer for this system: open the release page",
                    found.release.version
                )));
            };
            s.job = Some(UpdateJob {
                version: found.release.version.clone(),
                running: true,
                ok: None,
                downloaded: 0,
                total: asset.size,
                message: format!("Downloading {}…", asset.name),
            });
            (found.release, asset, found.sums)
        };
        tracing::info!("downloading {} ({} bytes)", asset.url, asset.size);
        let spawned = std::thread::Builder::new()
            .name("lightloom-update-download".into())
            .spawn(move || {
                let result = me.download_and_open(&release, &asset, &sums);
                match &result {
                    Ok(_) => tracing::info!("update {} downloaded, verified and opened", release.version),
                    Err(e) => tracing::warn!("update {}: {e}", release.version),
                }
                me.set_job(|j| {
                    j.running = false;
                    j.ok = Some(result.is_ok());
                    j.message = result.unwrap_or_else(|e| e);
                });
            });
        if let Err(e) = spawned {
            self.set_job(|j| {
                j.running = false;
                j.ok = Some(false);
                j.message = format!("cannot start the download: {e}");
            });
        }
        Ok(self.view())
    }
}

// ------------------------------------------------------------ the platform

/// Whether *Download* may run the installer here: a per-user MSI install
/// (`lightloomd.exe` in `%LOCALAPPDATA%\Programs\Lightloom`) on Windows.
pub fn installable_here() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Updates are installed from the release page on this system".into());
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .ok_or_else(|| "cannot tell where Lightloom runs from".to_string())?;
    let installed = std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("Programs").join("Lightloom"))
        .ok_or_else(|| "LOCALAPPDATA is not set".to_string())?;
    if same_dir(&exe_dir, &installed) {
        Ok(())
    } else {
        Err(format!(
            "This copy runs from {}, not from the installed location: download the new version from the release page",
            exe_dir.display()
        ))
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Open the verified MSI: `msiexec /i <file> /l*v <file>.log`, with its own
/// window, detached from the daemon (which the installer stops).
#[cfg(windows)]
pub fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let msiexec = system.join("System32").join("msiexec.exe");
    let spawn = |flags: u32| {
        Command::new(&msiexec)
            .arg("/i")
            .arg(path)
            .arg("/l*v")
            .arg(path.with_extension("log"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    // Out of any job the daemon runs in, so the installer outlives it; a job
    // that forbids breaking away refuses the flag.
    spawn(base | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(base))
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Not reached: only Windows downloads installers ([`installable_here`]).
#[cfg(not(windows))]
pub fn launch_installer(_path: &Path) -> Result<(), String> {
    Err("installers are opened on Windows only".into())
}

#[cfg(test)]
mod tests;
