//! A project is a self-contained folder: manifest, resampled source copies, cached analyses, takes.
//!
//! ```text
//! <dir>/project.toml        manifest
//! <dir>/sources/backing.wav f32, project rate
//! <dir>/sources/vocals.wav  f32, project rate
//! <dir>/cache/reference.f0  reference contour (keyed by stem hash + analyzer version)
//! <dir>/takes/take-NNN.wav  mono f32 + .f0 contour + .scores.json + .start sidecar
//! ```

pub mod audio_io;
pub mod export;
pub mod takes;
pub mod worker;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const MANIFEST: &str = "project.toml";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    /// Path relative to the project folder.
    pub file: String,
    /// File name it was imported from (display only).
    pub original_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Guide {
    pub audible: bool,
    pub level_db: f32,
    /// Transposition of the displayed reference, in cents.
    pub offset_cents: i32,
    /// Take to sing against instead of the reference (harmonising on a take); `None` = guide.
    #[serde(default)]
    pub target_take: Option<u32>,
}

impl Default for Guide {
    fn default() -> Self {
        Self {
            audible: false,
            level_db: -6.0,
            offset_cents: 0,
            target_take: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TakeEntry {
    pub id: u32,
    /// Song frame of the take's first sample.
    pub start_frame: i64,
    pub muted: bool,
    pub soloed: bool,
    pub volume_db: f32,
    pub visible: bool,
}

impl TakeEntry {
    pub fn new(id: u32, start_frame: i64) -> Self {
        Self {
            id,
            start_frame,
            muted: false,
            soloed: false,
            volume_db: 0.0,
            visible: true,
        }
    }

    pub fn name(&self) -> String {
        format!("take {}", self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format_version: u32,
    pub sample_rate: u32,
    pub backing: Option<Source>,
    pub vocals: Option<Source>,
    #[serde(default)]
    pub guide: Guide,
    #[serde(default)]
    pub takes: Vec<TakeEntry>,
    #[serde(default = "first_id")]
    pub next_take_id: u32,
}

fn first_id() -> u32 {
    1
}

#[derive(Debug, Clone)]
pub struct Project {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Project {
    /// Creates a project in `dir`, which must be empty (or not exist yet).
    pub fn create(dir: &Path, sample_rate: u32) -> Result<Self> {
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!(
                "{} is not empty; choose an empty folder for a new project",
                dir.display()
            );
        }
        for sub in ["sources", "cache", "takes"] {
            std::fs::create_dir_all(dir.join(sub))?;
        }
        let p = Self {
            dir: dir.to_path_buf(),
            manifest: Manifest {
                format_version: FORMAT_VERSION,
                sample_rate,
                backing: None,
                vocals: None,
                guide: Guide::default(),
                takes: Vec::new(),
                next_take_id: 1,
            },
        };
        p.save()?;
        Ok(p)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).with_context(|| {
            format!("{} is not a project folder (no {MANIFEST})", dir.display())
        })?;
        let manifest: Manifest =
            toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
        if manifest.format_version > FORMAT_VERSION {
            bail!(
                "project was created by a newer version (format {})",
                manifest.format_version
            );
        }
        for sub in ["sources", "cache", "takes"] {
            std::fs::create_dir_all(dir.join(sub))?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    /// Writes the manifest atomically (temp file + rename).
    pub fn save(&self) -> Result<()> {
        let tmp = self.dir.join(format!("{MANIFEST}.tmp"));
        std::fs::write(&tmp, toml::to_string_pretty(&self.manifest)?)?;
        std::fs::rename(&tmp, self.dir.join(MANIFEST))?;
        Ok(())
    }

    pub fn name(&self) -> String {
        self.dir
            .file_name()
            .map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    pub fn reference_cache(&self) -> PathBuf {
        self.dir.join("cache/reference.f0")
    }

    pub fn takes_dir(&self) -> PathBuf {
        self.dir.join("takes")
    }

    pub fn take_wav(&self, id: u32) -> PathBuf {
        takes::take_path(&self.takes_dir(), id, "wav")
    }

    pub fn take_file(&self, id: u32, ext: &str) -> PathBuf {
        takes::take_path(&self.takes_dir(), id, ext)
    }

    pub fn take(&self, id: u32) -> Option<&TakeEntry> {
        self.manifest.takes.iter().find(|t| t.id == id)
    }

    pub fn take_mut(&mut self, id: u32) -> Option<&mut TakeEntry> {
        self.manifest.takes.iter_mut().find(|t| t.id == id)
    }

    /// Removes a take and its files.
    pub fn delete_take(&mut self, id: u32) -> Result<()> {
        self.manifest.takes.retain(|t| t.id != id);
        self.save()?;
        for ext in ["wav", "f0", "scores.json", "start"] {
            let p = self.take_file(id, ext);
            if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        Ok(())
    }

    /// Adds manifest entries for take WAVs that have none (e.g. after a crash before the manifest
    /// was saved). Returns the recovered ids.
    pub fn recover_orphan_takes(&mut self) -> Result<Vec<u32>> {
        let mut recovered = Vec::new();
        for (id, path) in takes::list_take_wavs(&self.takes_dir())? {
            if self.take(id).is_none() {
                let start = takes::read_start_sidecar(&path.with_extension("start")).unwrap_or(0);
                self.manifest.takes.push(TakeEntry::new(id, start));
                recovered.push(id);
            }
            self.manifest.next_take_id = self.manifest.next_take_id.max(id + 1);
        }
        if !recovered.is_empty() {
            self.manifest.takes.sort_by_key(|t| t.id);
            self.save()?;
        }
        Ok(recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn temp_dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tuned-record-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn create_save_reopen() {
        let d = temp_dir("manifest");
        let mut p = Project::create(&d, 48_000).unwrap();
        assert!(d.join(MANIFEST).exists());
        p.manifest.guide.offset_cents = 400;
        p.manifest.takes.push(TakeEntry {
            volume_db: -3.0,
            ..TakeEntry::new(1, 4800)
        });
        p.save().unwrap();
        let q = Project::open(&d).unwrap();
        assert_eq!(q.manifest, p.manifest);
        assert!(
            Project::create(&d, 48_000).is_err(),
            "non-empty folder must be refused"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn open_rejects_non_project() {
        let d = temp_dir("notproject");
        std::fs::create_dir_all(&d).unwrap();
        assert!(Project::open(&d).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn recovers_orphan_take() {
        let d = temp_dir("orphan");
        let mut p = Project::create(&d, 48_000).unwrap();
        audio_io::write_wav(
            &p.take_wav(3),
            &crate::audio::mixer::Audio::mono(vec![0.1; 100]),
            48_000,
        )
        .unwrap();
        takes::write_start_sidecar(&p.take_file(3, "start"), 9600).unwrap();
        assert_eq!(p.recover_orphan_takes().unwrap(), vec![3]);
        assert_eq!(p.take(3).unwrap().start_frame, 9600);
        assert_eq!(p.manifest.next_take_id, 4);
        std::fs::remove_dir_all(d).unwrap();
    }
}
