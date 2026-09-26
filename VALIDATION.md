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
(`.github/workflows/ci.yml`), with `RUSTFLAGS=-D warnings`. A tag runs them
again before publishing (`.github/workflows/release.yml`).

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
| `packs_upload_side_by_side` (`uploads.rs`) | Eight packs to storage that takes 200 ms per write go four at a time: four writes are seen in flight together, and the whole takes well under the 1.6 s one at a time would |
| `an_index_is_written_only_after_every_pack_arrived` (`uploads.rs`) | Writing an index waits for every pack in flight: all six are stored when it returns |
| `a_failed_upload_fails_the_index_and_everything_after_it` (`uploads.rs`) | When one pack upload fails, the index write fails, so no index can name a missing pack, and so does every write after it; only packs reach the storage |

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
| `overwrite_replaces_changed_files` | Overwrite puts the backed-up content back over an edited file |
| `skip_restores_only_what_is_missing` | In a folder where some files exist and some do not, only the missing ones are written |
| `preview_counts_match_the_restore` | The dry run's counts equal what the restore then does, and the dry run leaves the missing file missing and the edited file edited |
| `preview_creates_nothing` | A dry run into a folder that does not exist yet does not create it |
| `restores_into_a_deleted_folder` | Restoring to the original place recreates missing parent folders |

The restore page's rules are tested without a window (`src/app/pages/restore.rs`):
Keep both and the original place are the defaults; **Restore** does nothing
until a successful dry run exists for exactly the current choices (a late
answer for earlier choices is ignored); a selection spanning several
snapshots becomes one restore per snapshot, run in turn; Cancel drops the rest;
a folder missing from another snapshot falls back to the starting folder, then
the top, then reports an error instead of looping.

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
| `a_projects_own_gitignore_is_honoured_without_needing_a_git_repository` | A plain `.gitignore`, with no `.git` folder at all, still excludes what it lists; the `.gitignore` file itself is still backed up |
| `files_larger_than_the_limit_are_excluded`, `case_insensitive_patterns_match_either_case`, `patterns_kept_in_a_file_are_applied` | The three extra exclusion rules each work; the case-insensitive and file-based ones needed a leading `!` added by Stellarshot itself, caught by a first version of the test failing because the unmodified pattern restricted the backup to only the excluded files instead of leaving them out |
| `an_unchanged_backup_is_skipped_when_asked` | Backing up twice with nothing changed adds no second snapshot; a real change afterward still is recorded |
| `a_dry_run_reports_size_without_writing_anything` | A dry run reports the size it would add and leaves the repository with no snapshot; a real backup right after adds exactly that much |
| `extended_attributes_are_saved_and_restored` | A user extended attribute set on a file survives a backup and restore. This needed no production code: rustic saves and restores them by default |
| `a_pinned_snapshot_survives_forget_that_would_otherwise_remove_it`, `pinning_an_already_pinned_snapshot_is_a_harmless_no_op` | Pinning the oldest of several daily snapshots keeps it through a `forget` that would otherwise remove it; pinning twice does not change its ID again. **Caught by the test:** pinning changes a snapshot's ID (it is a hash of the snapshot's own content), so the first version of `set_pinned` returned the old, now-deleted ID; fixed by finding the new one from the snapshot list before and after |
| `verify_existing_catches_content_that_looks_unchanged` | A restored file corrupted to the same size and modification time as the backup is left alone by default and only rewritten with `verify_existing` on |
| `restoring_with_numeric_or_no_ownership_does_not_fail` | Each ownership choice completes a restore without error. Actually changing an owner needs root; see "Not yet verified end to end" |
| `runner_pins_a_snapshot_and_protects_it_from_maintain` (`tests/runner.rs`) | The same pin, through `--run set-pinned` and then `--run maintain`, the way the window actually calls it |

### Storage and credentials (0.4)

| Test | What it proves |
| --- | --- |
| `a_bandwidth_limit_is_passed_to_rclone`, `no_bandwidth_limit_adds_no_flag`, `a_local_location_ignores_a_bandwidth_limit`, `a_bandwidth_limit_cannot_inject_a_second_rclone_argument` (`src/engine/repo.rs`) | A limit reaches the rclone command as `--bwlimit <value>`, found by splitting the built command with `shell_words` the same way rustic does, not a substring check; none is added when empty; a local destination, which has no transfer to limit, ignores it; a value crafted to look like `1M' --password-command 'evil` survives as one single argument rather than becoming two — a real review finding, see the code-review entry below |
| `a_profiles_bandwidth_limit_reaches_its_rclone_location` (`src/profile.rs`) | The whole path from a profile's own field to the location rustic actually opens |
| `the_commands_output_becomes_the_password`, `a_trailing_newline_is_trimmed_but_no_more_than_one`, `a_failing_command_reports_its_stderr`, `empty_output_is_refused_rather_than_an_empty_password`, `unmatched_quoting_is_reported_rather_than_run_incorrectly` (`src/password_command.rs`) | A password command's real output becomes the password; exactly one trailing newline is trimmed, not more; a failing command's own stderr is the reported error; empty output is refused rather than silently becoming an empty password; unparseable quoting is reported rather than run some other way |
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
| The logging fix earlier in this release only reached the window's own process; every real backup runs in a `--run` child or a `--scheduled` run, neither of which ever installed it, so the original bug (a real Google Drive failure with nothing in the log) was still not actually fixed | Both entry points now call `set_logger_for_child()`, which appends rather than truncates — several such processes, and the window, can share the one log file without racing to clobber each other's lines | `a_child_process_appends_rather_than_truncating`-equivalent coverage split across `truncate_false_appends_instead_of_overwriting` (`src/debug.rs`, the file mechanics) and the existing `rustic_backend_output_reaches_the_log_file` (`src/app/settings.rs`, the `log`-to-file bridge); the two are not tested together on purpose — `tracing`'s global subscriber can only be installed once per process, so a second test calling `init_tracing` in the same binary would pass or fail depending on test order rather than proving anything |
| A settings export left a REST destination's URL, credentials and all, in plain text, contradicting the export's own "never a password" promise | `Export::collect` redacts it with `redact_url`, the same function that already kept it out of messages and logs | `a_rest_destinations_credentials_are_stripped_on_export` (`src/settings_export.rs`) |
| `redact_url` failed open: a URL whose password contains an unescaped `/`, `?` or `#` fails to parse at all, and the old code returned the original string, credentials and all, in that case | Returns a fixed, non-leaking placeholder instead when parsing fails, rather than guessing where credentials end | `redact_url_shows_nothing_of_a_url_it_cannot_parse`, plus `redact_url_removes_a_parseable_urls_credentials` and `redact_url_leaves_a_credential_free_url_alone` for the normal cases, none of which existed before (`src/engine/repo.rs`) |
| A crafted or hand-edited settings export could carry a `password_command` through import even though a real export already clears it, and a bandwidth limit was hand-quoted into the rclone command string with a plain `'...'`, so a value containing a quote could end its own argument and start another — including `--password-command`, which rclone would then run | `merge` also clears `password_command` on import, not trusting the file; the rclone command is built with `shell_words::quote`, which cannot be broken out of | `a_password_command_from_an_untrusted_export_is_cleared_on_import` (`src/settings_export.rs`); `a_bandwidth_limit_cannot_inject_a_second_rclone_argument` (`src/engine/repo.rs`) |
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
| `cancelling_stops_the_walk`, `an_unreadable_root_reports_an_error_not_an_empty_list` (`src/engine/disk_tree.rs`) | A set cancel flag stops a walk in progress rather than being ignored; a folder that cannot even be opened is a real error, not silently reported as holding nothing |
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

### A History page across every backup (`src/event_log.rs`, `src/app/pages/history.rs`, `src/app/pages/profile.rs`, `src/app.rs`)

| Test | What it proves |
| --- | --- |
| `merge_sorted_interleaves_every_profiles_events_newest_first` (`src/event_log.rs`) | Events from more than one profile's log come back merged into a single newest-first list, not grouped by profile or left in per-profile order |
| `an_event_logged_before_source_existed_is_read_back_as_desktop` (`src/event_log.rs`) | A log entry written before the `source` field existed (no such key in its stored form at all) deserializes as `Source::Desktop`, not a parse failure — the same backward-compatibility pattern already proven for profile fields |
| `every_new_event_kind_describes_itself_with_the_snapshots_short_id` (`src/event_log.rs`) | Every new `EventKind` (restore, snapshot deletion, pin/unpin, password change, mount/unmount) renders as a sentence that names the *short* snapshot ID, not the full hash |
| `a_finished_snapshot_deletion_logs_which_one`, `a_failed_snapshot_deletion_logs_nothing`, `a_finished_pin_change_logs_which_way_it_went` (`src/app/pages/profile.rs`) | Deleting or pinning a snapshot logs exactly what was asked for once the child process actually finishes — not merely that some write happened — and a failed deletion logs nothing rather than a wrong success entry |

Five kinds of action gained a log entry that had none before: restore, snapshot deletion, pin/unpin, password change, and mount/unmount — previously only backup, check, clean-up, skip and failure were recorded at all. Each `Event` also now carries a `Source` (`Desktop` or `Web`), defaulted for every entry recorded before this field existed; nothing writes `Source::Web` yet, since the web interface itself does not exist, but the History page already renders a "Web" badge for one the moment something does, without a later migration of already-recorded history.

Not covered by an automated test: the History page's own `view()` in `src/app/pages/history.rs` (a pure rendering function, no interactive `Message` of its own yet) or the sidebar entry and its data-loading `Task` in `src/app.rs` — UI wiring, following the same pattern already noted above for Download, Open Copy and the mount buttons. The 500-entry display cap (`history::LIMIT`) is exercised by nothing but its own arithmetic; not proven against an actual machine with that much history.

### Web interface settings (`src/app/config.rs`, `src/keyring.rs`, `src/web_token.rs`, `src/app.rs`)

The settings surface for a planned web interface and REST API: a network scope, three independent authentication toggles, and an IP allow-list. The network scope and the allow-list are now genuinely enforced by the daemon in the next section; the three authentication methods are not enforced by anything yet.

| Test | What it proves |
| --- | --- |
| `the_web_interface_defaults_to_off` (`src/app/config.rs`) | A fresh config, or one saved before this setting existed, always comes up with the network scope off — never silently listening by default |
| `a_generated_tokens_hash_verifies_it`, `a_wrong_token_does_not_verify`, `two_generated_tokens_are_never_the_same`, `the_hash_never_equals_the_raw_token` (`src/web_token.rs`) | A generated API token's hash verifies exactly that token and no other, two generated tokens never collide, and the stored hash is never mistakable for the raw token itself |
| `web_password_round_trip` (`tests/keyring.rs`) | The web interface's shared password round-trips through a real Secret Service (GNOME Keyring, unlocked, in this sandbox) — stored, read back, replaced, and restored to whatever was there before the test ran, since this secret (unlike a profile's) has no ID of its own to test against safely |

Not covered by an automated test: the Settings page's own new controls in `src/app.rs` (the scope radio buttons, the two toggles-with-detail for password and token, the allow-list add/remove row) — UI wiring, following the same pattern as every other settings control already in this section. Not proven at all: PAM authentication itself. Its checkbox exists and its setting persists, but nothing calls into PAM yet; whether verifying a Linux user's own password from an unprivileged per-user service actually works (via `pam_unix`'s `unix_chkpwd` helper, which by design only checks the calling user's own password) is design research recorded in ROADMAP.md, not a running, tested code path.

### The web interface's own daemon (`src/web.rs`, `src/bin/web.rs`, `src/web_token.rs`)

The `stellarshot-web` binary: binds according to the network scope setting, enforces the IP allow-list, then checks authentication — password and token for real, PAM not wired up yet — ahead of the one route that exists (a health check). Order matters and is proven, not assumed: the allow-list runs before authentication, so an address that was never going to be let in is rejected before its credentials are even looked at.

| Test | What it proves |
| --- | --- |
| `the_off_scope_binds_nowhere`, `the_localhost_scope_binds_only_loopback`, `the_lan_scope_binds_every_interface` | Each network scope maps to the intended bind address, not merely "some address" |
| `an_empty_allow_list_allows_everything`, `a_single_address_only_allows_itself`, `a_cidr_range_allows_every_address_inside_it`, `an_unparseable_entry_matches_nothing_rather_than_panicking` | The allow-list's matching logic: empty means unrestricted, a single address matches only itself, a CIDR range matches everything inside it and nothing outside, and a garbled entry fails closed (matches nothing) rather than panicking the whole server |
| `no_credentials_at_all_are_never_authenticated`, `nothing_authenticates_when_no_method_is_enabled`, `the_correct_shared_password_authenticates`, `the_wrong_shared_password_does_not_authenticate`, `a_valid_bearer_token_authenticates`, `an_invalid_bearer_token_does_not_authenticate`, `basic_password_ignores_the_username` | The authentication decision itself: each enabled method accepts only its own correct credential, rejects a wrong one, and — the fail-closed default — no method enabled means no request ever authenticates, regardless of what credentials it carries |
| `constant_time_eq_still_compares_correctly` (`src/web.rs`), `a_generated_tokens_hash_verifies_it`, `a_wrong_token_does_not_verify` (`src/web_token.rs`) | The constant-time comparison used for both the password and the token hash still gets equal/unequal cases right — a real risk when replacing `==` with something else, since a comparison that is "safe" but wrong would fail silently open or closed depending on which way the bug went |
| `a_real_request_from_an_address_not_on_the_allow_list_is_forbidden`, `a_real_request_is_rejected_when_no_auth_method_is_enabled`, `a_real_request_with_the_correct_shared_password_reaches_the_health_route`, `a_real_request_with_the_wrong_shared_password_is_unauthorized`, `a_real_request_with_a_valid_api_token_reaches_the_health_route` | Against a real `TcpListener` and a real `axum::serve` on an ephemeral port, not a mocked request: each scenario isolates the layer it claims to test (the allow-list test uses *valid* credentials, so a 403 there cannot be authentication in disguise; the no-method-enabled test uses an *unrestricted* allow-list, so a 401 there cannot be the allow-list) |

Beyond the automated tests, this was verified against the actual compiled binary, isolating the layer under test the way a directory-permission claim needs isolating from a web-server config claim. With the allow-list empty and scope set to `Localhost`: `curl` with no credentials returned a real `200` (before authentication existed) or a real `401` (after); `curl` against the machine's own LAN address on the same port was refused outright (`Connection refused`), proving `Localhost` scope is not reachable from the network at all. With the allow-list set to an address that is not the loopback address, a request that previously succeeded came back a real `403`. With token authentication enabled and a real generated token's hash in the config: a request with no `Authorization` header came back `401`, one with the wrong token came back `401`, and one with the correct token came back `200` with the health body — run against the actual binary, not only the in-process test harness, and without ever touching this machine's real settings (a throwaway `XDG_CONFIG_HOME` for each run). The password path was proven the same way inside the automated tests but not against the real compiled binary, since doing so would exercise the real OS keyring the same "web interface password" entry `tests/keyring.rs`'s own round-trip test already has to carefully preserve and restore — not risked twice in one session. Not proven: `Lan` scope actually being reachable from a second machine on the network (this sandbox has no second machine to test from).

Not built yet: a systemd unit to run this as the always-on per-user service the design calls for, PAM authentication itself, and a configurable port (fixed at `8737`). Changing an authentication setting or the shared password currently requires restarting the daemon by hand to take effect, since the password is read from the keyring once at startup rather than on every request.

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

`Browser` moved from a size-optimized index (`IndexedIdsStatus`) to a fully-loaded one (`IndexedFullStatus`, via `to_indexed()` rather than `to_indexed_ids()`) so `dump` is available to call at all; every other browse operation (list, search, versions, diff, missing) keeps working unchanged, since `IndexedFullStatus` is a superset. Not covered by an automated test: the "Download…" buttons themselves in `src/app/pages/restore.rs`, or the save-file dialog they open — UI wiring, not new logic, following the same pattern already proven for **Open Copy** and **Restore This Version…**.

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
- **Single-instance activation works for real.** With the window running, launching `stellarshot` a second time exited in 16ms with no output, and the first instance was still running afterward with no crash — confirming `App::dbus_activation`'s "a window already exists" branch (`window::gain_focus` + `window::minimize(id, false)`) runs cleanly on a real system bus, not just that it compiles. Not verified the same way: the "window was closed to the panel, reopen it" branch (`window::open`, then `core.set_main_window_id`), since reaching that state needs an interactive window close this environment cannot reliably script (see this file's own note on AT-SPI-based UI testing), and the applet's popup layout and icon-swapping, since neither can be seen without registering the applet in a real panel, which was deliberately not done here to avoid changing a real, currently-running session.
- **`stellarshot-applet`'s release build strips developer logging too**, not just the window's: `scripts/verify-release-build.sh` now checks both binaries, and the applet's own build genuinely does reach a `debug_log!` call somewhere in the shared library (the check found the log path present without `release-build` and gone with it, the same as the window) — not a check that would have passed regardless of what the feature flag did.

### Hooks (`src/hooks.rs`, `src/runner.rs`, `tests/runner.rs`, `src/app/wizard/mod.rs`)

| Test | What it proves |
| --- | --- |
| `a_before_hook_that_fails_stops_the_rest`, `every_before_hook_running_cleanly_reports_ok` (`src/hooks.rs`) | `Before` hooks run in order and stop at the first failure, without running the ones after it |
| `after_hooks_run_for_the_matching_outcome_and_always`, `one_after_hook_failing_does_not_stop_the_rest` (`src/hooks.rs`) | An `After` hook runs only for its own outcome (`AfterSuccess`/`AfterFailure`) or `After` regardless; one failing does not stop the rest, since the backup itself has already finished |
| `a_disabled_or_differently_timed_hook_does_not_run`, `a_failing_hook_reports_its_stderr`, `unmatched_quoting_is_reported_rather_than_run_incorrectly` (`src/hooks.rs`) | A disabled hook, or one for a different timing, never runs; a failure's detail comes from the hook's own stderr; a malformed command line is reported rather than run incorrectly |
| `a_hook_that_never_finishes_is_killed_rather_than_hanging_forever` (`src/hooks.rs`) | A hook stuck past its timeout is killed and reported as a failure, not left to hang the fully synchronous call it runs inside |
| `a_before_hook_runs_before_the_backup_and_can_be_seen_by_it`, `a_failing_before_hook_stops_the_backup_from_running_at_all`, `a_disabled_before_hook_does_not_run`, `an_after_success_hook_runs_once_the_backup_has_actually_finished`, `an_after_success_hook_does_not_run_after_a_failed_backup` (`tests/runner.rs`) | The wiring into a real backup, not just the pure hook-running logic: a `Before` hook's marker file exists once the real `stellarshot --run backup` child reports done; a failing one leaves no snapshot at all; a disabled one never runs; an `AfterSuccess` hook's marker only appears once the backup has genuinely finished, and never appears after a backup that failed (a wrong password) |
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
| `only_unreachable_and_busy_repositories_are_skipped_quietly` | Everything else is reported |
| `a_failure_shows_until_a_later_success` | The page's warning goes once a backup succeeds, from the timer or the window |
| `a_scheduled_backup_runs_checks_and_is_recorded` | The real binary, settings from cosmic-config and the password from a real keyring: one snapshot, a check, and the run recorded with no failure |
| `an_unplugged_destination_is_skipped_quietly` | Exit status 0 and no failure recorded when the repository's drive is not there |
| `runner_maintains_by_forgetting_then_pruning` (`tests/runner.rs`) | `--run maintain` forgets and prunes, reporting both |

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
| `backup_finishes_after_the_window_goes_away` | Closing the reading end of the pipe mid-backup does not stop it; the snapshot is recorded |
| `runner_restores_a_selection_keeping_both` | A selective restore through the child process, as the Restore button runs it: `done` carries the counts, the existing file is untouched, and exactly one dated copy appears |
| `a_backup_streams_started_progress_and_done` | The window's stream yields `Started`, progress, then `Done` |
| `cancelling_a_backup_ends_it_as_cancelled_without_a_snapshot` | Cancel from the window's handle ends the stream as `Canceled`, with no snapshot left behind |
| `a_missing_executable_is_reported_not_hung` | If the child cannot start, the stream ends with an error instead of waiting forever |
| `cancelling_a_backup_through_rclone_ends_it_and_stops_rclone` (`tests/rclone.rs`) | Cancel during a backup through rclone ends the stream as `Canceled` within 30 seconds, and no `rclone serve` for the repository is left running. **Proven able to fail:** before the child ran in its own process group, the stream never ended (the orphaned rclone held its output open) and the test timed out |

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
| `the_arithmetic_adds_up_to_the_estimate` | "Included − excluded = total" holds exactly; each excluded folder is sized, nested ones count once towards the total, and what only a pattern removes is reported apart |

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

Fixtures modelled on a real Déjà Dup 50 Flatpak keyfile and on `dconf dump`
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

### The keyring (`tests/keyring.rs`)

`keyring_round_trip` stores, reads, replaces and forgets a password in a real
Secret Service, under a random profile ID so it can never touch a real saved
password. **Cannot pass vacuously:** without a Secret Service it fails (after
the keyring timeout) with "a Secret Service must be running and unlocked for
this test"; this was run and observed. CI runs it against an unlocked
gnome-keyring in a private D-Bus session.

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

### rustic's and rclone's diagnostics reach a log (`src/app/settings.rs`, `src/debug.rs`)

`rustic_backend_output_reaches_the_log_file` sets up the real logger against a
private path, logs through the `log` crate under `rustic_backend`'s own
target the way rclone's own output does, and reads that path back. **Proven
able to fail:** before the fix, `set_logger` built a `tracing` subscriber but
never called `tracing_log::LogTracer::init`, so nothing from `log` (all of
rustic_core, rustic_backend and rclone) ever reached it; this test would have
found an empty file.

Both this log and the developer debug log sit at a fixed, predictable path
under `/tmp`, so `debug::open_private_log_file` — the one place either is
opened — refuses to follow a symlink already there and creates the file mode
`0600`. `a_symlink_already_at_the_path_is_not_followed` plants a symlink to a
throwaway file first and checks both that opening it is refused and that the
symlink's target is untouched; `the_log_file_is_private` checks the mode bits
of a freshly opened one.

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
- **Cannot pass vacuously:** it fails if the English file is missing or has
  suspiciously few messages, rather than comparing nothing.

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

### Packages (`./install.sh package`, CI)

CI builds the `.deb`, `.rpm` and tarball on every push and prints the `.deb`'s
control data and file list, so a packaging break shows up before a release is
tagged.

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
| `schedule::next_run` reads the real next-run time over D-Bus | Run against a real scheduled backup's timer in the user session: correct to the minute against `systemctl list-timers`, and `None` for a made-up ID. **Proven able to fail:** first written with the property name zbus derives (`NextElapseUsecRealtime`) and `0` as the "none" sentinel; the real call failed with `Unknown property` (systemd's name capitalises the unit as `USec`), and a made-up ID silently returned `u64::MAX` seconds rather than `None`, because `LoadUnit` never fails for an unknown name — it returns a `"not-found"` unit instead, caught only by also reading `LoadState` | 0.2 |
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
