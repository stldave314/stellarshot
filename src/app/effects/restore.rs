// SPDX-License-Identifier: GPL-3.0-only

//! The restore page's effects.

use super::super::*;

impl App {
    pub(in crate::app) fn run_restore_effects(
        &mut self,
        effects: Vec<restore::Effect>,
    ) -> Task<Message> {
        let Some((page, secret)) = &self.restore else {
            return Task::none();
        };
        let Some(profile) = self.config.profile(&page.profile_id).cloned() else {
            // The backup is gone (removed elsewhere, or by an import). The
            // page cannot do anything real any more, but it must still be
            // able to close: the sidebar is hidden while it is showing, so
            // returning here without handling Back left it the only way
            // out to be quitting the application.
            for effect in effects {
                match effect {
                    restore::Effect::Close => self.restore = None,
                    restore::Effect::ShowError(context, error) => {
                        self.show_error(&context, &error);
                    }
                    _ => {}
                }
            }
            return Task::none();
        };
        let secret = secret.clone();
        let browser = page.browser();
        let session = self.restore_session;
        let to_page = move |message: restore::Message| app(Message::RestorePage(session, message));
        let mut tasks = Vec::new();
        for effect in effects {
            let browser = browser.clone();
            let task = match effect {
                restore::Effect::Load => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    Task::perform(
                        tasks::blocking(move || {
                            let location = profile.location()?;
                            Ok(std::sync::Arc::new(
                                engine::open(&location, &secret)?.browse()?,
                            ))
                        }),
                        move |result| to_page(restore::Message::Loaded(result)),
                    )
                }
                restore::Effect::List { snapshot, dir } => {
                    let listed = (snapshot.clone(), dir.clone());
                    Task::perform(
                        tasks::blocking(move || browsing(browser)?.list(&snapshot, &dir)),
                        move |result| {
                            let (snapshot, dir) = listed.clone();
                            to_page(restore::Message::Listed(snapshot, dir, result))
                        },
                    )
                }
                restore::Effect::Search { snapshot, query } => {
                    let asked = (snapshot.clone(), query.clone());
                    Task::perform(
                        tasks::blocking(move || {
                            browsing(browser)?.search(&snapshot, &query, RESTORE_RESULT_LIMIT)
                        }),
                        move |result| {
                            let (snapshot, query) = asked.clone();
                            to_page(restore::Message::Found(snapshot, query, result))
                        },
                    )
                }
                restore::Effect::Versions(path) => {
                    let asked = path.clone();
                    Task::perform(
                        tasks::blocking(move || browsing(browser)?.versions(&path)),
                        move |result| {
                            to_page(restore::Message::VersionsLoaded(asked.clone(), result))
                        },
                    )
                }
                restore::Effect::Missing { scope, since } => {
                    let asked = scope.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            browsing(browser)?.missing(&scope, since, RESTORE_RESULT_LIMIT)
                        }),
                        move |result| {
                            to_page(restore::Message::MissingFound(asked.clone(), since, result))
                        },
                    )
                }
                restore::Effect::Diff { from, to } => {
                    let asked = (from.clone(), to.clone());
                    Task::perform(
                        tasks::blocking(move || browsing(browser)?.diff(&from, &to)),
                        move |result| {
                            let (from, to) = asked.clone();
                            to_page(restore::Message::Compared(from, to, result))
                        },
                    )
                }
                restore::Effect::GlobalSearch { query } => {
                    let asked = query.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            browsing(browser)?.search_all(&query, RESTORE_RESULT_LIMIT)
                        }),
                        move |result| to_page(restore::Message::GlobalFound(asked.clone(), result)),
                    )
                }
                restore::Effect::Preview(requests) => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    let asked = requests.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            let location = profile.location()?;
                            let mut total = engine::RestorePreview::default();
                            for request in &requests {
                                let part =
                                    engine::open(&location, &secret)?.preview_restore(request)?;
                                total.files += part.files;
                                total.bytes += part.bytes;
                                total.unchanged += part.unchanged;
                                total.conflicts += part.conflicts;
                            }
                            Ok(total)
                        }),
                        move |result| to_page(restore::Message::Previewed(asked.clone(), result)),
                    )
                }
                restore::Effect::Restore(request) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            self.show_error(&fl!("restore-failed"), &err);
                            continue;
                        }
                    };
                    let job = Job {
                        restore: Some(request),
                        ..Job::new(repository, secret.clone())
                    };
                    Task::run(child::run(Operation::Restore, job), move |event| {
                        to_page(restore::Message::Restore(event))
                    })
                }
                restore::Effect::OpenCopy { snapshot, path } => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    Task::perform(
                        tasks::blocking(move || open_copy(&profile, &secret, &snapshot, &path)),
                        |result| match result {
                            Ok(()) => app(Message::Noop),
                            Err(err) => app(Message::Dialog(DialogMessage::Failed(
                                fl!("open-copy-failed"),
                                err,
                            ))),
                        },
                    )
                }
                restore::Effect::Download {
                    snapshot,
                    path,
                    is_folder,
                } => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let file_name = if is_folder {
                        format!("{name}.tar.gz")
                    } else {
                        name.into_owned()
                    };
                    Task::perform(
                        async move {
                            let destination =
                                tasks::choose_save_path(fl!("download-title"), file_name).await?;
                            Some(
                                tasks::blocking(move || {
                                    let browser = browsing(browser)?;
                                    if is_folder {
                                        browser.archive_folder(&snapshot, &path, &destination)
                                    } else {
                                        browser.dump_file(&snapshot, &path, &destination)
                                    }
                                })
                                .await,
                            )
                        },
                        |result| match result {
                            None | Some(Ok(())) => app(Message::Noop),
                            Some(Err(err)) => app(Message::Dialog(DialogMessage::Failed(
                                fl!("download-failed"),
                                err,
                            ))),
                        },
                    )
                }
                restore::Effect::PickScope => Task::perform(
                    tasks::pick_folder(fl!("select-scope-folder")),
                    move |path| {
                        path.map_or(app(Message::Noop), |path| {
                            to_page(restore::Message::ScopeChosen(path))
                        })
                    },
                ),
                restore::Effect::PickTarget => Task::perform(
                    tasks::pick_folder(fl!("select-restore-folder")),
                    move |path| {
                        path.map_or(to_page(restore::Message::TargetOriginal), |path| {
                            to_page(restore::Message::TargetChosen(path))
                        })
                    },
                ),
                restore::Effect::PickMountPoint => Task::perform(
                    tasks::pick_folder(fl!("select-mount-folder")),
                    move |path| match path {
                        Some(point) => to_page(restore::Message::MountPointChosen(point)),
                        None => app(Message::Noop),
                    },
                ),
                restore::Effect::Mount { snapshot, point } => {
                    let profile_id = profile.id.clone();
                    let logged_snapshot = snapshot.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            let browser = browsing(browser)?;
                            Ok(engine::mount::mount(browser, snapshot, &point)?.into())
                        }),
                        move |result: Result<restore::MountHandle, EngineError>| {
                            if result.is_ok() {
                                event_log::record(
                                    &profile_id,
                                    format::now(),
                                    event_log::EventKind::Mounted {
                                        snapshot: logged_snapshot.clone(),
                                    },
                                    event_log::Source::Desktop,
                                );
                            }
                            to_page(restore::Message::Mounted(result))
                        },
                    )
                }
                restore::Effect::OpenMounted(path) => {
                    if let Err(err) = open::that_detached(&path) {
                        error_log!(UI, "failed to open mounted folder {path:?}: {err}");
                    }
                    Task::none()
                }
                restore::Effect::Unmount(handle) => {
                    let profile_id = profile.id.clone();
                    let snapshot = handle.snapshot().to_owned();
                    Task::perform(
                        tasks::blocking(move || {
                            drop(handle);
                            Ok(())
                        }),
                        move |_: Result<(), EngineError>| {
                            event_log::record(
                                &profile_id,
                                format::now(),
                                event_log::EventKind::Unmounted {
                                    snapshot: snapshot.clone(),
                                },
                                event_log::Source::Desktop,
                            );
                            app(Message::Noop)
                        },
                    )
                }
                restore::Effect::ShowError(context, error) => {
                    self.show_error(&context, &error);
                    Task::none()
                }
                restore::Effect::Restored(done) => {
                    event_log::record(
                        &profile.id,
                        format::now(),
                        event_log::EventKind::Restored {
                            files: done.files,
                            bytes: done.bytes,
                        },
                        event_log::Source::Desktop,
                    );
                    self.dialogs.notify(Dialog::Info(
                        fl!("restore-done-title"),
                        fl!(
                            "restore-done-body",
                            count = (done.files as i64),
                            size = format::bytes(done.bytes),
                            conflicts = (done.conflicts as i64)
                        ),
                    ));
                    Task::none()
                }
                restore::Effect::Close => {
                    self.restore = None;
                    remove_old_open_copies()
                }
            };
            tasks.push(task);
        }
        Task::batch(tasks)
    }
}

/// The browser the restore page opened, or an error if it has not finished
/// opening.
pub(super) fn browsing(
    browser: Option<std::sync::Arc<engine::Browser>>,
) -> Result<std::sync::Arc<engine::Browser>, EngineError> {
    browser
        .ok_or_else(|| EngineError::new(engine::ErrorKind::Internal, "the backup is still opening"))
}

/// Restore one file from `snapshot` into a private temporary folder, make it
/// read-only, and open it with the default application: a way to look at an
/// old version without touching the current one.
fn open_copy(
    profile: &Profile,
    secret: &engine::Secret,
    snapshot: &str,
    path: &std::path::Path,
) -> Result<(), EngineError> {
    use std::os::unix::fs::PermissionsExt;
    let folder =
        engine::lock::runtime_dir().join(format!("open-{}", uuid::Uuid::new_v4().simple()));
    let request = engine::RestoreRequest {
        snapshot: snapshot.to_owned(),
        paths: vec![path.to_path_buf()],
        target: engine::Target::Folder(folder.clone()),
        policy: engine::ConflictPolicy::Overwrite,
        ..engine::RestoreRequest::default()
    };
    let location = profile.location()?;
    // The copy lives in memory-backed storage: ask how big it is before
    // writing any of it.
    let size = engine::open(&location, secret)?
        .preview_restore(&request)?
        .bytes;
    if size > OPEN_COPY_MAX_BYTES {
        return Err(EngineError::new(
            engine::ErrorKind::TooLargeToOpen,
            size.to_string(),
        ));
    }
    std::fs::create_dir_all(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    engine::open(&location, secret)?.restore(&request, std::sync::Arc::new(engine::NoProgress))?;
    let copy = folder.join(path.file_name().unwrap_or_default());
    // A snapshot's node can claim to be a symlink (see SEC-2 in the review
    // plan): for a repository shared with someone else, that is not
    // necessarily this process's own doing. `symlink_metadata` (unlike
    // `metadata`) does not follow it, so this refuses to chmod or open
    // whatever it points at — `~/.ssh`, say — instead of trusting that
    // "restored into a private folder this process just created" also means
    // "definitely a plain file".
    if !std::fs::symlink_metadata(&copy)?.is_file() {
        return Err(EngineError::new(
            engine::ErrorKind::Internal,
            format!("{} did not restore as a plain file", copy.display()),
        ));
    }
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o400))?;
    open::that_detached(&copy)?;
    Ok(())
}
