//! Autosave: while the project has unsaved changes, a recovery copy of it is written every
//! two minutes (and right after an audio recording ends) into the session folder (see
//! `recovery.rs`), never over the project's own file. The next start offers a copy that was
//! left behind, and quitting asks about unsaved changes first.
//!
//! The copy's content is put together on the interface thread (the project's document and
//! the running plug-ins' settings) and written by a thread of its own.
use crate::file_commands::FileCmd;
use crate::i18n::{tr, trf};
use crate::recovery::{self, Candidate};
use crate::App;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often a project with unsaved changes gets a new recovery copy.
pub const EVERY: Duration = Duration::from_secs(120);
/// How long a copy that is due waits while it cannot be written (a recording is running, the
/// copy before it is still being written).
const RETRY: Duration = Duration::from_secs(5);

pub struct Autosave {
    /// Off in runs that measure or draw pictures, which leave the user's folders alone.
    on: bool,
    /// The session folder the copies go to; `None`: this run's (`recovery::session_dir`).
    dir: Option<PathBuf>,
    /// When the next copy is due.
    next: Instant,
    /// The copy being written: how long it took, or why it failed.
    writer: Option<JoinHandle<Result<Duration, String>>>,
    /// Recovery copies found at start that are still to be offered, newest first.
    pub offers: Vec<Candidate>,
    /// The window may close without the "save changes?" question: it was answered, or this
    /// run never asks.
    pub quit_confirmed: bool,
    /// The answer to that question was Don't Save.
    pub dont_save: bool,
}

impl Default for Autosave {
    fn default() -> Self {
        Self::off()
    }
}

impl Autosave {
    /// No recovery copies are written and none are offered.
    pub fn off() -> Self {
        Self {
            on: false,
            dir: None,
            next: Instant::now() + EVERY,
            writer: None,
            offers: Vec::new(),
            quit_confirmed: false,
            dont_save: false,
        }
    }

    /// Copies go to this run's session folder; `offers` are the copies found at start.
    pub fn on(offers: Vec<Candidate>) -> Self {
        Self {
            on: true,
            offers,
            ..Self::off()
        }
    }

    #[cfg(test)]
    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            dir: Some(dir),
            ..Self::on(Vec::new())
        }
    }

    fn dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(recovery::session_dir)
    }

    /// A copy is written as soon as it can be (a recording has just ended).
    pub fn due_now(&mut self) {
        self.next = Instant::now();
    }

    /// Waits for the copy being written, if any.
    fn wait(&mut self) {
        let Some(writer) = self.writer.take() else {
            return;
        };
        match writer.join() {
            Ok(Ok(took)) => tracing::debug!("recovery copy written in {took:?}"),
            Ok(Err(e)) => tracing::warn!("the recovery copy could not be written: {e}"),
            Err(_) => tracing::warn!("the recovery copy's thread ended early"),
        }
    }
}

impl App {
    /// Every frame: writes a recovery copy when one is due, and asks to be drawn again when
    /// the next one is.
    pub fn autosave_tick(&mut self, ctx: &egui::Context) {
        if !self.autosave.on {
            return;
        }
        if self
            .autosave
            .writer
            .as_ref()
            .is_some_and(|w| w.is_finished())
        {
            self.autosave.wait();
        }
        let now = Instant::now();
        if now < self.autosave.next {
            ctx.request_repaint_after(self.autosave.next - now);
            return;
        }
        // Not over a copy still being written, not while a take is being recorded (its file
        // is not whole yet), and not before the copies found at start have been answered.
        let busy = self.autosave.writer.is_some()
            || self.ui.rec.active()
            || self.ui.audio_rec.active()
            || !self.autosave.offers.is_empty();
        let wait = if busy {
            RETRY
        } else {
            self.write_recovery_copy();
            EVERY
        };
        self.autosave.next = now + wait;
        ctx.request_repaint_after(wait);
    }

    /// Starts writing a recovery copy of the project as it is now, if it has unsaved changes
    /// (returns whether it has). Without any, a copy from before is removed: there is
    /// nothing to recover.
    pub fn write_recovery_copy(&mut self) -> bool {
        self.autosave.wait();
        let dir = self.autosave.dir();
        if !self.has_unsaved_changes() {
            recovery::clear_copy(&dir);
            return false;
        }
        // The plug-ins' settings as they are now go into a copy of the project: the project
        // itself is left as it is (reading them into it is what Save does).
        let mut copy = self.project.clone();
        self.plugins.states_into(&mut copy);
        let snap = recovery::snapshot(&copy, self.path.as_deref());
        let job = move || {
            let started = Instant::now();
            recovery::write(&dir, &snap)
                .map(|()| started.elapsed())
                .map_err(|e| e.to_string())
        };
        match std::thread::Builder::new()
            .name("recovery-copy".into())
            .spawn(job)
        {
            Ok(writer) => self.autosave.writer = Some(writer),
            Err(e) => tracing::warn!("the recovery copy could not be written: {e}"),
        }
        true
    }

    /// The project was saved, or another one took its place: its recovery copy goes, and the
    /// two minutes start again.
    pub fn drop_recovery_copy(&mut self) {
        if !self.autosave.on {
            return;
        }
        self.autosave.wait();
        recovery::clear_copy(&self.autosave.dir());
        self.autosave.next = Instant::now() + EVERY;
    }

    /// The window was asked to close (its close button, Quit): with unsaved changes the
    /// close is held back and the "save changes?" question is asked first.
    pub fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if self.autosave.quit_confirmed || !self.has_unsaved_changes() {
            self.prepare_exit();
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ui.ask_save = Some(FileCmd::Quit);
            ctx.request_repaint();
        }
    }

    /// The program is about to close: decides whether the session folder stays for the next
    /// start (returned; `None` when autosave is off). It stays, with a last recovery copy,
    /// when the project has unsaved changes and either nobody was asked about them or the
    /// answer was Don't Save while the project uses audio recorded in this session that is
    /// in no saved file. Otherwise it goes.
    pub fn prepare_exit(&mut self) -> Option<bool> {
        if !self.autosave.on {
            return None;
        }
        let dir = self.autosave.dir();
        let wanted = self.has_unsaved_changes()
            && (!self.autosave.dont_save || recovery::has_unsaved_recordings(&self.project, &dir));
        if wanted {
            self.write_recovery_copy();
        }
        self.autosave.wait();
        if !wanted {
            recovery::clear_copy(&dir);
        }
        let keep = wanted && dir.join(recovery::COPY).is_file();
        if self.autosave.dir.is_none() {
            recovery::keep_on_exit(keep);
        }
        Some(keep)
    }

    /// The question about a recovery copy found at start. Drawn like the save prompt; it has
    /// no Cancel, so Escape or a click beside it leaves it open.
    pub fn show_recover_prompt(&mut self, ctx: &egui::Context) {
        let Some(c) = self.autosave.offers.first().cloned() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique(("file", "recover_prompt"))).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(crate::APP_NAME);
            ui.add_space(6.0);
            ui.label(trf(
                "Recover the project \"{}\" as it was on {}?",
                &[&c.name, &recovery::local_time_text(c.written)],
            ));
            ui.label(tr(
                "If you discard it, the recovery copy and its recordings are deleted.",
            ));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Discard")).clicked() {
                    answer = Some(false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Recover")).clicked() {
                        answer = Some(true);
                    }
                });
            });
        });
        match answer {
            Some(true) => {
                self.recover(&c, ctx);
                // One project at a time: the other copies are offered at the next start.
                self.autosave.offers.clear();
            }
            Some(false) => {
                recovery::discard(&c);
                self.autosave.offers.remove(0);
            }
            None => return,
        }
        ctx.request_repaint();
    }

    /// Opens the recovery copy `c` as the project: unsaved, and with the file it belongs to
    /// as its own, so Save writes there. Its session folder becomes this run's.
    pub fn recover(&mut self, c: &Candidate, ctx: &egui::Context) {
        match recovery::open(c, ctx) {
            Ok((p, warnings)) => {
                self.replace(p, c.original.clone());
                self.saved = recovery::never_saved();
                if self.autosave.dir.is_some() {
                    self.autosave.dir = Some(c.dir.clone());
                } else if !recovery::adopt(&c.dir) {
                    tracing::warn!("{} is in use by another Pro Wave", c.dir.display());
                }
                if !warnings.is_empty() {
                    self.ui.status = Some(trf("missing media: {}", &[&warnings.join(", ")]));
                }
                self.ui.missing_audio = warnings;
            }
            Err(e) => {
                let copy = c.dir.join(recovery::COPY);
                self.ui.status = Some(trf("cannot open {}: {}", &[&copy.display(), &e]));
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::file_commands::tests::app;
    use crate::model::Project;
    use crate::project_io;
    use crate::recovery::{Session, COPY};
    use std::path::Path;
    use std::time::SystemTime;

    /// An app whose recovery copies go to a new session folder in `root`.
    fn app_in(root: &Path) -> (App, Session) {
        let session = Session::start(root).unwrap();
        let mut a = app();
        a.autosave = Autosave::in_dir(session.dir().to_path_buf());
        (a, session)
    }

    /// A take as the recorder leaves it: a file in the session's recordings, on a new track.
    fn record_take(a: &mut App, session: &Session, ctx: &egui::Context) -> PathBuf {
        let take = recovery::recordings_in(session.dir()).join("Audio 1-1.wav");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        project_io::tests::write_wav(&take);
        crate::import::import_audio(&mut a.project, &take, ctx).unwrap();
        take
    }

    fn tick_when_due(a: &mut App, ctx: &egui::Context) {
        a.autosave.due_now();
        a.autosave_tick(ctx);
        a.autosave.wait();
    }

    #[test]
    fn a_copy_is_written_only_while_there_are_unsaved_changes() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        let copy = session.dir().join(COPY);

        tick_when_due(&mut a, &ctx);
        assert!(!copy.exists(), "nothing changed: no copy");

        a.project.tempo = 97.0;
        // Not due yet: two minutes have not passed.
        a.autosave_tick(&ctx);
        a.autosave.wait();
        assert!(!copy.exists());
        tick_when_due(&mut a, &ctx);
        assert!(copy.is_file());
        assert!(a.autosave.next > Instant::now() + EVERY - Duration::from_secs(5));
        assert!(a.has_unsaved_changes(), "a recovery copy is not a save");
        assert_eq!(a.path, None);

        // The change is taken back: nothing to recover, the copy goes.
        a.project.tempo = Project::demo().tempo;
        tick_when_due(&mut a, &ctx);
        assert!(!copy.exists());
    }

    #[test]
    fn the_copy_never_touches_the_projects_own_file_and_goes_when_it_is_saved() {
        let root = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        assert!(a.save_to(&projects.path().join("Song")));
        let file = projects.path().join("Song.prowave");
        let before = std::fs::read(&file).unwrap();

        a.project.tempo = 61.0;
        tick_when_due(&mut a, &ctx);
        let copy = session.dir().join(COPY);
        assert!(copy.is_file());
        assert_eq!(std::fs::read(&file).unwrap(), before);
        let (on_disk, _) = project_io::open_project(&file, &ctx).unwrap();
        assert_ne!(on_disk.tempo, 61.0);
        assert_eq!(a.path.as_deref(), Some(file.as_path()));

        assert!(a.save_to(&file));
        assert!(!copy.exists(), "saved: nothing left to recover");
        assert_ne!(std::fs::read(&file).unwrap(), before);
    }

    #[test]
    fn no_copy_is_written_before_the_offers_found_at_start_are_answered() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        a.project.tempo = 97.0;
        a.autosave.offers.push(Candidate {
            dir: root.path().join("gone"),
            name: "Other".into(),
            original: None,
            written: SystemTime::now(),
            unclean: true,
        });
        tick_when_due(&mut a, &ctx);
        assert!(!session.dir().join(COPY).exists());
        assert!(a.autosave.next < Instant::now() + RETRY + Duration::from_secs(1));
    }

    #[test]
    fn a_crashed_sessions_copy_is_recovered_as_an_unsaved_project_of_the_original_file() {
        let root = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        assert!(a.save_to(&projects.path().join("Song")));
        let file = projects.path().join("Song.prowave");
        let take = record_take(&mut a, &session, &ctx);
        a.project.tempo = 61.0;
        tick_when_due(&mut a, &ctx);
        // The crash: nothing is cleaned up.
        drop(a);
        let dir = session.crash();

        let (mut b, own) = app_in(root.path());
        b.autosave.offers = recovery::sweep(root.path(), SystemTime::now());
        assert_eq!(b.autosave.offers.len(), 1, "the copy is offered");
        let offer = b.autosave.offers[0].clone();
        assert_eq!(offer.name, "Song");
        assert!(offer.unclean);

        // Drawing the question (nobody clicks) keeps it open.
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            b.show_recover_prompt(ui.ctx())
        });
        out.textures_delta.clear();
        assert_eq!(b.autosave.offers.len(), 1);

        b.recover(&offer, &ctx);
        assert_eq!(b.project.tempo, 61.0);
        assert_eq!(b.project.name, "Song");
        assert_eq!(
            b.path.as_deref(),
            Some(file.as_path()),
            "the original's path"
        );
        assert!(b.has_unsaved_changes(), "recovered is not saved");
        assert!(b.ui.missing_audio.is_empty());
        let region = &b.project.tracks.last().unwrap().regions[0];
        assert_eq!(region.audio.as_ref().unwrap().file.path, take);
        assert_eq!(
            b.autosave.dir(),
            dir,
            "the old session folder is this run's now"
        );
        assert!(
            dir.join(COPY).is_file(),
            "still there until the project is saved"
        );

        // Save writes back to the original, with the take in it.
        b.run_file_cmd(FileCmd::Save, &ctx);
        assert!(!b.has_unsaved_changes());
        assert!(!dir.join(COPY).exists());
        let names = prowave_project::media_names(&file).unwrap();
        assert!(
            names.iter().any(|n| n.ends_with("Audio 1-1.wav")),
            "{names:?}"
        );
        let (saved, missing) = project_io::open_project(&file, &ctx).unwrap();
        assert!(missing.is_empty());
        assert_eq!(saved.tempo, 61.0);
        own.end(false);
    }

    #[test]
    fn discarding_an_offer_deletes_the_copy_and_its_takes() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        let take = record_take(&mut a, &session, &ctx);
        tick_when_due(&mut a, &ctx);
        drop(a);
        let dir = session.crash();

        let (mut b, _own) = app_in(root.path());
        b.autosave.offers = recovery::sweep(root.path(), SystemTime::now());
        assert_eq!(b.autosave.offers.len(), 1);
        let tracks = b.project.tracks.len();
        recovery::discard(&b.autosave.offers.remove(0));
        assert!(!dir.exists() && !take.exists());
        assert_eq!(b.project.tracks.len(), tracks);
        assert!(recovery::sweep(root.path(), SystemTime::now()).is_empty());
    }

    #[test]
    fn a_take_survives_quitting_without_saving_and_is_offered_at_the_next_start() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let (mut a, session) = app_in(root.path());
        let take = record_take(&mut a, &session, &ctx);
        let dir = session.dir().to_path_buf();

        // Quit, Don't Save: less than two minutes in, so no copy was written yet.
        assert!(!dir.join(COPY).exists());
        a.autosave.dont_save = true;
        assert_eq!(a.prepare_exit(), Some(true), "the folder stays");
        drop(a);
        session.end(true);
        assert!(take.is_file(), "the take is still on disk");
        assert!(dir.join(COPY).is_file());

        let (mut b, own) = app_in(root.path());
        let offers = recovery::sweep(root.path(), SystemTime::now());
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].unclean, "that run ended cleanly");
        b.recover(&offers[0], &ctx);
        assert_eq!(b.path, None, "never saved: Save asks where");
        assert!(b.has_unsaved_changes());
        let region = &b.project.tracks.last().unwrap().regions[0];
        assert!(region.missing.is_none());
        assert_eq!(region.audio.as_ref().unwrap().file.path, take);
        own.end(false);
    }

    #[test]
    fn quitting_without_saving_keeps_nothing_when_no_audio_would_be_lost() {
        let root = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();

        // Changes, but no recordings: Don't Save means what it says.
        let (mut a, session) = app_in(root.path());
        a.project.tempo = 97.0;
        tick_when_due(&mut a, &ctx);
        assert!(session.dir().join(COPY).is_file());
        a.autosave.dont_save = true;
        assert_eq!(a.prepare_exit(), Some(false));
        assert!(!session.dir().join(COPY).exists());
        let dir = session.dir().to_path_buf();
        session.end(false);
        assert!(!dir.exists());

        // A take that is in the saved project file is not lost either.
        let (mut a, session) = app_in(root.path());
        record_take(&mut a, &session, &ctx);
        assert!(a.save_to(&projects.path().join("Song")));
        a.project.tempo = 97.0;
        a.autosave.dont_save = true;
        assert_eq!(a.prepare_exit(), Some(false));
        session.end(false);

        // Nothing unsaved at all.
        let (mut a, session) = app_in(root.path());
        assert_eq!(a.prepare_exit(), Some(false));
        session.end(false);

        // Unsaved changes and nobody was asked (the window went another way): kept.
        let (mut a, session) = app_in(root.path());
        a.project.tempo = 97.0;
        assert_eq!(a.prepare_exit(), Some(true));
        session.end(true);
        assert_eq!(recovery::sweep(root.path(), SystemTime::now()).len(), 1);
    }

    #[test]
    fn closing_the_window_with_unsaved_changes_asks_first() {
        let ctx = egui::Context::default();
        let close = || {
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.entry(input.viewport_id).or_default();
            viewport.events.push(egui::ViewportEvent::Close);
            input
        };
        let cancelled = |out: &egui::FullOutput| {
            out.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
            })
        };
        let closes = |out: &egui::FullOutput| {
            out.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Close))
            })
        };

        // Nothing to lose: the window closes.
        let mut clean = app();
        let mut out = ctx.run_ui(close(), |ui| clean.handle_close_request(ui.ctx()));
        out.textures_delta.clear();
        assert!(!cancelled(&out) && clean.ui.ask_save.is_none());

        // Unsaved changes: the close is held back and the question is up.
        let mut dirty = app();
        dirty.project.tempo += 1.0;
        let mut out = ctx.run_ui(close(), |ui| dirty.handle_close_request(ui.ctx()));
        out.textures_delta.clear();
        assert!(cancelled(&out));
        assert_eq!(dirty.ui.ask_save, Some(FileCmd::Quit));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            dirty.show_save_prompt(ui.ctx())
        });
        out.textures_delta.clear();
        assert_eq!(
            dirty.ui.ask_save,
            Some(FileCmd::Quit),
            "until it is answered"
        );
        assert!(!dirty.autosave.quit_confirmed);

        // Don't Save: the window is closed again, and this time nothing holds it back.
        dirty.ui.ask_save = None;
        dirty.answer_save_prompt(FileCmd::Quit, crate::file_commands::Answer::DontSave, &ctx);
        assert!(dirty.autosave.quit_confirmed && dirty.autosave.dont_save);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        assert!(closes(&out), "the close is asked for again");
        let mut out = ctx.run_ui(close(), |ui| dirty.handle_close_request(ui.ctx()));
        out.textures_delta.clear();
        assert!(!cancelled(&out));

        // Cancel: the project stays open and nothing closes.
        let mut kept = app();
        kept.project.tempo += 1.0;
        kept.answer_save_prompt(FileCmd::Quit, crate::file_commands::Answer::Cancel, &ctx);
        assert!(!kept.autosave.quit_confirmed && !kept.autosave.dont_save);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        assert!(!closes(&out));

        // Save: saved to its file, then the window closes.
        let dir = tempfile::tempdir().unwrap();
        let mut saved = app();
        assert!(saved.save_to(&dir.path().join("Song")));
        saved.project.tempo += 1.0;
        saved.answer_save_prompt(FileCmd::Quit, crate::file_commands::Answer::Save, &ctx);
        assert!(saved.autosave.quit_confirmed && !saved.has_unsaved_changes());
        let (on_disk, _) =
            project_io::open_project(&dir.path().join("Song.prowave"), &ctx).unwrap();
        assert_eq!(on_disk.tempo, saved.project.tempo);
    }

    /// How long a recovery copy takes, on the demo project and on the biggest test project
    /// (`bench::many_tracks(1000)`): the part on the interface thread (the project copied and
    /// its document made) and the part on the writing thread. Printed, not judged: run with
    /// `-- --nocapture`.
    #[test]
    fn timing_of_a_recovery_copy() {
        let root = tempfile::tempdir().unwrap();
        for (name, project) in [
            ("demo", Project::demo()),
            ("1000 tracks", crate::bench::many_tracks(1000)),
        ] {
            let host = crate::plugins::PluginHost::default();
            let (mut ui_thread, mut writer, mut size) = (Vec::new(), Vec::new(), 0);
            for _ in 0..5 {
                let started = Instant::now();
                let mut copy = project.clone();
                host.states_into(&mut copy);
                let snap = recovery::snapshot(&copy, None);
                ui_thread.push(started.elapsed());
                let started = Instant::now();
                recovery::write(root.path(), &snap).unwrap();
                writer.push(started.elapsed());
                size = std::fs::metadata(root.path().join(COPY)).unwrap().len();
            }
            ui_thread.sort();
            writer.sort();
            eprintln!(
                "recovery copy, {name} ({} tracks, {size} bytes): interface thread {:?} \
                 (worst {:?}), writing thread {:?} (worst {:?})",
                project.tracks.len(),
                ui_thread[2],
                ui_thread[4],
                writer[2],
                writer[4],
            );
        }
    }

    /// `recover-prompt.png` and `quit-prompt.png`: the two questions over the demo project.
    #[test]
    #[ignore = "pictures"]
    fn picture_of_the_recover_and_quit_prompts() {
        let dir = std::env::var_os("MANUAL_PNG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("prowave-manual"));
        std::fs::create_dir_all(&dir).unwrap();
        let meters = crate::mixer::Meters::default();
        let backdrop = |ui: &mut egui::Ui, a: &mut App| {
            crate::theme::apply(ui.ctx());
            crate::tracks_area::show(ui, &mut a.project, &mut a.ui, &meters);
        };

        let mut a = app();
        a.autosave.offers.push(Candidate {
            dir: dir.join("no-such-session"),
            name: "Song 1".into(),
            original: None,
            // 2026-11-10 14:32 UTC.
            written: SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_321_127),
            unclean: true,
        });
        crate::ui_test::render_png(
            900.0,
            420.0,
            2.0,
            egui::Color32::from_gray(0x2a),
            |ui| {
                backdrop(ui, &mut a);
                a.show_recover_prompt(ui.ctx());
            },
            &dir.join("recover-prompt.png"),
        )
        .unwrap();

        let mut a = app();
        a.project.name = "Song 1".into();
        a.ui.ask_save = Some(FileCmd::Quit);
        crate::ui_test::render_png(
            900.0,
            420.0,
            2.0,
            egui::Color32::from_gray(0x2a),
            |ui| {
                backdrop(ui, &mut a);
                a.show_save_prompt(ui.ctx());
            },
            &dir.join("quit-prompt.png"),
        )
        .unwrap();
    }
}
