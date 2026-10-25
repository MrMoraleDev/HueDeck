// SPDX-License-Identifier: GPL-3.0-or-later
//! The update check against a scripted GitHub: which request goes out, which
//! release is offered, the checksum-verified download, the options file, the
//! weekly schedule; and the real HTTP client against a loopback server (what
//! it sends).

use super::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};

const BASE: &str = "https://github.com/MrMoraleDev/Lightloom/releases";
const MSI: &[u8] = b"pretend this is an MSI package";

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn asset(name: &str, size: usize) -> serde_json::Value {
    json!({
        "name": name,
        "size": size,
        "browser_download_url": format!("{BASE}/download/v1.2.0/{name}"),
        "content_type": "application/octet-stream",
        "uploader": {"login": "someone"}
    })
}

fn release(tag: &str, prerelease: bool) -> serde_json::Value {
    let v = tag.trim_start_matches('v');
    // packaging/wix/build.ps1 names the MSI after x.y.z (a pre-release suffix is dropped).
    let msi = v.split('-').next().unwrap();
    json!({
        "url": "https://api.github.com/repos/MrMoraleDev/Lightloom/releases/1",
        "html_url": format!("{BASE}/tag/{tag}"),
        "id": 1,
        "tag_name": tag,
        "name": format!("Lightloom {v}"),
        "draft": false,
        "prerelease": prerelease,
        "published_at": "2026-10-20T10:00:00Z",
        "body": "## What's new\r\n\r\n* <b>Faster</b> effects <!-- hidden -->\r\n\r\n\r\n\r\n* Fixes\u{202e}",
        "assets": [
            asset(&format!("Lightloom-{msi}-x64.msi"), MSI.len()),
            asset(&format!("Lightloom-{msi}-x64-it-IT.msi"), MSI.len()),
            asset("lightloom-x86_64-pc-windows-msvc.zip", 10),
            asset(&format!("lightloom-{v}-linux-x86_64.tar.xz"), 10),
            asset(&format!("lightloom-{v}-x86_64.AppImage"), 10),
            asset("SHA256SUMS", 100),
        ]
    })
}

/// A scripted network: URL → (status, body); every request is recorded.
#[derive(Default)]
struct Scripted {
    answers: Mutex<BTreeMap<String, (u16, Vec<u8>)>>,
    requests: Mutex<Vec<(String, String)>>,
}

impl Scripted {
    fn answer(&self, url: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.answers
            .lock()
            .unwrap()
            .insert(url.into(), (status, body.into()));
    }

    fn urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(u, _)| u.clone())
            .collect()
    }
}

impl Http for Scripted {
    fn get(
        &self,
        url: &str,
        accept: &str,
        max: u64,
        sink: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, HttpError> {
        self.requests.lock().unwrap().push((url.into(), accept.into()));
        let (status, body) = self
            .answers
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .unwrap_or((404, Vec::new()));
        if status != 200 {
            return Err(HttpError::Status(status));
        }
        if body.len() as u64 > max {
            return Err(HttpError::Failed("body too long".into()));
        }
        sink.write_all(&body).unwrap();
        progress(body.len() as u64);
        Ok(body.len() as u64)
    }
}

struct Rig {
    http: Arc<Scripted>,
    updater: Arc<Updater>,
    launched: Arc<Mutex<Vec<(PathBuf, Vec<u8>)>>>,
    announced: Arc<Mutex<Vec<String>>>,
    _dir: TempDir,
}

/// A temporary directory, removed afterwards.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lightloom-update-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rig_in(dir: TempDir, current: &str, os: Os, installable: Result<(), String>) -> Rig {
    let http = Arc::new(Scripted::default());
    let launched = Arc::new(Mutex::new(Vec::new()));
    let announced = Arc::new(Mutex::new(Vec::new()));
    let (l, a) = (Arc::clone(&launched), Arc::clone(&announced));
    let updater = Updater::new(UpdaterOptions {
        http: http.clone(),
        file: Some(dir.0.join("update.json")),
        current_version: current.into(),
        os,
        installable,
        download_dir: dir.0.join("download"),
        launcher: Box::new(move |p: &Path| {
            l.lock()
                .unwrap()
                .push((p.to_path_buf(), std::fs::read(p).unwrap()));
            Ok(())
        }),
        announce: Box::new(move |r: &UpdateRelease| a.lock().unwrap().push(r.version.clone())),
    });
    Rig {
        http,
        updater,
        launched,
        announced,
        _dir: dir,
    }
}

fn rig(current: &str) -> Rig {
    rig_in(TempDir::new(), current, Os::Windows, Ok(()))
}

fn serve_latest(r: &Rig, tag: &str) {
    r.http.answer(LATEST_URL, 200, release(tag, false).to_string());
}

fn wait_for_job(u: &Updater) -> UpdateJob {
    for _ in 0..500 {
        let job = u.view().job.expect("a job");
        if !job.running {
            return job;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the download did not end");
}

#[test]
fn nothing_is_fetched_until_asked() {
    let r = rig("0.0.1");
    let v = r.updater.view();
    assert!(r.http.urls().is_empty());
    assert_eq!((v.check_weekly, v.beta, v.last_check), (false, false, None));
    assert_eq!(v.current_version, "0.0.1");
    // The weekly check is off by default: its tick does nothing.
    r.updater.weekly_tick(now_secs());
    assert!(r.http.urls().is_empty());
}

#[test]
fn a_check_asks_for_the_latest_release_and_offers_a_newer_one() {
    let r = rig("0.0.1");
    serve_latest(&r, "v1.2.0");
    let v = r.updater.check().unwrap();
    assert_eq!(
        *r.http.requests.lock().unwrap(),
        vec![(LATEST_URL.to_string(), "application/vnd.github+json".to_string())]
    );
    assert!(v.error.is_none() && v.last_check.is_some() && !v.checking);
    let a = v.available.expect("1.2.0 is newer");
    assert_eq!(
        (a.version.as_str(), a.tag.as_str(), a.prerelease),
        ("1.2.0", "v1.2.0", false)
    );
    assert_eq!(a.url, format!("{BASE}/tag/v1.2.0"));
    assert_eq!(a.name, "Lightloom 1.2.0");
    // Plain text: no tags, comments, CRs, bidi overrides or runs of blank lines.
    assert_eq!(a.notes, "## What's new\n\n* Faster effects\n\n* Fixes");
    let asset = a.asset.expect("the MSI");
    assert_eq!(asset.name, "Lightloom-1.2.0-x64.msi");
    assert_eq!(asset.size, MSI.len() as u64);
}

#[test]
fn the_same_or_an_older_release_is_up_to_date() {
    for (current, tag) in [("1.2.0", "v1.2.0"), ("1.3.0", "v1.2.0"), ("1.2.0", "1.2.0")] {
        let r = rig(current);
        serve_latest(&r, tag);
        let v = r.updater.check().unwrap();
        assert!(v.available.is_none() && v.error.is_none(), "{current} vs {tag}");
    }
    // No release published yet.
    let r = rig("0.0.1");
    let v = r.updater.check().unwrap();
    assert!(v.available.is_none() && v.error.is_none());
}

#[test]
fn failures_are_reported_and_keep_nothing_stale() {
    let r = rig("0.0.1");
    serve_latest(&r, "v1.2.0");
    assert!(r.updater.check().unwrap().available.is_some());
    r.http.answer(LATEST_URL, 403, "");
    let v = r.updater.check().unwrap();
    assert!(v.error.unwrap().contains("rate limit"));
    r.http.answer(LATEST_URL, 200, "<html>not json</html>");
    let v = r.updater.check().unwrap();
    assert!(v.error.unwrap().contains("not a release"));
}

#[test]
fn prereleases_need_the_beta_channel() {
    let r = rig("1.2.0");
    let list = json!([
        release("v1.3.0-beta.1", true),
        release("v1.2.0", false),
        release("v1.1.0", false)
    ]);
    r.http.answer(RELEASES_URL, 200, list.to_string());
    serve_latest(&r, "v1.2.0");
    assert!(r.updater.check().unwrap().available.is_none());
    assert_eq!(r.http.urls(), vec![LATEST_URL.to_string()]);

    let v = r.updater.set_options(None, Some(true)).unwrap();
    assert!(v.beta && !v.check_weekly);
    let v = r.updater.check().unwrap();
    assert_eq!(r.http.urls().last().unwrap(), RELEASES_URL);
    let a = v.available.unwrap();
    assert_eq!((a.version.as_str(), a.prerelease), ("1.3.0-beta.1", true));
    assert_eq!(a.asset.unwrap().name, "Lightloom-1.3.0-x64.msi");
    // Turning the channel off forgets the pre-release.
    assert!(r
        .updater
        .set_options(None, Some(false))
        .unwrap()
        .available
        .is_none());
}

#[test]
fn drafts_foreign_links_and_semver_prereleases_are_ignored() {
    let r = rig("1.0.0");
    let mut draft = release("v3.0.0", false);
    draft["draft"] = json!(true);
    let mut foreign = release("v2.5.0", false);
    foreign["html_url"] = json!("https://evil.example/Lightloom/releases/tag/v2.5.0");
    let tagged_pre = release("v2.4.0-rc.1", false);
    let mut bad_asset = release("v2.0.0", false);
    bad_asset["assets"][0]["browser_download_url"] = json!("https://evil.example/Lightloom-2.0.0-x64.msi");
    bad_asset["assets"][1]["name"] = json!("..\\Lightloom-2.0.0-x64.msi");
    let list = json!([draft, foreign, tagged_pre, bad_asset]);
    r.http.answer(RELEASES_URL, 200, list.to_string());
    r.updater.set_options(None, Some(true)).unwrap();
    // The beta channel takes the release candidate (the draft and the foreign page never count).
    let a = r.updater.check().unwrap().available.unwrap();
    assert_eq!(a.version, "2.4.0-rc.1");

    // A stable check never offers a semver pre-release, even if GitHub calls it latest.
    let r = rig("1.0.0");
    r.http
        .answer(LATEST_URL, 200, release("v2.4.0-rc.1", false).to_string());
    assert!(r.updater.check().unwrap().available.is_none());

    // Only GitHub download links of this repository, with plain names, are offered.
    let r = rig("1.0.0");
    r.http
        .answer(LATEST_URL, 200, json!(bad_asset_only()).to_string());
    assert!(r.updater.check().unwrap().available.unwrap().asset.is_none());
}

fn bad_asset_only() -> serde_json::Value {
    let mut r = release("v2.0.0", false);
    r["assets"] = json!([
        {"name": "Lightloom-2.0.0-x64.msi", "size": 1, "browser_download_url": "https://evil.example/Lightloom-2.0.0-x64.msi"},
        {"name": "../Lightloom-2.0.0-x64.msi", "size": 1, "browser_download_url": format!("{BASE}/download/v2.0.0/x.msi")},
    ]);
    r
}

#[test]
fn assets_per_system() {
    let v = Version::new(1, 2, 0);
    let assets: Vec<GhAsset> = serde_json::from_value(release("v1.2.0", false)["assets"].clone()).unwrap();
    let name = |os| pick_asset(os, &v, &assets).map(|a| a.name);
    assert_eq!(name(Os::Windows).as_deref(), Some("Lightloom-1.2.0-x64.msi"));
    assert_eq!(
        name(Os::Linux).as_deref(),
        Some("lightloom-1.2.0-x86_64.AppImage")
    );
    assert_eq!(name(Os::Other), None);
    let tarball_only: Vec<GhAsset> = assets
        .into_iter()
        .filter(|a| a.name.ends_with(".tar.xz"))
        .collect();
    assert_eq!(
        pick_asset(Os::Linux, &v, &tarball_only)
            .map(|a| a.name)
            .as_deref(),
        Some("lightloom-1.2.0-linux-x86_64.tar.xz")
    );
}

#[test]
fn checksum_listings() {
    let h = "AB".repeat(32);
    let listing = format!(
        "{h}  Lightloom-1.2.0-x64.msi\n{}  lightloom.zip\n{} *bin.tar.gz\n",
        "0".repeat(64),
        "1".repeat(64)
    );
    assert_eq!(
        checksum_for(&listing, "Lightloom-1.2.0-x64.msi"),
        Some("ab".repeat(32))
    );
    assert_eq!(checksum_for(&listing, "bin.tar.gz"), Some("1".repeat(64)));
    assert_eq!(checksum_for(&listing, "Lightloom-1.2.0-x64-it-IT.msi"), None);
    assert_eq!(
        checksum_for("abc  Lightloom-1.2.0-x64.msi", "Lightloom-1.2.0-x64.msi"),
        None
    );
}

#[test]
fn notes_are_plain_and_bounded() {
    assert_eq!(plain_notes("a<script>alert(1)</script>b"), "aalert(1)b");
    assert_eq!(plain_notes("1 < 2 and 3 > 2"), "1 < 2 and 3 > 2");
    assert_eq!(plain_notes("x<!-- a > b -->y\u{7}\u{2066}z"), "xyz");
    assert_eq!(plain_notes("\n\n\na\n\n\n\nb\n\n"), "a\n\nb");
    let long = plain_notes(&"é".repeat(MAX_NOTES + 10));
    assert!(long.ends_with('…') && long.chars().count() == MAX_NOTES + 2);
}

fn serve_download(r: &Rig, sums: &str) {
    serve_latest(r, "v1.2.0");
    r.http
        .answer(&format!("{BASE}/download/v1.2.0/SHA256SUMS"), 200, sums);
    r.http.answer(
        &format!("{BASE}/download/v1.2.0/Lightloom-1.2.0-x64.msi"),
        200,
        MSI,
    );
}

#[test]
fn install_downloads_verifies_and_opens_the_msi() {
    let r = rig("0.0.1");
    let sums = format!(
        "{}  lightloom-x86_64-pc-windows-msvc.zip\n{}  Lightloom-1.2.0-x64.msi\n",
        "0".repeat(64),
        sha256_hex(MSI)
    );
    serve_download(&r, &sums);
    // Nothing to install before a check found something.
    assert_eq!(r.updater.install().unwrap_err().status, 409);
    r.updater.check().unwrap();
    let v = r.updater.install().unwrap();
    assert!(v.job.is_some());
    let job = wait_for_job(&r.updater);
    assert_eq!(job.ok, Some(true), "{}", job.message);
    assert_eq!((job.downloaded, job.total), (MSI.len() as u64, MSI.len() as u64));
    let launched = r.launched.lock().unwrap();
    assert_eq!(launched.len(), 1);
    assert_eq!(launched[0].0.file_name().unwrap(), "Lightloom-1.2.0-x64.msi");
    assert_eq!(launched[0].1, MSI);
    assert_eq!(
        r.http.urls()[1..],
        [
            format!("{BASE}/download/v1.2.0/SHA256SUMS"),
            format!("{BASE}/download/v1.2.0/Lightloom-1.2.0-x64.msi")
        ]
    );
}

#[test]
fn a_wrong_checksum_deletes_the_download_and_opens_nothing() {
    let r = rig("0.0.1");
    serve_download(&r, &format!("{}  Lightloom-1.2.0-x64.msi\n", "f".repeat(64)));
    r.updater.check().unwrap();
    r.updater.install().unwrap();
    let job = wait_for_job(&r.updater);
    assert_eq!(job.ok, Some(false));
    assert!(job.message.contains("does not match"), "{}", job.message);
    assert!(r.launched.lock().unwrap().is_empty());
    let dir = r.updater.opts.download_dir.clone();
    assert_eq!(std::fs::read_dir(dir).unwrap().count(), 0);
}

#[test]
fn a_release_without_checksums_is_not_downloaded() {
    let r = rig("0.0.1");
    let mut rel = release("v1.2.0", false);
    rel["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["name"] != "SHA256SUMS");
    r.http.answer(LATEST_URL, 200, rel.to_string());
    r.updater.check().unwrap();
    r.updater.install().unwrap();
    let job = wait_for_job(&r.updater);
    assert_eq!(job.ok, Some(false));
    assert!(job.message.contains("no SHA-256"), "{}", job.message);
    assert_eq!(r.http.urls(), vec![LATEST_URL.to_string()]);
    assert!(r.launched.lock().unwrap().is_empty());
}

#[test]
fn a_short_download_fails() {
    let r = rig("0.0.1");
    serve_download(&r, &format!("{}  Lightloom-1.2.0-x64.msi\n", sha256_hex(MSI)));
    r.http.answer(
        &format!("{BASE}/download/v1.2.0/Lightloom-1.2.0-x64.msi"),
        200,
        &MSI[..5],
    );
    r.updater.check().unwrap();
    r.updater.install().unwrap();
    let job = wait_for_job(&r.updater);
    assert_eq!(job.ok, Some(false));
    assert!(job.message.contains("incomplete"), "{}", job.message);
}

#[test]
fn elsewhere_download_means_the_release_page() {
    let r = rig_in(
        TempDir::new(),
        "0.0.1",
        Os::Linux,
        Err("from the release page".into()),
    );
    serve_latest(&r, "v1.2.0");
    let v = r.updater.check().unwrap();
    assert!(!v.installable);
    assert_eq!(v.install_note.as_deref(), Some("from the release page"));
    assert_eq!(
        v.available.unwrap().asset.unwrap().name,
        "lightloom-1.2.0-x86_64.AppImage"
    );
    let e = r.updater.install().unwrap_err();
    assert_eq!(e.status, 409);
    assert_eq!(r.http.urls().len(), 1);
}

#[test]
fn options_are_kept_in_update_json() {
    let dir = TempDir::new();
    let file = dir.0.join("update.json");
    let r = rig_in(dir, "0.0.1", Os::Windows, Ok(()));
    r.updater.set_options(Some(true), Some(true)).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["checkWeekly"], true);
    assert_eq!(saved["beta"], true);
    // A new daemon reads them back.
    let again = Updater::new(UpdaterOptions {
        http: r.http.clone(),
        file: Some(file),
        current_version: "0.0.1".into(),
        os: Os::Windows,
        installable: Ok(()),
        download_dir: r._dir.0.join("download"),
        launcher: Box::new(|_: &Path| Ok(())),
        announce: Box::new(|_: &UpdateRelease| {}),
    });
    let v = again.view();
    assert!(v.check_weekly && v.beta);
}

#[test]
fn the_weekly_check_runs_once_a_week_and_announces_once() {
    let r = rig("0.0.1");
    serve_latest(&r, "v1.2.0");
    r.updater.set_options(Some(true), None).unwrap();
    let now = now_secs();
    r.updater.weekly_tick(now);
    assert_eq!(r.http.urls().len(), 1);
    assert_eq!(*r.announced.lock().unwrap(), vec!["1.2.0".to_string()]);
    // Not due again within the week.
    r.updater.weekly_tick(now + 3600);
    assert_eq!(r.http.urls().len(), 1);
    // Due a week later; the same release is not announced twice.
    r.updater.weekly_tick(now + WEEK.as_secs() + 1);
    assert_eq!(r.http.urls().len(), 2);
    assert_eq!(r.announced.lock().unwrap().len(), 1);
    // A newer one is.
    serve_latest(&r, "v1.3.0");
    r.updater.weekly_tick(now + 3 * WEEK.as_secs());
    assert_eq!(
        *r.announced.lock().unwrap(),
        vec!["1.2.0".to_string(), "1.3.0".to_string()]
    );
    // Turned off: nothing.
    r.updater.set_options(Some(false), None).unwrap();
    r.updater.weekly_tick(now + 10 * WEEK.as_secs());
    assert_eq!(r.http.urls().len(), 3);
}

/// The real client against a loopback HTTP server: the request carries the
/// `User-Agent` and `Accept` and nothing that identifies the user.
#[test]
fn the_real_client_sends_nothing_but_the_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in [("200 OK", "{\"ok\":true}"), ("404 Not Found", "")] {
            let (mut s, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(s.try_clone().unwrap());
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                head.push(line.trim().to_string());
            }
            seen.push(head);
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).unwrap();
            let _ = s.read(&mut [0u8; 16]);
        }
        seen
    });
    let http = UreqHttp::with(false);
    let url = format!("http://127.0.0.1:{port}/repos/MrMoraleDev/Lightloom/releases/latest");
    let body = get_bytes(&http, &url, "application/vnd.github+json", MAX_API_BODY).unwrap();
    assert_eq!(body, b"{\"ok\":true}");
    assert_eq!(
        get_bytes(&http, &url, "application/vnd.github+json", MAX_API_BODY),
        Err(HttpError::Status(404))
    );
    let seen = server.join().unwrap();
    let head = &seen[0];
    assert_eq!(
        head[0],
        "GET /repos/MrMoraleDev/Lightloom/releases/latest HTTP/1.1"
    );
    let headers: BTreeMap<String, String> = head[1..]
        .iter()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    assert_eq!(headers["user-agent"], USER_AGENT);
    assert_eq!(headers["accept"], "application/vnd.github+json");
    let allowed = ["host", "user-agent", "accept"];
    let extra: Vec<&String> = headers
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .collect();
    assert!(extra.is_empty(), "unexpected headers: {head:?}");
    // HTTPS only outside tests.
    assert!(get_bytes(&UreqHttp::new(), &url, "*/*", 10).is_err());
}
