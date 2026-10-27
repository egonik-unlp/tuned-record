//! Background jobs (one at a time, in order): loading projects, importing sources, pitch
//! analysis, export. Results come back as [`WorkerEvent`]s.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use super::audio_io::{load_for_project, read_wav, write_wav};
use super::export::{ExportInput, ExportReport, export};
use super::{Project, Source, TakeEntry, takes};
use crate::audio::mixer::Audio;
use crate::feedback::PhraseScore;
use crate::pitch::contour::Contour;
use crate::pitch::offline::{ANALYZER_VERSION, analyse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Backing,
    Vocals,
}

impl SourceKind {
    pub fn file(self) -> &'static str {
        match self {
            SourceKind::Backing => "sources/backing.wav",
            SourceKind::Vocals => "sources/vocals.wav",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Backing => "backing track",
            SourceKind::Vocals => "vocal stem",
        }
    }
}

pub enum Job {
    Open {
        dir: PathBuf,
    },
    Import {
        kind: SourceKind,
        from: PathBuf,
        project_dir: PathBuf,
        rate: u32,
    },
    AnalyseReference {
        audio: Arc<Audio>,
        rate: u32,
        cache: PathBuf,
        hash: [u8; 32],
    },
    LoadTake {
        id: u32,
        wav: PathBuf,
    },
    AnalyseTake {
        id: u32,
        audio: Arc<Audio>,
        rate: u32,
        start_frame: i64,
        f0_path: PathBuf,
    },
    Export {
        input: ExportInput,
        dir: PathBuf,
    },
}

pub struct LoadedTake {
    pub entry: TakeEntry,
    pub audio: Arc<Audio>,
    pub contour: Option<Contour>,
    pub scores: Vec<PhraseScore>,
}

pub struct LoadedProject {
    pub project: Project,
    pub backing: Option<Arc<Audio>>,
    pub vocals: Option<(Arc<Audio>, [u8; 32])>,
    /// Cached reference contour, if still valid for the stem.
    pub reference: Option<Contour>,
    pub takes: Vec<LoadedTake>,
    pub recovered: Vec<u32>,
}

pub enum WorkerEvent {
    Progress {
        label: String,
        fraction: f32,
    },
    Opened(Result<Box<LoadedProject>, String>),
    Imported {
        kind: SourceKind,
        audio: Arc<Audio>,
        source: Source,
        hash: [u8; 32],
    },
    ReferenceReady(Contour),
    TakeLoaded {
        id: u32,
        audio: Arc<Audio>,
    },
    TakeAnalysed {
        id: u32,
        contour: Contour,
    },
    Exported(Result<ExportReport, String>),
    Error(String),
    Idle,
}

pub fn spawn(jobs: Receiver<Job>, events: Sender<WorkerEvent>, notify: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("worker".into())
        .spawn(move || {
            let ev = |e: WorkerEvent| {
                let _ = events.send(e);
                notify();
            };
            for job in jobs.iter() {
                run(job, &ev);
                if jobs.is_empty() {
                    ev(WorkerEvent::Idle);
                }
            }
        })
        .expect("spawn worker thread");
}

fn run(job: Job, ev: &dyn Fn(WorkerEvent)) {
    match job {
        Job::Open { dir } => ev(WorkerEvent::Opened(
            open(&dir, ev).map(Box::new).map_err(|e| format!("{e:#}")),
        )),
        Job::Import {
            kind,
            from,
            project_dir,
            rate,
        } => match import(kind, &from, &project_dir, rate, ev) {
            Ok((audio, source, hash)) => ev(WorkerEvent::Imported {
                kind,
                audio,
                source,
                hash,
            }),
            Err(e) => ev(WorkerEvent::Error(format!(
                "Could not import {}: {e:#}",
                from.display()
            ))),
        },
        Job::AnalyseReference {
            audio,
            rate,
            cache,
            hash,
        } => {
            let mono = audio.to_mono();
            let mut c = analyse(&mono, rate, 0, |f| {
                ev(WorkerEvent::Progress {
                    label: "Analysing vocal stem".into(),
                    fraction: f,
                })
            });
            c.source_hash = hash;
            if let Err(e) = c.save(&cache) {
                ev(WorkerEvent::Error(format!(
                    "Could not cache the reference analysis: {e}"
                )));
            }
            ev(WorkerEvent::ReferenceReady(c));
        }
        Job::LoadTake { id, wav } => match read_wav(&wav) {
            Ok((a, _)) => ev(WorkerEvent::TakeLoaded {
                id,
                audio: Arc::new(a),
            }),
            Err(e) => ev(WorkerEvent::Error(format!(
                "Could not load take {id}: {e:#}"
            ))),
        },
        Job::AnalyseTake {
            id,
            audio,
            rate,
            start_frame,
            f0_path,
        } => {
            let c = analyse(&audio.to_mono(), rate, start_frame, |f| {
                ev(WorkerEvent::Progress {
                    label: format!("Analysing take {id}"),
                    fraction: f,
                })
            });
            if let Err(e) = c.save(&f0_path) {
                ev(WorkerEvent::Error(format!(
                    "Could not save take {id} contour: {e}"
                )));
            }
            ev(WorkerEvent::TakeAnalysed { id, contour: c });
        }
        Job::Export { input, dir } => {
            let r = export(&input, &dir, |f| {
                ev(WorkerEvent::Progress {
                    label: "Exporting".into(),
                    fraction: f,
                })
            });
            ev(WorkerEvent::Exported(r.map_err(|e| format!("{e:#}"))));
        }
    }
}

pub fn hash_file(path: &Path) -> Result<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    h.update_reader(std::fs::File::open(path)?)?;
    Ok(*h.finalize().as_bytes())
}

fn import(
    kind: SourceKind,
    from: &Path,
    project_dir: &Path,
    rate: u32,
    ev: &dyn Fn(WorkerEvent),
) -> Result<(Arc<Audio>, Source, [u8; 32])> {
    ev(WorkerEvent::Progress {
        label: format!("Importing {}", kind.label()),
        fraction: 0.0,
    });
    let audio = load_for_project(from, rate)?;
    let dest = project_dir.join(kind.file());
    write_wav(&dest, &audio, rate)?;
    let hash = hash_file(&dest)?;
    let original_name = from
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    ev(WorkerEvent::Progress {
        label: format!("Importing {}", kind.label()),
        fraction: 1.0,
    });
    Ok((
        Arc::new(audio),
        Source {
            file: kind.file().into(),
            original_name,
        },
        hash,
    ))
}

fn open(dir: &Path, ev: &dyn Fn(WorkerEvent)) -> Result<LoadedProject> {
    let mut project = Project::open(dir)?;
    let recovered = project.recover_orphan_takes()?;
    let label = "Opening project".to_string();
    ev(WorkerEvent::Progress {
        label: label.clone(),
        fraction: 0.0,
    });

    let load = |s: &Option<Source>| -> Result<Option<Arc<Audio>>> {
        match s {
            Some(s) => {
                let (a, _) = read_wav(&project.path(&s.file))
                    .with_context(|| format!("missing {}", s.file))?;
                Ok(Some(Arc::new(a)))
            }
            None => Ok(None),
        }
    };
    let backing = load(&project.manifest.backing)?;
    let vocals = match load(&project.manifest.vocals)? {
        Some(a) => Some((
            a,
            hash_file(&project.path(project.manifest.vocals.as_ref().unwrap().file.as_str()))?,
        )),
        None => None,
    };
    let reference = vocals.as_ref().and_then(|(_, hash)| {
        Contour::load(&project.reference_cache())
            .ok()
            .filter(|c| c.analyzer_version == ANALYZER_VERSION && &c.source_hash == hash)
    });

    let mut loaded_takes = Vec::new();
    let n = project.manifest.takes.len().max(1);
    for (k, entry) in project.manifest.takes.iter().enumerate() {
        let wav = project.take_wav(entry.id);
        let audio = match read_wav(&wav) {
            Ok((a, _)) => Arc::new(a),
            Err(e) => {
                ev(WorkerEvent::Error(format!(
                    "Take {} could not be read and was skipped: {e:#}",
                    entry.id
                )));
                continue;
            }
        };
        let contour = Contour::load(&project.take_file(entry.id, "f0"))
            .ok()
            .filter(|c| c.analyzer_version == ANALYZER_VERSION);
        let scores = takes::load_scores(&project.take_file(entry.id, "scores.json"));
        loaded_takes.push(LoadedTake {
            entry: entry.clone(),
            audio,
            contour,
            scores,
        });
        ev(WorkerEvent::Progress {
            label: label.clone(),
            fraction: (k + 1) as f32 / n as f32,
        });
    }
    Ok(LoadedProject {
        project,
        backing,
        vocals,
        reference,
        takes: loaded_takes,
        recovered,
    })
}
