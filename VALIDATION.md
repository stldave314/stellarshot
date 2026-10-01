# Validation

How Stellarshot is checked, what each check proves, and how to run it.

The rule every check here follows: **a check that cannot fail is not a check.**
Each one is written so that a missing fixture, an empty input or a skipped step
produces a failure, not a green result. Security-relevant behavior is proven
against real files and real binaries, never by reading the code.

## Running everything locally

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features      # must report zero warnings
cargo test --all-features
./scripts/verify-release-build.sh
./scripts/validate-metadata.sh                  # needs desktop-file-utils and appstream
./install.sh package                            # needs cargo-deb and cargo-generate-rpm
```

CI runs the same commands on every push and pull request
(`.github/workflows/ci.yml`), with `RUSTFLAGS=-D warnings`, and also:

- installs the built `.deb` and `.rpm` in clean containers, starts the
  program, checks the installed files (and runs `lintian` on the `.deb`), then
  removes them;
- installs the tarball as root and checks that `/usr` keeps its ownership;
- scans the packaged binaries with `scripts/verify-packaged-binaries-strip-logging.sh`
  (no debug logging) and `scripts/verify-packaged-binaries-auditable.sh`
  (dependency list embedded by `cargo auditable`);
- runs `cargo deny check` (advisories, licenses, sources) and builds on the
  minimum supported Rust version.

A tag runs the build and tests again before publishing
(`.github/workflows/release.yml`), checks the tag against `Cargo.toml`, and
publishes `SHA256SUMS` and a build-provenance attestation with the packages.

## Automated checks

### Repository safety (`src/engine/location.rs`, `src/engine/tests.rs`)

These guard against the upstream behavior that could have deleted a home
directory. They run against real directories in temporary folders.

| Test | What it proves |
| --- | --- |
| `delete_repository_leaves_foreign_files` | Deleting a repository removes every repository entry, leaves a neighboring `Documents/report.odt` byte-for-byte intact, keeps the folder, and reports what it left |
| `delete_repository_removes_the_folder_when_nothing_else_is_there` | A folder holding only a repository is removed completely |
| `delete_refuses_a_folder_that_is_not_a_repository` | A folder without `config` and `keys` is refused, and nothing in it is touched |
| `delete_does_not_follow_a_symlinked_entry` | A `snapshots` entry that is a symlink to another folder is removed as a link; the target's files survive |
| `init_refuses_non_empty_non_repository` | A folder with the user's own files is classified as unusable |
| `init_refuses_existing_and_non_empty_locations` | The engine's `init` refuses both an existing repository and a folder of the user's files, and writes nothing into the latter |
| `url_with_space_becomes_real_path` (`src/app/portal.rs`) | `file:///tmp/My%20Backups` becomes `/tmp/My Backups`; the test also asserts that `Url::path()` would have kept `%20`, so it fails if the premise stops being true |

### The engine (`src/engine/tests.rs`)

Real repositories in temporary folders, with a fixture tree containing names
with spaces, unicode and `%`, a symlink, an empty folder, a 64 KiB binary file
and a mode-0600 file.

| Test | What it proves |
| --- | --- |
| `round_trip_preserves_tree` | Back up, restore, and compare every entry: content, symlink target, permissions, empty folders. **Cannot pass vacuously:** it first asserts the fixture has at least ten entries |
| `excluded_folder_is_not_in_the_snapshot` | An excluded folder inside a source is absent after restore, and the rest is present |
| `exclude_pattern_applies_at_any_depth` | `node_modules` and `*.tmp` are excluded wherever they occur |
| `wrong_password_is_reported_as_such` | A wrong password is `WrongPassword`, not a generic failure |
| `open_non_repository_is_not_a_repository` / `unreachable_location_is_reported_as_unavailable` | A folder of other files, and a path whose parent does not exist (an unplugged drive), are told apart |
| `second_backup_is_incremental` | After changing one small file, the unchanged 64 KiB file is not stored again |
| `snapshots_are_listed_newest_first_and_can_be_deleted` | Ordering, short IDs, and deleting the right snapshot |
| `check_passes_on_a_sound_repository` / `check_reports_a_damaged_repository` | The integrity check passes a sound repository **and fails** one whose index was removed, so it is known to detect damage |
| `backup_reports_progress_ending_at_the_total` | Progress reports every byte of every regular file |
| `throttle_sends_first_and_final` (`progress.rs`) | 1,000 rapid increments produce few reports, and the last one is exact |
| `a_finished_upload_repeats_the_last_event_with_the_bytes_stored` (`progress.rs`) | A finished upload is reported even while reading stands still, so the progress card keeps moving |
| `packs_upload_side_by_side` (`uploads.rs`) | Eight packs to storage that takes 200 ms per write go four at a time: at some point during the run, four writes were genuinely in flight together (`most_busy == 4`), which sequential uploads could never produce regardless of how fast they ran — an elapsed-time assertion used to stand in for this and flaked on a busy runner for reasons that had nothing to do with whether uploads were actually concurrent |
| `an_index_is_written_only_after_every_pack_arrived` (`uploads.rs`) | Writing an index waits for every pack in flight: all six are stored when it returns |
| `a_failed_upload_fails_the_index_and_everything_after_it` (`uploads.rs`) | When one pack upload fails, the index write fails, so no index can name a missing pack, and so does every write after it; only packs reach the storage |
| `a_panicking_upload_fails_the_index_rather_than_hanging_forever` (`uploads.rs`) | REL-7: a worker whose pack write *panics*, rather than returning `Err`, no longer unwinds past the `in_flight` bookkeeping and leaves `settle`/`Drop` waiting on the condvar forever — the index write returns `Err` (not a hang) within a 2-second `recv_timeout`, run on its own thread so a regression would hang only that one assertion |

### Browsing and restoring (`src/engine/tests.rs`)

Real snapshots of the same fixture tree, restored into real folders.

| Test | What it proves |
| --- | --- |
| `lists_a_folder_in_a_snapshot` | A folder's entries, folders first, with their kinds and sizes |
| `search_finds_names_anywhere` | A name is found at any depth, ignoring case |
| `versions_collapse_identical_content` | A file changed once across three snapshots: the version identical to the newer one is marked, the two different ones are not |
| `diff_reports_added_removed_modified` | Each kind of change is reported for the right path |
| `a_snapshot_compared_with_itself_has_no_changes` | No differences are reported |
| `missing_finds_deleted_files_with_their_last_snapshot` | A file deleted from disk is listed with the newest snapshot that still has it |
| `restore_to_a_folder_keeps_names` | Each selected item lands in the chosen folder under its own name |
| `keep_both_never_touches_the_existing_file` / `keep_both_for_a_single_file` | The existing file keeps its content; the restored copy is next to it under a dated name. **Cannot pass vacuously:** the existing file is first changed so the two differ |
| `keep_both_does_not_overwrite_a_file_within_the_same_second` | REL-14: an existing file whose modification time matches the snapshot's to the *second* but not the nanosecond used to look identical to `looks_identical` (which only compared to the second), so Keep Both never renamed it aside — confirmed by reading `rustic_core`'s own source, not assumed, that its own comparison is full-precision and would restore over such a file anyway. Sets a clean second boundary before backing up, then perturbs the mtime by 500ms (never rolling into the next second) and changes the same-length content afterward; the existing content survives a Keep Both restore |
| `overwrite_replaces_changed_files` | Overwrite puts the backed-up content back over an edited file |
| `skip_restores_only_what_is_missing` | In a folder where some files exist and some do not, only the missing ones are written |
| `preview_counts_match_the_restore` | The dry run's counts equal what the restore then does, and the dry run leaves the missing file missing and the edited file edited |
| `preview_creates_nothing` | A dry run into a folder that does not exist yet does not create it |
| `restores_into_a_deleted_folder` | Restoring to the original place recreates missing parent folders |
| `a_file_vs_directory_conflict_is_reported_not_silently_ignored`, `skip_leaves_a_file_blocking_a_directory_completely_untouched`, `keep_both_also_leaves_a_file_blocking_a_directory_untouched`, `a_symlink_vs_existing_directory_conflict_is_reported` | TST-5: a snapshot directory (or a file whose own parent path is a plain file, not a directory) that conflicts in *type*, not just content, with what is already there |

Found while writing the tests above, not by inspection: `restore_one`'s shaping loop treated `on_disk.symlink_metadata()` failing as "does not exist yet, safe to create" unconditionally — which does not distinguish that from "cannot exist because something is in the way higher up." A snapshot directory whose own name was already a plain file at the destination (or any of its descendants, since their own parent path was then blocked too) silently reported zero conflicts under every `ConflictPolicy`, then handed rustic a path it could not actually restore into. Confirmed with a scratch test that printed the real `RestorePreview` and on-disk state before writing any fix — `conflicts: 0` where there plainly was one. Fixed with `ancestor_is_not_a_directory`, checked for both a directory item and a non-directory item whose ancestor is blocked: both now count as a real conflict and respect `Skip`. `KeepBoth` cannot rename a conflicting directory aside the way a file conflict does (every item is shaped independently against the same, fixed destination, with no way to carry a rename down to that directory's own descendants), so it is treated the same as `Skip` — left untouched — rather than attempt a rename that could not actually keep a directory and its contents together.

The restore page's rules are tested without a window (`src/app/pages/restore.rs`):
Keep both and the original place are the defaults; **Restore** does nothing
until a successful dry run exists for exactly the current choices (a late
answer for earlier choices is ignored); a selection spanning several
snapshots becomes one restore per snapshot, run in turn; Cancel drops the rest;
a folder missing from another snapshot falls back to the starting folder, then
the top, then reports an error instead of looping.

SEC-2: a node name taken straight from a snapshot's own tree data (not this app's own filtering, which only ever chooses among names the snapshot already has) could in principle contain a `..` component or be absolute, which `PathBuf::join` would then walk outside the intended base instead of landing inside it — a snapshot's tree is deserialized from repository data, and for a repository shared with another person or machine (`keys.rs`) that data may not be trustworthy. `restore.rs`'s `reject_unsafe_relative_path` rejects any such name with `ErrorKind::UnsafePath` rather than silently restoring, listing, or probing local disk with it; `restore_one` (the one write path), and now `browse.rs`'s `list`, `mount_list`, `search`, `missing` and `diff_trees` (every read path that turns a snapshot's own names into a path) all call it. Own unit tests, in `engine::restore::tests` and `engine::browse::tests`: `a_parent_dir_component_is_rejected`, `a_parent_dir_component_buried_partway_through_is_still_rejected`, `an_absolute_path_is_rejected`, `an_ordinary_relative_path_is_accepted`, `the_empty_path_of_the_restored_item_itself_is_accepted` prove the check itself; `a_maliciously_named_node_is_rejected_when_listed`, `a_maliciously_named_node_is_rejected_when_mounted`, `an_ordinary_node_name_lists_fine` prove `list`'s and `mount_list`'s own wiring calls it, built with `rustic_core::Node::new_node` directly rather than a real backup (no repository or write access needed for a single hostile `Node`, unlike a whole malicious tree). Not proven end to end: an actual crafted, saved snapshot reaching these functions through `repo.ls`/`get_tree` themselves — that would need `rustic_core`'s largely private tree-saving API, a disclosed gap, not a silent one.

### The rclone configuration stays private (`tests/rclone_permissions.rs`, `src/engine/rclone.rs`)

| Test | What it proves |
| --- | --- |
| `the_configuration_is_private_before_rclone_writes_a_token` | A stand-in `rclone` records the configuration's mode at the moment it runs: a file that started at 0644 is already 0600 when the token goes in. **Proven able to fail:** with the tightening moved after the rclone call, as it used to be, the test fails with "644" |
| `signing_in_never_writes_into_a_readable_configuration` (`tests/rclone.rs`) | The same through the real rclone: the file ends 0600 with both the old and the new section |
| `a_new_configuration_is_private_from_the_start` / `a_readable_configuration_is_tightened_before_a_token_goes_in` | A new file is created 0600 in a 0700 folder; copying a remote into a 0644 file tightens it first and loses nothing |

### Retention and pruning (`src/engine/tests.rs`, `src/profile.rs`)

| Test | What it proves |
| --- | --- |
| `forget_applies_the_rules` | Four snapshots a day apart, "keep 2 daily": the two newest days stay and exactly two are removed |
| `forget_leaves_other_computers_alone` | Forgetting on behalf of another host name removes nothing, so a shared repository keeps the other computer's history |
| `forget_keeps_everything_without_rules` | No rules, nothing removed |
| `only_the_newest_snapshot_of_a_day_is_kept`, `days_without_a_backup_are_passed_over_not_counted`, `weeks_and_months_are_counted_the_same_way_and_overlap_with_days`, `the_first_snapshot_is_kept_until_twelve_months_are_covered` (`maintenance.rs`) | Each sentence of the "Smart" explanation in the README and the app, checked against rustic on 400 days of history with a gap: 7 days, 4 weeks and 12 months, the newest of each; gaps skipped; one snapshot counting for all three; the first snapshot kept while under a year and not after |
| `prune_reclaims_forgotten_data` | After forgetting the snapshots that held a 512 KiB file, prune reports at least that much unused; the repository then checks clean and the kept snapshot restores. **Cannot pass vacuously:** it first asserts two snapshots were forgotten |
| `retention_rules`, `prune_defaults_to_on_only_for_this_computer`, `older_settings_load_with_no_prune_choice` | Each "Keep" choice maps to the right rules; freeing space defaults on for local folders and drives only; settings from before M5 load unchanged |

### Exclusions, snapshot pinning and restore options (`src/engine/tests.rs`, 0.3)

| Test | What it proves |
| --- | --- |
| `a_folder_marked_as_a_cache_is_excluded` | A folder holding a `CACHEDIR.TAG` file is left out whole |
| `a_projects_own_gitignore_is_honored_without_needing_a_git_repository` | A plain `.gitignore`, with no `.git` folder at all, still excludes what it lists; the `.gitignore` file itself is still backed up |
| `files_larger_than_the_limit_are_excluded`, `case_insensitive_patterns_match_either_case`, `patterns_kept_in_a_file_are_applied` | The three extra exclusion rules each work; the case-insensitive and file-based ones needed a leading `!` added by Stellarshot itself, caught by a first version of the test failing because the unmodified pattern restricted the backup to only the excluded files instead of leaving them out |
| `excluded_folders_with_glob_metacharacters_in_their_name_are_not_backed_up`, `a_non_utf8_exclude_fails_the_backup_rather_than_including_it` (REL-5, TST-1) | A folder whose real name contains `*`, `?` or `[` is excluded correctly rather than being read as a pattern that could match unrelated files — or the repository's own folder, causing a backup into itself; an exclude path that is not valid UTF-8 fails the backup rather than silently matching nothing and including it |
| `a_non_utf8_named_file_can_be_restored_by_itself`, `keep_both_preserves_a_non_utf8_stem` (REL-6, TST-1) | A non-UTF-8-named file can be restored on its own by its exact path; **Keep Both** still preserves a non-UTF-8 file's exact name stem in its own copy |
| `an_unchanged_backup_is_skipped_when_asked` | Backing up twice with nothing changed adds no second snapshot; a real change afterward still is recorded |
| `a_dry_run_reports_size_without_writing_anything` | A dry run reports the size it would add and leaves the repository with no snapshot; a real backup right after adds exactly that much |
| `extended_attributes_are_saved_and_restored` | A user extended attribute set on a file survives a backup and restore. This needed no production code: rustic saves and restores them by default |
| `a_pinned_snapshot_survives_forget_that_would_otherwise_remove_it`, `pinning_an_already_pinned_snapshot_is_a_harmless_no_op` | Pinning the oldest of several daily snapshots keeps it through a `forget` that would otherwise remove it; pinning twice does not change its ID again. **Caught by the test:** pinning changes a snapshot's ID (it is a hash of the snapshot's own content), so the first version of `set_pinned` returned the old, now-deleted ID; fixed by finding the new one from the snapshot list before and after. A later pass (REL-17) confirmed against rustic's own `rewrite_snapshots`/`save_snapshots` source that it never hands the new ID back to the caller either, so the before/after diff is the only way to find it — and hardened that diff to error rather than silently return the old snapshot if it ever comes back empty or ambiguous (a concurrent writer) |
| `verify_existing_catches_content_that_looks_unchanged` | A restored file corrupted to the same size and modification time as the backup is left alone by default and only rewritten with `verify_existing` on |
| `restoring_with_numeric_or_no_ownership_does_not_fail` | Each ownership choice completes a restore without error. Actually changing an owner needs root; see "Not yet verified end to end" |
| `runner_pins_a_snapshot_and_protects_it_from_maintain` (`tests/runner.rs`) | The same pin, through `--run set-pinned` and then `--run maintain`, the way the window actually calls it |

### Storage and credentials (0.4)

| Test | What it proves |
| --- | --- |
| `a_bandwidth_limit_is_passed_to_rclone`, `no_bandwidth_limit_adds_no_flag`, `a_local_location_ignores_a_bandwidth_limit`, `a_bandwidth_limit_cannot_inject_a_second_rclone_argument` (`src/engine/repo.rs`) | A limit reaches the rclone command as `--bwlimit <value>`, found by splitting the built command with `shell_words` the same way rustic does, not a substring check; none is added when empty; a local destination, which has no transfer to limit, ignores it; a value crafted to look like `1M' --password-command 'evil` survives as one single argument rather than becoming two — a real review finding, see the code-review entry below |
| `a_profiles_bandwidth_limit_reaches_its_rclone_location` (`src/profile.rs`) | The whole path from a profile's own field to the location rustic actually opens |
| `the_commands_output_becomes_the_password`, `a_trailing_newline_is_trimmed_but_no_more_than_one`, `a_failing_command_reports_its_stderr`, `empty_output_is_refused_rather_than_an_empty_password`, `unmatched_quoting_is_reported_rather_than_run_incorrectly` (`src/password_command.rs`) | A password command's real output becomes the password; exactly one trailing newline is trimmed, not more; a failing command's own stderr is the reported error; empty output is refused rather than silently becoming an empty password; unparseable quoting is reported rather than run some other way |
| `a_timeout_kills_a_backgrounded_grandchild_too` (`src/password_command.rs`, REL-4, TST-1) | A password command that backgrounds a grandchild before returning does not leave it running past the command's own timeout — the same whole-process-group kill `hooks.rs`'s own version of this test proves for hooks |
| `a_password_command_is_used_instead_of_the_keyring` (`src/profile.rs`) | `Profile::password` prefers a set command over the keyring |
| `a_new_repository_verifies_data_after_compression_by_default` (`src/engine/tests.rs`) | rustic's own `extra_verify` default is on; Stellarshot's `init` does not turn it off. Confirms the setting, not a real corruption being caught — rustic exposes no public hook to induce one from outside |
| `nothing_set_leaves_the_defaults_alone`, `a_chosen_directory_and_no_cache_both_reach_the_options` (`src/engine/cache_settings.rs`) | The cache preference reaches `RepositoryOptions` correctly, and leaves rustic's own defaults alone when nothing has been set |
| `a_second_key_opens_the_same_repository_as_the_first`, `changing_the_password_replaces_the_key_it_was_opened_with`, `a_key_that_is_not_the_current_one_can_be_deleted`, `the_key_a_repository_was_opened_with_cannot_be_deleted` (`src/engine/tests.rs`) | A second key really does open the same repository; changing the password removes the old key rather than leaving it alongside the new one, so the old password stops working; a key that is not in use can be deleted, and afterward its password no longer opens the repository; the key currently in use cannot be deleted at all, and is still there and still works afterward. **Caught two real bugs, both against a real repository, not assumed correct from reading rustic_core's docs:** first, `Repo::keys()` read each key file with `cat_file`, the same call every other file type uses, but a key file is protected by its password, not the repository's master key — `cat_file` always decrypts with the master key, so every key past the first failed with a garbled decryption error. Fixed by listing key IDs directly instead of reading file content, which also means the metadata a key file carries (hostname, username, created) cannot currently be shown — noted in ROADMAP.md rather than worked around. Second, `Repo::change_password()` added the new key and then tried to delete the old one on the same open handle, which rustic itself always refuses: the key a repository was *opened* with is fixed for that handle's whole life, so adding a new key never changes what deleting-the-current-key means to it. Fixed by re-opening with the new password before deleting the old key |
| `an_append_only_repository_refuses_to_forget_a_snapshot` (`src/engine/tests.rs`) | Against a real repository created with append-only on: rustic itself refuses to delete a snapshot from it, and the snapshot is still there afterward. Proves the setting actually takes hold through `init_with`, not just that the flag is threaded through |
| `a_profile_round_trips_through_the_text_form` (`src/settings_export.rs`) | **Caught a real regression from this session's own `password_command` field:** the field's name alone, even empty, made "nothing password-shaped in a settings export" false, since RON writes struct field names into the text regardless of their value. Fixed by clearing the field in `Export::collect` and skipping it entirely when empty (`skip_serializing_if`), rather than weakening the check |
| `probe_finds_an_empty_location_then_the_repository_once_created`, `deleting_a_rest_repository_is_refused_rather_than_attempted`, `backup_and_restore_round_trip_through_a_rest_server` (`tests/rest_server.rs`) | Against a real local `rustic-server`, run by hand rather than in CI (see below): probing tells empty from repository correctly by way of `config_id`; deleting one from Stellarshot is refused, and the repository is still there afterwards; a full backup and restore round trips real file content. **Caught along the way:** `rustic-server`'s own `--private-repos`/`RUSTIC_SERVER_PRIVATE_REPOS` default to on and do not actually turn off from the command line or environment despite accepting the flag, confirmed with its own `-v` debug log showing the override applied and then silently dropped when the config layers merge — worked around with a repository-specific ACL section instead of `[default]`. **Also caught:** waiting for the server's own "Listening on" log line, read from a piped stderr, hung indefinitely rather than timing out, because an abscissa app (what `rustic-server` is built on) fully buffers stderr once it is a pipe instead of a terminal; replaced with polling a real HTTP request against the port, which does not depend on that buffering. **All three are now marked `#[ignore]`, not just the round trip**: all pass reliably by hand on this project's own development machine but fail in CI specifically, every time, with the same `Connect` error on a write shortly after the readiness probe reports the server ready. Two theories were each tried for real (not just reasoned about) and disproven by actually pushing and reading CI's own logs: a one-off timing race (disproven — a 3-attempt retry, each a whole fresh server and data directory, failed identically every single time, which a genuine race would not do); CPU contention between this file's tests running concurrently (disproven — serializing every one of them behind a process-wide `Mutex`, confirmed to actually take effect by the total run time matching serial rather than parallel execution, changed nothing in CI). `spawn_server` now captures the server's own stdout and stderr to a file instead of discarding them, so whoever investigates this next has something real to read instead of a third guess made without it |

### A full code review of the 0.3/0.4 work, before release

A review of everything changed since 0.2.0 found nine real issues, ranked
most to least severe below. All were fixed and covered by a test where the
finding was about behavior a test can exercise; the two that are purely
about wording (an inaccurate doc comment, a claim about a rustic behavior
that turned out to be wrong once checked against its actual source) needed
no test, only a correction.

| Finding | Fix | Proven by |
| --- | --- | --- |
| The logging fix earlier in this release only reached the window's own process; every real backup runs in a `--run` child or a `--scheduled` run, neither of which ever installed it, so the original bug (a real Google Drive failure with nothing in the log) was still not actually fixed | Both entry points now call `set_logger_for_child()`, which appends rather than truncates — several such processes, and the window, can share the one log file without racing to clobber each other's lines | `a_child_process_appends_rather_than_truncating`-equivalent coverage split across `truncate_false_appends_instead_of_overwriting` (`src/debug.rs`, the file mechanics) and the existing `rustic_backend_output_reaches_the_log_file` (`src/app/startup.rs`, the `log`-to-file bridge); the two are not tested together on purpose — `tracing`'s global subscriber can only be installed once per process, so a second test calling `init_tracing` in the same binary would pass or fail depending on test order rather than proving anything |
| A settings export left a REST destination's URL, credentials and all, in plain text, contradicting the export's own "never a password" promise | `Export::collect` redacts it with `redact_url`, the same function that already kept it out of messages and logs | `a_rest_destinations_credentials_are_stripped_on_export` (`src/settings_export.rs`) |
| `redact_url` failed open: a URL whose password contains an unescaped `/`, `?` or `#` fails to parse at all, and the old code returned the original string, credentials and all, in that case | Returns a fixed, non-leaking placeholder instead when parsing fails, rather than guessing where credentials end | `redact_url_shows_nothing_of_a_url_it_cannot_parse`, plus `redact_url_removes_a_parseable_urls_credentials` and `redact_url_leaves_a_credential_free_url_alone` for the normal cases, none of which existed before (`src/engine/repo.rs`) |
| A crafted or hand-edited settings export could carry a `password_command` through import even though a real export already clears it, and a bandwidth limit was hand-quoted into the rclone command string with a plain `'...'`, so a value containing a quote could end its own argument and start another — including `--password-command`, which rclone would then run | `merge` also clears `password_command` on import, not trusting the file; the rclone command is built with `shell_words::quote`, which cannot be broken out of | `a_password_command_from_an_untrusted_export_is_cleared_on_import` (`src/settings_export.rs`); `a_bandwidth_limit_cannot_inject_a_second_rclone_argument` (`src/engine/repo.rs`) |
| An imported backup could arrive already scheduled and already running its hooks, before anyone had reviewed what those hooks actually do, or with an rclone remote or profile ID crafted outside the shapes Stellarshot's own wizard ever produces (SEC-1, TST-1) | `merge` resets an imported profile's schedule to `Manual` and disables every hook (flagging the import for review rather than discarding them), rejects an rclone remote `Destination::location()` would not accept, and rejects an unsafe profile ID — along with any history for it, so nothing is merged under an ID that was itself refused | `an_imported_backups_schedule_and_hooks_start_off_and_flag_for_review`, `an_rclone_remote_outside_the_wizards_shape_is_rejected_on_import`, `an_unsafe_profile_id_is_rejected_and_its_history_is_not_merged` (`src/settings_export.rs`) |
| Changing a backup's password ran in-process, skipping the cross-process write lock every other write takes, and a failed keyring update afterward was silently dropped — a scheduled backup could then keep failing with no explanation | Routed through the same `--run` child as every other write, which takes the lock and the logging fix above; a keyring failure now surfaces as its own error rather than nothing | Covered by the existing `changing_the_password_replaces_the_key_it_was_opened_with` and the four other key-management tests (`src/engine/tests.rs`), which already exercise `change_password` end to end; the lock and keyring-failure paths themselves are UI/process wiring not practical to unit test, same as the rest of `child.rs`'s process-spawning |
| An append-only repository's own state was only recognized when Stellarshot created it — opening an existing one left the flag `false`, so Clean Up Now and pinning stayed offered and would then be refused by rustic itself | Read back from the repository itself (`is_append_only`) when opening, not assumed from the wizard's own toggle (which only exists at creation); Clean Up Now, pin and single-snapshot delete are now hidden for an append-only backup | `an_append_only_repository_refuses_to_forget_a_snapshot` (`src/engine/tests.rs`) proves the underlying setting; the UI gating itself is a `.then_some`/`.then` guard, the same pattern already used for `is_busy()` elsewhere, not separately unit tested |
| Pinning or deleting a single snapshot never marked the page busy, so a fast double-click could send a second request against a snapshot whose ID the first request had already rewritten, hitting an internal "no snapshot with that ID" | Both now go through the same single-flight `Work` tracking as a backup, check or clean-up | Not separately tested: the fix reuses `start`/`on_work`, already covered by the existing backup/check/clean-up tests exercising that same machinery |
| A password command's own failure (a locked vault, a wrong command) was silently swallowed into the same generic "password not remembered" outcome, indistinguishable from never having set one at all, and had no timeout, so a command stuck on a prompt nobody could see would hang a `--scheduled` run forever | `Profile::password` now returns the real error separately from "nothing configured"; the command itself runs under a timeout with `kill_on_drop` | `a_failing_password_command_is_reported_rather_than_treated_as_unremembered` (`src/profile.rs`); `a_command_that_never_finishes_times_out_rather_than_hanging_forever` (`src/password_command.rs`) |
| A missing or unreadable exclude-pattern file was silently ignored (`unwrap_or_default`), so a backup would quietly include whatever it was meant to leave out | Fails the backup with an `Io` error naming the file instead | `a_missing_pattern_file_fails_the_backup_rather_than_including_everything` (`src/engine/tests.rs`) |
| The REST-delete refusal was a hardcoded English sentence bypassing `fl!()`, and doc comments in three files plus two CHANGELOG/ROADMAP entries claimed append-only "cannot be turned off later" and rustic's `config` command "stops working entirely" once set — checked directly against rustic_core's own source (`commands/config.rs`), both are wrong: `config` refuses every change to an append-only repository except turning append-only itself back off, which needs nothing more than the repository's own password | Given its own `ErrorKind::DeleteUnsupported` and a translated message in all five locales; every inaccurate doc comment and changelog/roadmap line corrected to say what is actually true (Stellarshot exposes no way to do it, not that rustic cannot) | `deleting_a_rest_repository_is_refused_rather_than_attempted` (`tests/rest_server.rs`) updated to assert the new `ErrorKind` |

### An accessibility pass, before release

Not a full audit (see ROADMAP.md's 1.0 section for what is not covered), but
one real gap found and fixed, checked against the actual framework source
rather than assumed: every icon-only button (`src/app.rs`,
`src/app/pages/profile.rs`, `src/app/pages/restore.rs`,
`src/app/wizard/mod.rs`) had a visual `.tooltip()`, and some had neither
that nor anything else. Reading libcosmic's `widget/button/icon.rs` and
`widget/button/widget.rs` directly confirmed `.tooltip()` and `.name()` are
two separate fields: only `.name()` reaches the AccessKit node a screen
reader sees (`node.set_label`), and Iced's own `Tooltip` widget (checked in
its source too) has no accessibility implementation of its own — a tooltip
is genuinely mouse-only. All seven buttons now set `.name()` as well.

Attempted to confirm this against a real, running instance's AT-SPI tree
(the same demo setup `scripts/screenshots.sh` builds, driven by a small
Python probe instead of a screenshot) rather than trust the source reading
alone. The attempt did not get that far: the demo instance could not reach
the session's accessibility bus at all (`AT-SPI: Unable to open bus
connection`), a sandboxing issue with the environment a demo instance
needs rather than anything about Stellarshot's own code. Not yet retried;
noted honestly in ROADMAP.md rather than claimed as verified live.

### Browsing folders by size (`src/engine/disk_tree.rs`, `src/app/wizard/browse.rs`)

| Test | What it proves |
| --- | --- |
| `files_are_sized_by_their_own_length`, `a_folders_size_is_everything_under_it`, `largest_comes_first` (`src/engine/disk_tree.rs`) | A file's size is its own length; a folder's is the real sum of everything under it, against a real temporary tree; rows come back largest first |
| `a_symlink_counts_its_own_size_and_is_never_followed`, `a_symlink_cycle_does_not_hang` (`src/engine/disk_tree.rs`) | A symlink is sized as itself, not the target, and is never walked into — checked against a real symlink cycle (a folder linking back to its own ancestor), which a walk that did follow symlinks would spin on forever rather than return from |
| `canceling_stops_the_walk`, `an_unreadable_root_reports_an_error_not_an_empty_list` (`src/engine/disk_tree.rs`) | A set cancel flag stops a walk in progress rather than being ignored; a folder that cannot even be opened is a real error, not silently reported as holding nothing |
| `opening_lists_the_root`, `toggling_an_unlisted_folder_requests_its_children`, `toggling_an_expanded_folder_collapses_it_without_a_new_request`, `marking_bubbles_up_rather_than_being_applied_here` (`src/app/wizard/browse.rs`) | Opening the browser requests the root's own children; expanding an unlisted folder asks for its children exactly once, not again if it is already expanded or already loading; collapsing needs no new request; marking a folder excluded is not applied by this module itself, only handed up to the wizard that owns the actual exclude list |
| `mark_of_a_plain_folder_is_included`, `mark_of_a_listed_exclude_is_excluded`, `mark_of_an_ancestor_of_an_exclude_is_partial`, `mark_of_an_unrelated_folder_is_included` (`src/app/wizard/browse.rs`) | The three-way mark (Included / Excluded / Partly included) matches the wizard's own exclude list correctly in each case, including a folder that is not itself excluded but has an exclusion somewhere inside it |
| `excluded_bytes_sums_every_known_excluded_descendant`, `excluded_bytes_is_zero_for_an_exclude_never_sized_this_session` (`src/app/wizard/browse.rs`) | A folder's shown size is reduced by whatever excluded descendant has actually been sized (which includes every folder right under an already-open root, the common case), and stays unreduced — not guessed at — for one that never was |

Not covered by an automated test, and not yet tried against the real, rendered window either: the layout itself (indentation, the disclosure arrow, the scrolling region, the checkbox no longer sitting under the scrollbar) — the tests above exercise the state machine behind it, not what it looks like on screen. The include/exclude control changed from a status-label button to a checkbox specifically because the old one did not read as interactive — a real usability report, not something a unit test would have caught, and not something this environment's own UI testing limitations allow verifying by eye either.

### Compression level (`src/engine/repo.rs`, `src/profile.rs`, `src/app/wizard/mod.rs`)

| Test | What it proves |
| --- | --- |
| `a_chosen_compression_level_is_stored_in_the_repository` (`src/engine/tests.rs`) | A chosen level reaches the repository's own config, against a real repository — read back through a freshly reopened handle, not just the one that created it |
| `no_chosen_compression_leaves_rustics_own_default_in_place` (`src/engine/tests.rs`) | Leaving the default choice writes nothing to `ConfigOptions`, so a future rustic version's own default is not silently pinned to today's |
| `compression_is_chosen_at_creation_and_never_revisited` (`src/app/wizard/mod.rs`) | Choosing a level updates the wizard's own state; editing an existing profile's schedule neither shows nor overwrites its already-chosen level — it is loaded from nothing and saved only for a genuinely new profile |
| `old_profiles_without_new_fields_still_load` (`src/profile.rs`) | A profile saved before this field existed defaults to `Compression::Default`, not a parse failure |

Not built: the benchmark against this machine's own speed that the original idea for this included, only a curated three-way choice (Default, Fast, Best) in place of zstd's full -7 to 22 range.

### Mounting a snapshot as a folder through FUSE (`src/engine/mount.rs`, `src/app/pages/restore.rs`)

| Test | What it proves |
| --- | --- |
| `a_mounted_snapshot_can_be_read_with_plain_filesystem_calls` (`src/engine/tests.rs`) | Against a real FUSE mount in this sandbox (not a mock filesystem): a plain file, a nested folder, a symlink and a file with a non-default mode (`0600`) all read correctly through ordinary `std::fs` calls once mounted |
| `a_mounted_file_cannot_be_written_to` (`src/engine/tests.rs`) | Writing to a mounted file fails, since the mount is read-only both by mount option and because `SnapshotFs` implements no write operation at all |
| `a_new_inode_table_starts_with_only_the_root`, `the_same_path_always_gets_the_same_inode` (`src/engine/mount.rs`) | Inode numbering is stable for as long as a path has been seen, and the reserved root inode (1) is never reassigned |
| `a_folder_defaults_to_read_only_permissions_without_a_recorded_mode`, `a_files_recorded_mode_is_kept_masked_to_permission_bits`, `an_unrecorded_mode_defaults_to_read_only_for_a_file`, `attr_reports_the_mounting_user_as_owner` (`src/engine/mount.rs`) | Reported permissions and ownership match what a real read-only mount should show, including a recorded mode's file-type bits never leaking into `perm` |

Found only by running the real end-to-end test, not by reading the code: an early version turned on `MountOption::DefaultPermissions` and reported every entry as owned by root, which had the kernel enforce permission checks against that fake ownership — reading `private.txt` (backed up at `0600`) came back `EACCES`, since the kernel compared its own real, unprivileged uid against the reported root owner and refused. Fixed by dropping `DefaultPermissions` and reporting the real mounting user's uid/gid instead (`rustix::process::getuid()`/`getgid()`), since FUSE already restricts the mount to that one user; the same test then passed.

Not covered by an automated test: the "Mount as Folder…"/"Open Folder"/"Unmount" buttons themselves in `src/app/pages/restore.rs`, or the folder-choosing dialog they open — UI wiring, not new logic, following the same `Effect`/blocking-task pattern already proven for **Download** and **Open Copy**. Also not proven: behavior with `allow_other` or multi-user access — not offered, since only the user who authenticated with the repository's password can see the mount at all, which is the intended scope.

REL-15 replaced `open`'s whole-file read (a `Vec<u8>` holding the entire
file, for as long as it stayed open) with rustic's own `OpenFile`/
`read_at`: opening a file now reads nothing at all, and `read` fetches
only the blob range each call asks for. `a_mounted_snapshot_can_be_read_with_plain_filesystem_calls` already reads a real 64 KiB file
(`nested/deeper/data.bin`) through the mount and still passes, now
exercising this path for real rather than the removed one — the same
test also reads `private.txt`, `link-to-plain` and a non-UTF-8-named
file, none of which changed behavior. Considered adding
`MountOption::AutoUnmount` for a crashed process, per the review plan,
but `fuser`'s own `Session::new` refuses to enable it unless the mount
also allows root or other users — exactly the `allow_other`/multi-user
access this file's own note above says is deliberately not offered — so
it was left out rather than trading one gap for a real access-control
change nothing here decided. Every `Mutex::lock().unwrap()` in this file
also became poison-tolerant (a small `lock()` helper), so one FUSE call
panicking cannot take every later call on the same handle table down
with it; not independently tested (deliberately triggering a panic mid-
request to prove the recovery would need its own test-only seam this
project has no equivalent of elsewhere), but a small, mechanical,
easily-read change.

### A History page across every backup (`src/event_log.rs`, `src/app/pages/history.rs`, `src/app/pages/profile.rs`, `src/app.rs`)

| Test | What it proves |
| --- | --- |
| `merge_sorted_interleaves_every_profiles_events_newest_first` (`src/event_log.rs`) | Events from more than one profile's log come back merged into a single newest-first list, not grouped by profile or left in per-profile order |
| `an_event_logged_before_source_existed_is_read_back_as_desktop` (`src/event_log.rs`) | A log entry written before the `source` field existed (no such key in its stored form at all) deserializes as `Source::Desktop`, not a parse failure — the same backward-compatibility pattern already proven for profile fields |
| `every_new_event_kind_describes_itself_with_the_snapshots_short_id` (`src/event_log.rs`) | Every new `EventKind` (restore, snapshot deletion, pin/unpin, password change, mount/unmount) renders as a sentence that names the *short* snapshot ID, not the full hash |
| `a_finished_snapshot_deletion_logs_which_one`, `a_failed_snapshot_deletion_logs_nothing`, `a_finished_pin_change_logs_which_way_it_went` (`src/app/pages/profile.rs`) | Deleting or pinning a snapshot logs exactly what was asked for once the child process actually finishes — not merely that some write happened — and a failed deletion logs nothing rather than a wrong success entry |

Five kinds of action gained a log entry that had none before: restore, snapshot deletion, pin/unpin, password change, and mount/unmount — previously only backup, check, clean-up, skip and failure were recorded at all. Each `Event` also carries a `Source` (`Desktop`, or `Other` for another program recording into the same history, shown as an "Other program" badge), defaulted for every entry recorded before this field existed. `Other` also absorbs any source value this version does not know, so an unfamiliar one can never make a whole log unreadable: `an_entry_from_a_source_this_version_does_not_know_still_loads` and `a_source_written_by_a_later_version_loads_as_other_too` (`src/event_log.rs`) prove both, including an entry in the exact form an earlier version wrote.

Not covered by an automated test: the History page's own `view()` in `src/app/pages/history.rs` (a pure rendering function, no interactive `Message` of its own yet) or the sidebar entry and its data-loading `Task` in `src/app.rs` — UI wiring, following the same pattern already noted above for Download, Open Copy and the mount buttons. The 500-entry display cap (`history::LIMIT`) is exercised by nothing but its own arithmetic; not proven against an actual machine with that much history.

### The new-backup Browse hint (`src/app/wizard/mod.rs`)

A real report: exclusions and the size estimate live behind the Browse button next to a source folder, and nothing else on the "what to include" step says so — easy to never discover, since "Browse…" reads as generic file-picker boilerplate rather than a feature of its own.

Not covered by an automated test: the popover's own on-screen appearance (the same category of gap as every other new settings control in this file — UI wiring, not logic). What is covered, as pure `Wizard` state:
- Creating a new backup starts with the hint not yet dismissed; opening or editing an existing one starts with it already dismissed, since the hint is about a *brand new* backup's first folder specifically.
- Dismissing it directly, or browsing the first (and only, at that point) source folder, both mark it dismissed; browsing a *different* source does not, since the hint only ever points at the first one.


### Searching across every snapshot, and the Compare tab's folder grouping (`src/engine/browse.rs`, `src/app/pages/restore.rs`)

| Test | What it proves |
| --- | --- |
| `search_all_finds_every_snapshot_a_name_appears_in` (`src/engine/tests.rs`) | Against a real repository with three real backups: a name present in two of three snapshots comes back as *one* result naming both, not two separate rows, and a snapshot that never had it is correctly excluded |
| `search_all_matches_every_distinct_path` | Two different files whose names both match the query come back as two distinct results, not merged or one dropped |
| `search_all_ignores_case_and_an_empty_query_finds_nothing` | Case-insensitive matching, and an empty query is treated as "nothing to search for" rather than "match everything" |
| `common_ancestor_finds_the_longest_shared_prefix`, `common_ancestor_of_one_path_is_its_own_parent_chain`, `group_diff_puts_every_entry_directly_in_the_common_root_together`, `group_diff_separates_entries_in_different_subfolders` (`src/app/pages/restore.rs`) | The Compare tab's folder-grouping logic: changes sharing a folder land in one group, changes in different folders land in separate groups, and the group key is relative to the diff's own common root rather than the full absolute path |
| `a_folders_diff_expansion_toggles`, `comparing_again_clears_the_previous_expansion` | Expanding a folder's changes toggles cleanly, and starting a new comparison does not leave a stale folder expanded from the previous one |
| `jump_to_match_switches_to_browse_at_the_matched_snapshot_and_folder`, `jump_to_an_unknown_snapshot_does_nothing` | Clicking a search result's snapshot switches to Browse already pointed at the right snapshot and folder; a snapshot ID that somehow does not match anything currently loaded is a no-op, not a panic |
| `a_unique_prefix_finds_its_snapshot`, `an_ambiguous_prefix_is_reported_as_ambiguous_not_missing`, `a_prefix_matching_nothing_is_reported_as_not_found` (REL-17) | `Browser::snapshot`'s ID matching, factored into a free `index_of` function over plain ID strings so this is checked without a real repository: a unique prefix or a full ID resolves; a prefix two snapshots share now reports `ErrorKind::Ambiguous`, not the same "not in this snapshot" a genuinely missing ID gets — before this fix both cases returned the identical error |

One real bug was found and fixed by the real-repository test, not by reading the code: `find_matching_nodes` returns paths relative to the tree root, without the leading `/` every other path in `Browser` carries (`list`, `search`, `diff` all prepend it). The first version of `search_all` did not know this and returned paths like `tmp/.../keepme.txt` instead of `/tmp/.../keepme.txt` — `search_all_finds_every_snapshot_a_name_appears_in`'s exact-equality assertion caught it immediately; a looser `.ends_with(...)` check (used in the other two tests, for other reasons) would not have. Fixed by applying the same `Path::new("/").join(...)` fix-up `search` already does.

Not covered by an automated test: `search_view`/`global_match_row`'s own rendering in `src/app/pages/restore.rs` — UI wiring, following the same pattern as this project's other view functions. Not built: searching file *contents*, which rustic_core has no support for at all (see ROADMAP.md).

### Usability fixes from real use (`src/app.rs`, `src/run_state.rs`)

Three more changes from watching the app actually get used, none of them logic a unit test would catch:

- **Signing in to Google Drive now shows "switch to your browser" as a dialog**, not a line of page text below a button that had just disappeared — easy to miss since the user's attention is already on the button they just clicked. `src/app.rs`'s handling of `place::Effect::SignIn` now opens a `Dialog::Info` alongside starting the sign-in itself; it closes again once `place::Message::SignedIn` arrives, whichever way it went.
- **A shield (`security-high-symbolic`) replaces a plain hard drive icon** for a backup that is up to date, in `run_state.rs`'s `BackupStatus::icon`. Rendered with `librsvg` against this project's own development machine and looked at directly, not assumed correct from the freedesktop icon name alone — an earlier attempt with ImageMagick's own SVG renderer produced a blank image for the same file, which would have been reported as "looks fine" on nothing but a missing check.
- **Investigated but not changed**: a report that Next on the wizard's "where" step needs Check pressed first before it works. `one_press_of_next_checks_the_destination_and_moves_on` (`src/app/wizard/mod.rs`) already proves the opposite — Next checks an unchecked destination itself and advances once the check succeeds, wiring present since before this project's first release. No gap was found in the current source; most likely explanation is an older installed build.

### Downloading straight from a snapshot (`src/engine/browse.rs`)

| Test | What it proves |
| --- | --- |
| `dump_file_writes_exactly_that_files_bytes` | A file downloaded from a snapshot matches its backed-up content byte for byte, via rustic's own `dump` |
| `dump_file_refuses_a_folder`, `archive_folder_refuses_a_file` | Downloading a folder as a file, or a file as a folder, is refused before anything is written, rather than producing an empty or wrong file |
| `archive_folder_produces_a_tar_gz_with_the_same_tree` | A folder downloaded as a `.tar.gz`, extracted with a real `tar`/`gzip` reader, matches the original tree exactly — names, content and Unix permissions, proven against a tree with nested folders, unicode names, a `0600` file and a symlink |
| `archive_folder_leaves_nothing_behind_when_a_blob_read_fails_partway_through` (TST-5) | A failure well after writing has started — not merely an upfront rejection — still leaves nothing behind: neither the destination `.tar.gz` nor a stray temp file in its folder. A new `corrupt_every_file` helper overwrites every pack under a real repository's `data/` directory after a real backup (leaving `index/` and the config alone), so opening and listing still succeed but reading a file's actual content fails the way a damaged remote or bit-rotted disk would |

`Browser` moved from a size-optimized index (`IndexedIdsStatus`) to a fully-loaded one (`IndexedFullStatus`, via `to_indexed()` rather than `to_indexed_ids()`) so `dump` is available to call at all; every other browse operation (list, search, versions, diff, missing) keeps working unchanged, since `IndexedFullStatus` is a superset. Not covered by an automated test: the "Download…" buttons themselves in `src/app/pages/restore.rs`, or the save-file dialog they open — UI wiring, not new logic, following the same pattern already proven for **Open Copy** and **Restore This Version…**.

REL-15: `archive_folder` used to read each file fully into a `Vec` before
handing it to the tar writer — checked `dump_file` for the same problem
first rather than assuming the review plan's file list meant both were
equally affected, and found it already streamed straight into the
destination `File` with no buffering step at all. Only `archive_folder`
needed the streaming fix, done with a small `Read` adapter
(`BlobReader`) over rustic's `OpenFile`, so `tar::Builder::append_data`
now pulls bytes on demand the same way it would from a real file.
`archive_folder_produces_a_tar_gz_with_the_same_tree`'s byte-for-byte
comparison against the extracted archive still passes unchanged, now
exercising that streaming path instead of the removed buffer — a real
correctness check on the new code, though it does not itself measure
memory. Both `dump_file` and `archive_folder` now write to a temporary
file first (via the `atomicwrites` crate already used elsewhere in this
project) and move it into place only once the write actually finishes;
`archive_folder_refuses_a_file`'s existing `assert!(!destination.exists())`
still passes, though that specific assertion was already true before this
change too (the folder-vs-file check runs before any file is touched
either way) — the atomicity this adds instead covers a failure *partway
through* a real write, later proven directly by
`archive_folder_leaves_nothing_behind_when_a_blob_read_fails_partway_through`
(TST-5), corrupting the backend's own pack data to force exactly that.
A directory entry with no
recorded mode now defaults to `0o755`, not `0o644` — not independently
tested, since a real local backup's directories always have a real mode
already; a backend that omits it is the untested case this exists for.

### Conditions for a scheduled backup (`src/conditions.rs`, `src/app/wizard/mod.rs`)

| Test | What it proves |
| --- | --- |
| `no_conditions_are_always_met` | A profile with every condition off never blocks a run, and reads no system state to decide that |
| `ac_is_required_only_when_actually_on_battery`, `a_battery_minimum_blocks_only_below_it`, `metered_blocks_only_when_actually_metered` | Each condition blocks exactly the state it names and nothing else — the minimum itself clears it, one below it does not |
| `a_trusted_network_by_name_satisfies_the_condition`, `a_vpn_satisfies_the_trusted_network_condition_on_its_own` | The trusted-network condition is met by a listed Wi-Fi network or a VPN on its own, with neither required if the other is present |
| `an_unreadable_network_does_not_block_a_trusted_network_condition`, and the same pattern in the AC and metered tests | A piece of state Stellarshot could not read (the service is not running, or the machine has no battery) satisfies whichever condition needed it, rather than blocking a schedule forever on a machine that can never answer the check |
| `vpn_types_match_what_a_real_tailscale_connection_reports` | NetworkManager does not set its own `Vpn` flag for a Tailscale connection; it comes up as a plain `tun` connection instead — confirmed against a real `tailscale0` interface in this project's own development environment, not assumed from NetworkManager's documentation |
| `read_state_reaches_the_real_system_bus_without_panicking` | The zbus proxy definitions (interface, property names and types for UPower and NetworkManager) still match a real running system, and a value read back is sane; run once with its result printed against a real machine with a battery, NetworkManager, and an active Tailscale connection, confirming all five fields (on-battery, battery percentage, metered, connected Wi-Fi, VPN) came back correct before the print statement was removed |
| `a_new_backup_has_no_conditions`, `conditions_round_trip_through_editing`, `setting_conditions_updates_them_one_at_a_time`, `trusted_networks_can_be_added_and_removed` (`src/app/wizard/mod.rs`) | A new profile starts with every condition off; editing an existing profile's schedule loads its conditions and saves them back unchanged if untouched; each condition can be set independently; trusted networks can be added (blank and duplicate entries are rejected) and removed |

Reading the real state (`upower_state`, `network_state`, `read_state`) and deciding whether it satisfies a profile's conditions (`met`) are kept apart on purpose: the decision is exhaustively tested without a real system bus, and the reading layer is thin enough that one proof against a real machine is enough to trust the interface definitions, following the same split already used for scheduled-run notifications (`notify.rs`, also untested beyond a real interface check made by hand). Not covered by an automated test: the Conditions section's own layout in the wizard, or a scheduled run actually skipping for real — `scheduled::main`'s own gate is one `if let Err(reason) = ...` around the already-proven `conditions::check`, wired the same way an unreachable destination already was (`is_quiet` now also covers `ErrorKind::ConditionsNotMet`, proven in `only_unreachable_busy_or_condition_skipped_repositories_are_skipped_quietly`).

### The panel applet and minimizing to it (`src/app/applet.rs`, `src/status.rs`, `src/engine/lock.rs`, `src/app.rs`)

| Test | What it proves |
| --- | --- |
| `is_running_reflects_a_real_held_lock_without_taking_it_over` (`src/engine/lock.rs`) | `is_running` reports `true` exactly while a real lock is held, `false` once it is released, and probing it does not itself take or disturb the lock |
| `a_never_run_backup_is_not_overdue_and_not_failed`, `a_failure_after_the_last_success_shows_until_a_later_success`, `an_overdue_daily_backup_is_reported_as_such` (`src/status.rs`) | A backup's status is built correctly from its run history alone: never having run is not a failure; a failure shows until a later success clears it; an overdue schedule is reported as such |

Not covered by an automated test, and verified by hand instead, since both are real system integration rather than logic this project's own code decides:

- **The applet binary starts cleanly.** Run standalone (not registered with a running panel, to avoid changing a real session's panel configuration) in this project's own development environment, which has a real COSMIC session (`cosmic-comp`, `cosmic-panel`) and real UPower and NetworkManager services: it ran for 5 seconds with no panic and no error output, then was killed and left nothing behind. This exercises the same `cosmic::applet::run` startup path a panel would use to load it.
- **Single-instance activation works for real.** With the window running, launching `stellarshot` a second time exited in 16ms with no output, and the first instance was still running afterward with no crash — confirming `App::dbus_activation`'s "a window already exists" branch (`window::gain_focus` + `window::minimize(id, false)`) runs cleanly on a real system bus, not just that it compiles. Not verified the same way at the time: the "window was closed to the panel, reopen it" branch (`window::open`, then `core.set_main_window_id`), since reaching that state needs an interactive window close this environment cannot reliably script (see this file's own note on AT-SPI-based UI testing), and the applet's popup layout and icon-swapping, since neither can be seen without registering the applet in a real panel, which was deliberately not done here to avoid changing a real, currently-running session. **This gap was real**: that exact branch built its reopened window from a bare `iced::window::Settings::default()` (`decorations: true`, asking the compositor to draw its own title bar) instead of the client-side-only decoration `cosmic::app::Settings` gives the window the app starts with (`client_decorations: true` by default, which libcosmic turns into `decorations: false`) — a real user hit it as a double title bar after minimizing to the panel and reopening. Fixed by setting `decorations: false` on that window explicitly, confirmed by reading both `iced`'s and `libcosmic`'s own source for their respective defaults rather than assumed; still not proven by an automated or interactive test, for the same scripting-limitation reason as before.
- **`stellarshot-applet`'s release build strips developer logging too**, not just the window's: `scripts/verify-release-build.sh` now checks both binaries, and the applet's own build genuinely does reach a `debug_log!` call somewhere in the shared library (the check found the log path present without `release-build` and gone with it, the same as the window) — not a check that would have passed regardless of what the feature flag did.

### Hooks (`src/hooks.rs`, `src/runner.rs`, `tests/runner.rs`, `src/app/wizard/mod.rs`)

| Test | What it proves |
| --- | --- |
| `a_before_hook_that_fails_stops_the_rest`, `every_before_hook_running_cleanly_reports_ok` (`src/hooks.rs`) | `Before` hooks run in order and stop at the first failure, without running the ones after it |
| `after_hooks_run_for_the_matching_outcome_and_always`, `one_after_hook_failing_does_not_stop_the_rest` (`src/hooks.rs`) | An `After` hook runs only for its own outcome (`AfterSuccess`/`AfterFailure`) or `After` regardless; one failing does not stop the rest, since the backup itself has already finished |
| `a_disabled_or_differently_timed_hook_does_not_run`, `a_failing_hook_reports_its_stderr`, `unmatched_quoting_is_reported_rather_than_run_incorrectly` (`src/hooks.rs`) | A disabled hook, or one for a different timing, never runs; a failure's detail comes from the hook's own stderr; a malformed command line is reported rather than run incorrectly |
| `a_hook_that_never_finishes_is_killed_rather_than_hanging_forever` (`src/hooks.rs`) | A hook stuck past its timeout is killed and reported as a failure, not left to hang the fully synchronous call it runs inside |
| `a_before_hook_runs_before_the_backup_and_can_be_seen_by_it`, `a_failing_before_hook_stops_the_backup_from_running_at_all`, `a_disabled_before_hook_does_not_run`, `an_after_success_hook_runs_once_the_backup_has_actually_finished`, `an_after_success_hook_does_not_run_after_a_failed_backup` (`tests/runner.rs`) | The wiring into a real backup, not just the pure hook-running logic: a `Before` hook's marker file exists once the real `stellarshot --run backup` child reports done; a failing one leaves no snapshot at all; a disabled one never runs; an `AfterSuccess` hook's marker only appears once the backup has genuinely finished, and never appears after a backup that failed (a wrong password) |
| `an_after_hook_still_runs_when_the_repository_never_opens` (`tests/runner.rs`, REL-2, TST-1) | A `Before` hook succeeding guarantees its matching `After` runs even when the backup itself never gets the chance to (a wrong password, an unreachable destination) — the documented use is stopping a database before and starting it again after, and a failure to open must not leave it stopped |
| `a_timeout_kills_the_whole_group_not_just_the_direct_child` (`src/hooks.rs`, REL-4, TST-1) | A hook that backgrounds a grandchild process is killed as a whole process group on timeout, not just its own direct child, which would otherwise leave the grandchild running past the timeout meant to bound it |
| `a_new_backup_has_no_hooks`, `a_hook_needs_both_a_name_and_a_command`, `a_hooks_timing_can_be_chosen_before_adding_it`, `a_hook_can_be_toggled_and_removed`, `hooks_round_trip_through_editing` (`src/app/wizard/mod.rs`) | A new profile starts with no hooks; adding one needs both a name and a command; its timing can be chosen before adding it; a hook can be toggled off or removed; editing an existing profile's hooks loads and saves them back correctly |

A hook's command line is split with `shell_words`, the same as `password_command`'s own — never through a real shell — and killed rather than left to hang if it runs longer than `HOOK_TIMEOUT`, reading its stderr on a separate thread so a chattier hook cannot deadlock a poll loop that never drains its pipe. Not covered by an automated test: the Hooks page's own layout, since only the wizard step's logic (adding, removing, toggling, saving) is proven, the same as every other wizard step in this project.

### Starting a backup when its drive is connected (`src/schedule.rs`, `src/profile.rs`, `src/app/wizard/mod.rs`)

| Test | What it proves |
| --- | --- |
| `a_path_unit_watches_the_drives_uuid` (`src/schedule.rs`) | A `.path` unit's `PathExists=` line names the destination drive's own `/dev/disk/by-uuid/` entry, and no `OnCalendar=` line makes sense for it, so `timer_text` refuses one |
| `only_plain_ids_go_into_units`, `only_plain_uuids_go_into_path_units` (`src/schedule.rs`) | A malformed profile ID or drive UUID (a path, a space, a shell metacharacter, one that is too long) never reaches a unit file, matching the existing profile-ID check `service_text`/`timer_text` already had |
| `on_connect_has_no_fixed_period` (`src/profile.rs`) | Unlike an hourly, daily or weekly schedule, a drive-connected one is never reported overdue — there is nothing on a clock to be late against |
| `on_connect_is_only_offered_for_a_removable_destination`, `on_connect_round_trips_through_editing_a_removable_backup`, `leaving_a_removable_destination_falls_back_off_on_connect` (`src/app/wizard/mod.rs`) | The 4th frequency choice only selects anything for a removable-drive destination; choosing it and saving round-trips correctly; stepping back and changing the destination away from a removable drive falls the schedule back to hourly rather than leaving it pointed at a condition that can no longer be met |

`schedule.rs`'s `apply`/`remove`, which actually run `systemctl` and write real unit files, are not unit-tested here, the same as they were not before this feature: only the pure unit-file-text functions are, matching this project's own established pattern for that file. Switching a profile between a timer and a path unit (or back) is handled by `apply` removing whichever kind is no longer wanted before installing the one that is — proven by reading the code, not by an automated test, since doing so for real needs a real systemd user session. Not covered by an automated test: a real drive actually being plugged in and the unit firing, `scheduled::main` needed no changes at all for this feature since a path-triggered run reaches it exactly the same way a timer-triggered one does.

### Three bugs a live session surfaced, fixed the same day

- **"Delete backup and all data" refused a backup whose data was already removed by hand**, treating "nothing here" the same as "this is not a repository" (a folder full of someone else's unrelated files) and refusing both identically. Fixed in `src/engine/location.rs` (local) and `src/engine/rclone.rs` (rclone) by classifying the three cases (empty, a real repository, something else entirely) separately rather than collapsing the first two together, and treating "empty" as a no-op success. `delete_succeeds_as_a_no_op_when_nothing_is_there_at_all` (both files) proves it against a real, genuinely-missing location — a second delete of an already-deleted repository is checked the same way, since that is exactly the shape of a user calling delete twice.
- **A crash mid-write could leave a systemd unit file empty**, breaking a backup's schedule silently until it was saved again — found from a real hard freeze during this session, not reasoned about in the abstract. `src/schedule.rs`'s `write_if_changed` used a plain `fs::write`, with neither an atomic rename nor an fsync of the file or its directory. Fixed with `atomicwrites` (the same crate `cosmic-config` already uses internally for the profile list, confirmed by reading its own source rather than assumed). `a_written_unit_never_ends_up_empty_or_half_written` (`src/schedule.rs`) proves the write, the no-op-when-unchanged case, and the overwrite case together; `write_file` (`src/app/tasks.rs`, used for settings exports) gets the identical fix with no dedicated test of its own beyond the existing export round-trip tests, since the underlying mechanism is exactly the same one already proven in `schedule.rs`.
- **A typed destination path (SSH server, Google Drive folder, custom rclone remote, REST URL) never checked itself**, unlike a folder picker or a drive selection, which check themselves the moment something is chosen — so a folder that already held a backup, or leftover files from an interrupted one, stayed unexplained until Check or Next was found and pressed. Fixed by adding `on_submit` (Enter) to every one of those fields in `src/app/wizard/place.rs`, the same trigger the wizard's own Name field already used to advance. No dedicated test: it is one line per field wiring an existing, already-tested message (`Message::Check`) to an existing, already-tested widget event, not new logic of its own.

### Scheduling (`src/schedule.rs`, `src/scheduled.rs`, `src/run_state.rs`, `tests/scheduled.rs`)

| Test | What it proves |
| --- | --- |
| `timer_units_catch_up_missed_runs` | Each frequency's `OnCalendar`, and `Persistent=true` so a missed slot runs at the next login |
| `the_service_runs_the_scheduled_backup_gently` | The service runs `--scheduled <id>` as a oneshot at `Nice=10` with idle I/O |
| `exec_paths_are_escaped_for_systemd` / `only_plain_ids_go_into_units` | A path with spaces, `%`, `$` and quotes is escaped; a path with a newline, or an ID that is not letters, digits and dashes, produces no unit at all |
| `a_check_is_due_every_thirty_days`, `prune_waits_for_a_clean_check`, `keep_forever_forgets_and_prunes_nothing` | What runs after a scheduled backup |
| `only_unreachable_busy_or_condition_skipped_repositories_are_skipped_quietly` | Everything else is reported |
| `a_failure_shows_until_a_later_success` | The page's warning goes once a backup succeeds, from the timer or the window |
| `a_scheduled_backup_runs_checks_and_is_recorded` | The real binary, settings from cosmic-config and the password from a real keyring: one snapshot, a check, and the run recorded with no failure |
| `an_unplugged_destination_is_skipped_quietly` | Exit status 0 and no failure recorded when the repository's drive is not there |
| `runner_maintains_by_forgetting_then_pruning` (`tests/runner.rs`) | `--run maintain` forgets and prunes, reporting both |

REL-12: a quiet failure (unreachable, locked, conditions not met) used to
be quiet only during the backup stage — the same kind of failure during
forget, check or prune was reported as a real failure with a notification,
even though the next scheduled slot retries it exactly the same way. The
quiet check (`is_quiet`) is unchanged and its own unit tests still pass;
what changed is that `main` no longer gates it on `stage == Stage::Backup`.
`an_unplugged_destination_is_skipped_quietly` and
`a_scheduled_backup_runs_checks_and_is_recorded` both still pass, confirming
the backup-stage behavior they already covered is undisturbed. Not yet
covered by a test: the cleanup/check-stage half specifically, which needs a
way to force a lock conflict to appear *between* two stages of one run —
a test seam that does not exist yet (see REL-12's own status in the review
plan for the larger `run_plan` consolidation this same seam would also
serve).

The wizard's tests hold that a new backup runs daily and keeps a smart
history, that editing the schedule changes nothing else about the backup
(and editing the folders keeps the schedule), and that a Déjà Dup import keeps
its folders, schedule and "Keep" period.

### The `--run` child process (`tests/runner.rs`, `tests/child.rs`)

These run the real `stellarshot` binary, the way the window does.

| Test | What it proves |
| --- | --- |
| `runner_backs_up_and_reports_done` | A job on stdin produces progress events, a final `done` with a report, exit status 0, and one snapshot |
| `runner_reports_wrong_password` | Exit status 1, an error event of kind `WrongPassword`, and no snapshot |
| `runner_rejects_an_unknown_operation` | Exit status 2 for an operation that does not exist |
| `second_writer_reports_locked` | With the lock held, a backup reports `Locked` and **writes nothing** |
| `killed_backup_leaves_a_sound_repository` | 48 MiB of incompressible data; the child is killed with SIGKILL as soon as data is being stored. Afterwards there is no snapshot, `check` passes, and the next backup succeeds. **Cannot pass vacuously:** it fails if the backup finishes before it can be killed |
| `a_backup_does_not_hang_on_a_full_stderr_pipe` (`tests/child.rs`, REL-3, TST-1) | A real `stellarshot --run backup` child with 2000 unreadable subfolders (each one a line of diagnostic output) finishes inside a 60-second timeout with a real `Done` event, rather than deadlocking on a stderr pipe nobody was draining — its output is now read continuously in the background, not only after the child exits |
| `backup_finishes_after_the_window_goes_away` | Closing the reading end of the pipe mid-backup does not stop it; the snapshot is recorded |
| `runner_restores_a_selection_keeping_both` | A selective restore through the child process, as the Restore button runs it: `done` carries the counts, the existing file is untouched, and exactly one dated copy appears |
| `a_backup_streams_started_progress_and_done` | The window's stream yields `Started`, progress, then `Done` |
| `canceling_a_backup_ends_it_as_canceled_without_a_snapshot` | Cancel from the window's handle ends the stream as `Canceled`, with no snapshot left behind |
| `a_missing_executable_is_reported_not_hung` | If the child cannot start, the stream ends with an error instead of waiting forever — and that error is a plain `Io`, not `AppUpdated`, since a test-injected path is not `crate::exe::running_image()` itself |
| `canceling_a_backup_through_rclone_ends_it_and_stops_rclone` (`tests/rclone.rs`) | Cancel during a backup through rclone ends the stream as `Canceled` within 30 seconds, and no `rclone serve` for the repository is left running. **Proven able to fail:** before the child ran in its own process group, the stream never ended (the orphaned rclone held its output open) and the test timed out |

REL-11: `run` used to spawn a raw `current_exe()`, detecting a package upgrade by checking whether the path's own string ended in the kernel's `(deleted)` marker (through a lossy conversion) and refusing with `AppUpdated` if so. It now spawns `crate::exe::running_image()` (`/proc/self/exe`) instead, which keeps resolving to the same executable inode even after the path it was launched from is unlinked — there is nothing left to detect ahead of time, since the spawn just keeps working. Confirmed directly, not assumed: copying a real binary (`/bin/sleep`) to a temp file, running it, deleting the file, and successfully executing a fresh command through its `/proc/<pid>/exe` link anyway, proves the exact mechanism this depends on — `a_running_processs_exe_link_stays_executable_after_its_file_is_deleted` (`src/exe.rs`) runs that same proof on every `cargo test`. What remains reachable of `AppUpdated` — `running_image()` itself somehow failing to spawn, rather than the general case — is `app::child`'s own `spawn_error`, proven directly with an injected `io::Error` against both `running_image()` (→ `AppUpdated`) and an unrelated path (→ plain `Io`, the same case `a_missing_executable_is_reported_not_hung` above exercises end to end) in `src/app/child.rs`'s own unit tests, without needing a real subprocess for the discrimination logic itself.

### Backup profiles (`src/profile.rs`)

| Test | What it proves |
| --- | --- |
| `repository_inside_a_source_is_excluded` | Backing up `~` to `~/Backups/home` leaves the repository folder out, so a backup never copies itself |
| `a_similar_prefix_is_not_inside` | `/home/dave2` is not treated as inside `/home/dave`: paths are compared by component, not by string |
| `v1_repositories_become_profiles` | Each version 1 repository becomes a profile with its path and name and a distinct ID |
| `old_profiles_without_new_fields_still_load` | A profile saved before a field existed still loads, with the documented default |

### The size estimate (`src/engine/estimate.rs`)

| Test | What it proves |
| --- | --- |
| `estimate_matches_the_backup` | With an exclusion nested inside an include, an overlapping include, a pattern and a hard link, the estimate **equals** the byte total the backup then reports; per-folder totals are exact too. This is the regression Déjà Dup has |
| `exclude_through_a_symlinked_path_still_applies` (`src/engine/tests.rs`) | An exclusion written through a symlinked path (`/home` → `/var/home`) still leaves the folder out |
| `estimate_respects_cancel` | A canceled estimate stops and reports nothing, so a stale total never replaces a newer one |
| `the_arithmetic_adds_up_to_the_estimate` | "Included − excluded = total" holds exactly; each excluded folder is sized, nested ones count once toward the total, and what only a pattern removes is reported apart |
| `the_everything_baseline_ignores_every_kind_of_exclusion_not_just_two` | REL-13: the "everything" baseline used to clear only `excludes`/`exclude_patterns`, leaving `exclude_larger_than`, `exclude_caches`, pattern files and `git_ignore` active and undercounting what "everything" means. A fresh fixture with a real `CACHEDIR.TAG` file proves both `exclude_larger_than` and `exclude_caches` are now ignored for the baseline — writing it caught its own 43-byte arithmetic mistake first (the marker file's own signature line, forgotten from the expected total), a reminder that a check that passes on the first try without ever failing for the wrong reason has usually not been looked at hard enough |

### Storage through rclone (`tests/rclone.rs`, `src/engine/rclone.rs`)

These run the real rclone transport with rclone's own `:local:` backend: the
same path SSH servers and cloud storage take (rustic starting `rclone serve
restic`, the probe, the delete) without needing a server or an account. They
use a private, empty rclone configuration, so the user's own is never read.
**Cannot pass vacuously:** each asserts rclone is installed first and fails
if it is not; CI installs it.

| Test | What it proves |
| --- | --- |
| `backup_through_rclone_round_trips` | Create, back up and restore through rclone; the restored file matches, and no rclone is left running once the repositories are closed |
| `rclone_probe_classifies_like_a_folder` | A missing folder, a folder of other files and a repository are told apart exactly as for a local folder |
| `rclone_delete_leaves_foreign_files` | Deleting through rclone removes the repository's entries and leaves a neighboring `report.odt` byte-for-byte intact |
| `rclone_delete_refuses_a_folder_that_is_not_a_repository` | Nothing is deleted from a folder that is not a repository |
| `an_unreachable_remote_is_unavailable` | An undefined remote is `DestinationUnavailable`, not a crash or "not a repository" |
| `sftp_remotes_check_host_keys` | Every SFTP remote carries `known_hosts_file`, so host keys are always checked |

### Removable drives (`src/drives.rs`)

Pure parsing, tested with `/proc/self/mountinfo` and `/dev/disk/by-*` fixtures:
only removable mounts with a UUID count as drives (not the root filesystem or a
network share); a drive mounted at a new place is found there
(`uuid_resolves_to_its_current_mount_point`); an unplugged drive is simply
absent (`missing_drive_is_unavailable`), and a profile pointing at it reports
`DestinationUnavailable` with the drive's name
(`an_unplugged_drive_is_unavailable_by_name`); escaped names such as
`Photo\040Disk` are decoded.

### Déjà Dup import (`src/dejadup.rs`)

Fixtures modeled on a real Déjà Dup 50 Flatpak keyfile and on `dconf dump`
output. They prove that missing keys take Déjà Dup's schema defaults
(`$HOME` included, Trash and Downloads excluded); that `$DOWNLOAD` and the
other tokens follow `user-dirs.dirs` rather than English folder names; that a
relative local folder is under the home folder; that `sftp://` URIs become
servers; and that duplicity, borg and unsupported backends are refused. In the
wizard, `a_dejadup_import_keeps_its_folders_but_asks_for_the_password` proves
the recorded drive is selected by UUID, the exclusions carry over, and the
password field starts empty.

### The "where" step (`src/app/wizard/place.rs`)

A drive is remembered by UUID; a check only counts for the destination it
checked, so editing a field after a check needs a new one; a server needs a
host, a folder and a valid port; Google needs a sign-in first and only one
sign-in runs at a time; one of the user's remotes is only ever used through
Stellarshot's own copy of it; a destination edited while it is being checked
is not left "checking", and can be checked again
(`editing_during_a_check_does_not_leave_it_checking`).

### The keyring (`tests/keyring.rs`, `src/keyring.rs`)

`keyring_round_trip` stores, reads, replaces and forgets a password in a real
Secret Service, under a random profile ID so it can never touch a real saved
password. **Cannot pass vacuously:** without a Secret Service it fails (after
the keyring timeout) with "a Secret Service must be running and unlocked for
this test"; this was run and observed. CI runs it against an unlocked
gnome-keyring in a private D-Bus session.

REL-16 rewrote `store`/`load`/`forget` as thin wrappers over one shared
`store_item`/`load_item`/`forget_item` each (the plan's own
"near-duplicates" complaint), and made `load` log every failure path instead of treating a
keyring that could not be reached the same as one that was simply empty.
The round trip in `tests/keyring.rs` still passes
against this machine's real, unlocked Secret Service after that refactor —
the only thing worth re-proving here, since the logging additions cannot
themselves be exercised without a way to make a real Secret Service fail on
demand, which this machine's own keyring is not a safe thing to force.
`cargo clippy` caught a real compile break mid-refactor: `oo7`'s
`AsAttributes` trait is implemented for the unsized slice type `[(K, V)]`,
so a generic helper written to take `&[(&str, &str)]` does not satisfy
`oo7::Keyring::create_item`/`search_items`/`delete`'s own implicit `Sized`
bound on their `impl AsAttributes` parameter — fixed by typing the helpers
to the fixed-size `&[(&str, &str); 2]` `attributes()` actually returns.

"Remember password" silently doing nothing when the keyring refuses it
(the plan's main complaint) is now reported through a new
`DialogMessage::PasswordNotRemembered`, wired as an independent `Task`
batched alongside opening or finishing the wizard rather than through
`Finished`/`Opened`'s own result — `app::tasks::tests::finishing_*`
(`src/app/tasks.rs`) cover `finish`'s new two-argument signature (no
longer takes `remember` at all, since it no longer stores anything itself)
and still pass. Not verified live: the dialog's own on-screen appearance,
which would need stopping this machine's real `gnome-keyring-daemon` to
trigger — not done, consistent with this project's practice of not
disrupting Dave's own running session (see UI-1).

### The window (`src/app/wizard.rs`, `src/app/pages/profile.rs`, `src/app.rs`, `src/app/tasks.rs`)

State and effects are tested without rendering:

- **Wizard:** a backup cannot leave "what" without a folder; one press of
  "next" checks an unchecked destination and moves on when the check passes,
  without a second press (`one_press_of_next_checks_the_destination_and_moves_on`);
  a destination holding other files, or edited while it was checked, does not
  move on; an unreachable destination or a timed-out check stays put and
  "next" checks again; "open" needs an existing repository; passwords must
  match; the suggested name follows a host name as it is typed until a name
  is typed; editing keeps the profile ID and needs no password; stale probe
  and estimate results are ignored.
- **Profile page:** the keyring is consulted once; a remembered password opens
  without storing it again; a typed password is cleared from the field once
  used; a wrong password stays locked and is reported; Back Up Now needs a
  password and folders, and runs one backup at a time; a finished backup
  records its time and reloads; deleting a snapshot waits for a running backup.
- **Dialogs:** `delete_requires_the_exact_name` refuses an empty, differently
  cased, trailing-space or partial name, and refuses a second confirmation
  while the first delete runs.
- **Finishing the wizard, against real repositories:** create makes the
  repository; open takes its folders from the latest snapshot; open with the
  wrong password fails as `WrongPassword`.
- **UI-2, launching again while already running:** `a_launch_survives_the_round_trip_to_wire_strings_and_back`
  proves `Launch::from_flags` and its own `to_string()`/`from_wire` survive
  the exact shape a real D-Bus `activate_action` call carries (plain
  strings, not the typed enum) for every flag combination; a second test
  proves the precedence order (`--new-backup` over `--restore` over
  `--profile <id>`, matching `main.rs`'s own `if`/`else if` chain) and a
  third that an unrecognized action, or `profile` with no ID, is not
  treated as any launch at all rather than panicking or picking one
  anyway. Not run live: exercising the real second-process-over-D-Bus
  path needs an actual desktop session with a running compositor, which
  this sandbox does not have — the same category of gap as UI-1's own
  disclosed click-through.

### rustic's and rclone's diagnostics reach a log (`src/app/startup.rs`, `src/debug.rs`)

`rustic_backend_output_reaches_the_log_file` sets up the real logger against a
private path, logs through the `log` crate under `rustic_backend`'s own
target the way rclone's own output does, and reads that path back. **Proven
able to fail:** before the fix, `set_logger` built a `tracing` subscriber but
never called `tracing_log::LogTracer::init`, so nothing from `log` (all of
rustic_core, rustic_backend and rclone) ever reached it; this test would have
found an empty file.

The developer debug log is at `$XDG_STATE_HOME/stellarshot/developer-debug.log`
(off by default, and stripped from release builds entirely), and an existing
one left readable by others is tightened to `0600` when opened. The backend
log moved to `$XDG_STATE_HOME/stellarshot/backend.log` (or
`~/.local/state/stellarshot/backend.log`), since it is always on: two real
users of a shared machine sharing one fixed `/tmp` path would otherwise
collide, each inheriting whatever permissions the other happened to leave the
file with. `debug::open_private_log_file` — the one place either log is
opened — refuses to follow a symlink already at its path, creates the file
mode `0600`, and (new) refuses a pre-existing regular file this user does not
own via `fstat`, since `O_CREAT` without `O_EXCL` opens rather than fails on
one. `a_symlink_already_at_the_path_is_not_followed` plants a symlink to a
throwaway file first and checks both that opening it is refused and that the
symlink's target is untouched; `the_log_file_is_private` checks the mode bits
of a freshly opened one; `a_pre_existing_file_owned_by_someone_else_is_refused`
proves the *matching*-uid case still opens (the mismatched case needs a second
real uid, so it is not exercised directly — see SEC-5 in the review plan).
The backend log's own state directory is created and verified private by
`engine::lock::create_private_dir`, shared with the write-lock directory: see
"The lock and log directories are verified private" below for its own tests.

ARC-7: the *developer* debug log (`debug.rs`'s own `SINK`, not the rustic
backend log above, which already had this fixed) truncated on every
process's own first write, unconditionally — a `--run` child spawned
while the window was already open and logging would truncate the file
the window still held open at its old byte offset, leaving NUL-filled
holes once the window wrote again. Fixed with `debug::init(Role)`: only
`Role::Window` truncates; the applet, a `--run`
child and a `--scheduled` run all append, and a process that somehow
logs before calling `init` also appends rather than guessing it is the
window. Each line now also carries `[role pid]`, so two processes'
interleaved lines can be told apart. `only_the_window_truncates` and
`a_formatted_line_names_the_role_and_pid_before_the_category` are new,
testing the pulled-out pure `Role::truncates`/`format_line` functions
directly rather than the real global `SINK` (which a unit test cannot
touch safely — it is shared process-wide, including across tests in the
same binary). Beyond the unit tests: the real `--run`/`--scheduled`
integration suites (`tests/child.rs`, `tests/runner.rs`,
`tests/scheduled.rs` — 23 tests spawning the actual compiled binary)
still pass with `debug::init` now called at the top of each of those
entry points, and the panel applet's own `Applet::init` now calls it
too — it had no debug-log initialization at all before this, despite
being exactly as long-running as the window and so exactly as exposed
to the race being fixed.

### The lock and log directories are verified private (`src/engine/lock.rs`)

`runtime_dir` (the write lock, and the progress files a window reads a
scheduled backup's status from) and `rustic_log_path` (above) no longer fall
back to a shared, world-writable location like `/tmp` when `$XDG_RUNTIME_DIR`
or `$XDG_STATE_HOME` is unset — `$HOME/.cache` or `$HOME/.local/state`
instead. Either way, `create_private_dir` verifies the directory it is about
to use, not just the one it creates: `DirBuilder::mode` only applies when the
call actually creates the directory, so a *pre-existing* one — plausible on a
shared machine, or if something else already created the fallback path —
needed its own check. **Proven able to fail:** before this, a pre-existing
world-writable or symlinked directory at the fallback location was accepted
without complaint, and every lock and progress file after it inherited
whatever an attacker who created that directory first could arrange (holding
`flock` on a predictable lock file name to make a real backup look
permanently `Locked`, or a symlink a progress-file write would follow).

| Test | What it proves |
| --- | --- |
| `a_world_writable_pre_existing_directory_is_refused` | A directory already there, mode `0777`, is rejected rather than used as-is |
| `a_pre_existing_symlink_is_refused_rather_than_followed` | A symlink at the target path is rejected, not followed to whatever it points at |
| `a_private_pre_existing_directory_is_still_accepted` | The ordinary case — a directory this user already made and locked down — keeps working; the two checks above are not so strict they also reject the correct state |

Even with the directory itself verified private, the `.progress.tmp` file `runner.rs`'s `Output::emit` writes on every progress update was still opened with plain `std::fs::write`, which follows a symlink already at that path rather than refusing it — the one item SEC-5 originally left open. `write_progress_temp` now opens it with `rustix::fs::open`'s `O_NOFOLLOW | O_CREAT | O_WRONLY | O_TRUNC | O_CLOEXEC`, the same primitive `debug::open_private_log_file` already used. `write_progress_temp_refuses_a_symlink_and_does_not_write_through_it` plants a real symlink at the `.progress.tmp` path pointing at a second file, asserts the write returns `Err`, and asserts the second file's own content is untouched — proving the refusal, not just that the right flags compile; `write_progress_temp_writes_an_ordinary_file` is the non-regression case. The full `tests/runner.rs` suite (16 tests, real `--run` child processes) still passes, so a genuine backup's progress file keeps updating.

### Language selection (`src/core/localization.rs`)

`a_requested_language_is_actually_used` selects German and reads back
"Löschen", then English and reads "Delete". Upstream never selected a language
at all, so this would have failed there.

### Settings migration (`src/app/migrate.rs`)

| Test | What it proves |
| --- | --- |
| `migrates_old_config_once` | Old settings are copied, and a second launch does not copy them again over a later change |
| `v1_repositories_become_profiles_once` | Version 1 repositories become profiles, and once version 2 has a profile list (even an empty one) they are never imported again |
| `does_not_overwrite_existing_new_config` | Settings that already exist under the new ID always win |
| `no_old_config_is_a_no_op` | Nothing is created when there is nothing to migrate |

### Locale parity (`tests/i18n.rs`)

Fluent falls back silently: a key missing from one locale shows as English at
runtime, and a mangled `{ $placeholder }` misbehaves only in that language.

- Every locale has exactly the English key set: nothing missing, nothing
  orphaned.
- Every message has the same placeholders in every locale.
- No locale repeats a key.
- Every message that interpolates `$count` or `$files` (this project's own
  convention for a pluralizable quantity) has a Fluent plural selector, in
  every locale — added for I18N-2, and immediately useful for more than
  its own motivating examples: writing it before fixing anything found
  `browse-scanning`, `compare-folder` and `compare-folder-root` with the
  identical bug the plan's named examples did not mention, in all 5
  locales, not just one.
- **Cannot pass vacuously:** it fails if the English file is missing or has
  suspiciously few messages, rather than comparing nothing.
- `no_locale_has_a_fluent_syntax_error` (I18N-1, TST-1) parses every locale
  with the real Fluent parser, not the hand-written one above: a genuine
  syntax error (an unindented continuation line, the original report — the
  "this word is only shown once" warning silently lost part of its own
  value) is dropped as a "junk" entry rather than a build failure, so the
  key loses that content in every language with nothing else here able to
  see it.

### Release builds carry no debug logging (`scripts/verify-release-build.sh`)

Builds the binary twice with the developer switch forced on: once without and
once with `--features release-build`. It then checks whether the debug log path
survives into each binary.

- **Cannot pass vacuously:** it fails if the path is missing from the build
  *without* the feature. In that state it could not detect a regression.
- It restores `src/debug.rs` on exit, even on failure.
- The M0 release was also checked on the packaged binary: the `.deb`'s
  `/usr/bin/stellarshot` does not contain the path.

### Packaging metadata (`scripts/validate-metadata.sh`)

- `desktop-file-validate` on the desktop entry, where any finding except a hint
  fails.
- `appstreamcli validate` on the metainfo.
- Checks `appstreamcli` does not make:
  - at least one screenshot, every screenshot captioned, exactly one default;
  - the newest `<release>` equals the version in `Cargo.toml`;
  - the component ID, launchable, desktop `Icon=` and installed icon files all
    agree on the application ID;
  - developer, content rating, source link and branding are present.
- **Cannot pass vacuously:** a missing metainfo, desktop entry or `Cargo.toml`
  is a failure, not a skip.

One pedantic finding is accepted: the application ID contains uppercase
letters (`Stellarshot`), which AppStream discourages. That follows the COSMIC
convention (`com.system76.CosmicFiles`).

I18N-3 added `stellarshot-applet` to `<provides>`
(both binaries are installed by the same package, per `install.sh`) and
a `<supports>` block for keyboard/pointing input. Ran
`scripts/validate-metadata.sh` itself, not just `appstreamcli` on its
own, so the change is checked against every rule this project's own
script enforces (screenshot captions, the release-version match, the ID
agreement across component/launchable/desktop-file/icon, and the rest),
not only the generic validator's. Still one pedantic finding, the same
pre-existing one above — nothing new introduced. Not done: the
localized `Name[xx]`/`Comment[xx]` half of I18N-3, which needs a
build-time generator this project does not have yet; see the review
plan's own status note for why that was not rushed in alongside this.

### Packages (`./install.sh package`, CI)

CI builds the `.deb`, `.rpm` and tarball on every push, installs each in a
clean container and starts the program, so a packaging break shows up before
a release is tagged.

## Manual checks

Things a test cannot reach yet, and how they were confirmed.

| Check | How | Last confirmed |
| --- | --- | --- |
| The `.deb` contains the binary, desktop entry, both icons, metainfo and README, with `Recommends: rclone` and the no-reply maintainer address | `dpkg-deb --info` and `--contents` on the built package | M0 |
| The shipped binary has no debug log path | `dpkg-deb -x`, then `strings usr/bin/stellarshot` | M0 |
| The window starts in a COSMIC Wayland session and exits cleanly | Launched for six seconds; no output on stderr, no process left behind | M1 |
| Settings migrate on a real account | First launch copied `~/.config/cosmic/com.github.cosmic-utils.Stellarshot/v1/repositories` to the new ID | M1 |
| The main screen, wizard and profile page render, the estimate fills in, and a remembered password unlocks the page | `scripts/screenshots.sh`, then every image inspected | M2 |
| The keyring test fails, rather than hangs or skips, with no Secret Service | Run in an isolated `dbus-run-session` with a throwaway home: failed after 120 s with the expected message | M2 |
| The CI keyring recipe works | The CI command run locally in an isolated session with a throwaway home: passed, the real keyrings untouched | M2 |
| Déjà Dup settings are detected on a real install | Stellarshot started with empty settings and the real home folder (Déjà Dup 50.2, Flatpak): the first screen offered "Import from Déjà Dup" | M3 |
| Déjà Dup's backups are restic | Its cache and log show `--repo=rclone::drive:<folder>`: the repository is directly in the Drive folder Déjà Dup's settings name | M3 |
| The restore page opens from `--restore` and lists the demo snapshot's folders | `scripts/screenshots.sh restore`, then the image inspected | M4 |
| Generated units are valid, including an executable path with a space and `%` | `systemd-analyze --user verify` on the service and timer: no errors, and the escaped path resolved to the real file | M5 |
| A timer installs, runs its service and uninstalls cleanly | Installed for a throwaway ID in the real user session: listed by `list-timers` with the next run, `enabled`, its service started and exited; after removal no unit, no `timers.target.wants` link and no timer remained | M5 |
| `schedule::next_run` reads the real next-run time over D-Bus | Run against a real scheduled backup's timer in the user session: correct to the minute against `systemctl list-timers`, and `None` for a made-up ID. **Proven able to fail:** first written with the property name zbus derives (`NextElapseUsecRealtime`) and `0` as the "none" sentinel; the real call failed with `Unknown property` (systemd's name capitalizes the unit as `USec`), and a made-up ID silently returned `u64::MAX` seconds rather than `None`, because `LoadUnit` never fails for an unknown name — it returns a `"not-found"` unit instead, caught only by also reading `LoadState` | 0.2 |
| A scheduled run works inside a systemd user service | `systemd-run --user --wait … stellarshot --scheduled <id>` with a demo profile: exit 0, a snapshot, and `last_success` and `last_check` recorded; the keyring was reachable from the service | M5 |
| The wizard no longer clips fields or hides rows under the scrollbar | `scripts/screenshots.sh wizard` before and after: the scrollbar now sits beside the cards instead of over them; a focused SFTP field under Xwayland shows its whole focus ring at the left edge | 0.1.x |
| One press of Next checks an SFTP destination | Under Xwayland with `xdotool`: one press ran the check (a closed port on 127.0.0.1), which failed at once, stayed on the step and left Next ready to try again | 0.1.x |
| Déjà Dup's rclone settings | The running Déjà Dup's rclone environment (names and non-secret values only): its own `RCLONE_DRIVE_CLIENT_ID`, `RCLONE_DRIVE_SCOPE=drive.file`, `RCLONE_DRIVE_USE_TRASH=false`, run by restic as `rclone serve restic --stdio` | 0.1.x |
| Déjà Dup's schedule defaults | Read from the installed Flatpak's `org.gnome.DejaDup.gschema.xml`: `periodic` false, `periodic-period` 7, `delete-after` 0 (forever) | M5 |

### Not yet verified end to end

- **Google sign-in and a backup to a real Google Drive.** Signing in needs a
  person at a browser with a Google account. The rclone transport it uses is
  covered by `tests/rclone.rs`; the sign-in step itself (`rclone config
  create … drive`) has not been run to completion by the tests.
- **Google Drive speed against Déjà Dup.** Four uploads at once and one
  request per pack are covered by `uploads.rs` and the rclone tests; how much
  faster that makes a real Google Drive backup has not been measured. The
  "Checking… 0:12" count and the one-minute limit have not been seen against
  a slow real account either.
- **A backup to a real SSH server.** Covered only through the same rclone
  transport and the remote-string tests.
- **Importing a real Déjà Dup Google Drive backup**, which needs the sign-in
  above.
- **Opening the backup by clicking a failure notification.** Not yet
  triggered on a desktop on purpose.
- **A long-overdue destination's own notification** (0.2, `scheduled::notify_if_overdue`).
  Confirmed by accident rather than on purpose: an early integration test for
  it seeded an old `last_success` and ran the real `--scheduled` binary
  without isolating it from the real session bus, which sent a real
  notification to this machine's desktop and then hung for several minutes
  waiting on it, killed by hand. The notification itself therefore did
  arrive and read correctly; the decision to send it moved to a pure,
  unit-tested function (`overdue_notification_due`) and no automated test
  may call `notify::failure` again — see `tests/scheduled.rs`'s doc comment.
- **A timer firing on its own at its calendar time.** The timer's schedule and
  its service were each confirmed, and systemd starts one from the other.
- **Restoring with numeric or no ownership actually changing a file's owner**
  (0.3, `Ownership::Numeric` / `Ownership::None`). Changing an owner needs
  root, which no automated test may assume; `restoring_with_numeric_or_no_ownership_does_not_fail`
  proves the option is wired through and does not break a normal restore, not
  that ownership itself changes.

## Adding a check

- Name the failure it prevents, in the test name or a comment.
- Make it fail when its fixture or input is missing.
- For anything that deletes, writes or grants access, test against real files,
  and assert on what must **survive** as well as what must change.
- Add it to this document.
