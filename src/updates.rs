//! Check for a published Keyclash release without doing network work on the UI thread.

use eframe::egui;
use serde::Deserialize;
use std::fmt;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

const RELEASE_API: &str = "https://api.github.com/repos/hitpoint-hi/keyclash/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/hitpoint-hi/keyclash/releases";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version([u32; 3]);

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let mut version = [0; 3];
        for part in &mut version {
            let digits = parts.next()?;
            if !digits.bytes().all(|digit| digit.is_ascii_digit())
                || (digits.len() > 1 && digits.starts_with('0'))
            {
                return None;
            }
            *part = digits.parse().ok()?;
        }
        parts.next().is_none().then_some(Self(version))
    }

    pub fn release_url(self) -> String {
        format!("{RELEASE_PAGE}/tag/v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    state: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Checking,
    Current,
    Available(Version),
    Preparing(Version),
    Failed,
}

type CheckResult = Result<Status, ()>;

fn newer_release(json: &[u8], installed: Version) -> CheckResult {
    let release: Release = serde_json::from_slice(json).map_err(|_| ())?;
    if release.draft || release.prerelease {
        return Err(());
    }
    let latest = Version::parse(release.tag_name.strip_prefix('v').ok_or(())?).ok_or(())?;
    if latest <= installed {
        return Ok(Status::Current);
    }
    // The release script prepares this ZIP alongside the versioned copy.
    let has_download = release.assets.iter().any(|asset| {
        asset.name == "Keyclash.zip" && asset.state == "uploaded"
    });
    Ok(if has_download {
        Status::Available(latest)
    } else {
        Status::Preparing(latest)
    })
}

fn fetch_latest() -> CheckResult {
    let installed = Version::parse(crate::bundle::VERSION).ok_or(())?;
    // -q disables the user's curl config. Use only a fixed HTTPS endpoint with no credentials.
    let output = Command::new("/usr/bin/curl")
        .args([
            "-q",
            "--fail",
            "--silent",
            "--proto", "=https",
            "--connect-timeout", "5",
            "--max-time", "10",
            "--max-filesize", "1048576",
            "--user-agent", "Keyclash",
            "--header", "Accept: application/vnd.github+json",
            RELEASE_API,
        ])
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    newer_release(&output.stdout, installed)
}

pub struct Checker {
    pub status: Status,
    pub available: Option<Version>,
    pending: Option<mpsc::Receiver<CheckResult>>,
    last_check: Option<(SystemTime, Duration)>,
}

impl Default for Checker {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            available: None,
            pending: None,
            last_check: None,
        }
    }
}

impl Checker {
    pub fn time_until_check(&self) -> Option<Duration> {
        if self.pending.is_some() {
            return None;
        }
        let Some((last, interval)) = self.last_check else {
            return Some(Duration::ZERO);
        };
        // Time spent asleep counts. A backward clock change makes the check due again.
        Some(last.elapsed().map_or(Duration::ZERO, |elapsed| interval.saturating_sub(elapsed)))
    }

    pub fn start(&mut self, ctx: &egui::Context, settings_window: egui::ViewportId) {
        if self.pending.is_some() {
            return;
        }
        let (send, receive) = mpsc::channel();
        let egui_ctx = ctx.clone();
        let worker = std::thread::Builder::new().name("release check".into()).spawn(move || {
            let _ = send.send(fetch_latest());
            egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
            egui_ctx.request_repaint_of(settings_window);
        });
        if worker.is_ok() {
            self.pending = Some(receive);
            self.status = Status::Checking;
        } else {
            self.finish(Err(()));
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        ctx.request_repaint_of(settings_window);
    }

    pub fn receive(&mut self) {
        let Some(pending) = &self.pending else { return };
        let result = match pending.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(()),
        };
        self.pending = None;
        self.finish(result);
    }

    fn finish(&mut self, result: CheckResult) {
        self.status = result.unwrap_or(Status::Failed);
        let interval = match self.status {
            Status::Failed | Status::Preparing(_) => RETRY_INTERVAL,
            _ => CHECK_INTERVAL,
        };
        self.last_check = Some((SystemTime::now(), interval));
        match self.status {
            Status::Available(version) => self.available = Some(version),
            Status::Current | Status::Preparing(_) => self.available = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool, prerelease: bool, asset: &str, state: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": prerelease,
            "html_url": "https://example.invalid/",
            "assets": [{"name": asset, "state": state}],
        }))
        .unwrap()
    }

    #[test]
    fn reports_only_a_newer_stable_version_and_links_to_the_official_release() {
        let installed = Version([0, 1, 9]);
        for (tag, expected) in [
            ("v0.1.8", Status::Current),
            ("v0.1.9", Status::Current),
            ("v0.1.10", Status::Available(Version([0, 1, 10]))),
            ("v1.0.0", Status::Available(Version([1, 0, 0]))),
        ] {
            assert_eq!(
                newer_release(&release(tag, false, false, "Keyclash.zip", "uploaded"), installed),
                Ok(expected)
            );
        }
        assert_eq!(
            Version([0, 1, 10]).release_url(),
            "https://github.com/hitpoint-hi/keyclash/releases/tag/v0.1.10"
        );
    }

    #[test]
    fn ignores_numbers_that_cannot_name_a_stable_release() {
        for invalid in ["1.0", "1.0.0.1", "+1.0.0", "01.0.0", "1.0.0-beta", "1.0.0/path"] {
            assert!(Version::parse(invalid).is_none());
        }
    }

    #[test]
    fn does_not_advertise_a_preview_or_a_missing_download() {
        let installed = Version([0, 1, 9]);
        for (draft, prerelease) in [(true, false), (false, true)] {
            assert_eq!(
                newer_release(
                    &release("v0.1.10", draft, prerelease, "Keyclash.zip", "uploaded"),
                    installed
                ),
                Err(())
            );
        }
        for (asset, state) in [("Source.zip", "uploaded"), ("Keyclash.zip", "new")] {
            assert_eq!(
                newer_release(&release("v0.1.10", false, false, asset, state), installed),
                Ok(Status::Preparing(Version([0, 1, 10])))
            );
        }
        assert_eq!(newer_release(b"{}", installed), Err(()));
    }

    #[test]
    fn retries_failed_and_incomplete_checks_sooner() {
        let mut checker = Checker::default();
        checker.finish(Ok(Status::Current));
        assert_eq!(checker.last_check.unwrap().1, CHECK_INTERVAL);
        checker.finish(Err(()));
        assert_eq!(checker.last_check.unwrap().1, RETRY_INTERVAL);
        checker.finish(Ok(Status::Preparing(Version([0, 1, 10]))));
        assert_eq!(checker.last_check.unwrap().1, RETRY_INTERVAL);
    }
}
