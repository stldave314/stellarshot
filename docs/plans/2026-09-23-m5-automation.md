# M5: Automation, implementation plan

**Goal:** Backups run on their own:
- **Schedules:** hourly, daily or weekly, through systemd user timers.
- **Retention:** old snapshots are forgotten under a policy, and their space is reclaimed where that is safe.
- **Notifications:** a scheduled run that fails raises a desktop notification. Clicking it opens the backup.
- **Checks:** the repository is checked for damage every 30 days.

**Architecture:**
- **`schedule`** writes one `.service` and one `.timer` per scheduled profile into `~/.config/systemd/user/`.
  - Unit files contain no user-supplied text: only the executable path (quoted and escaped for systemd) and the profile ID, which is validated.
  - The timer is `Persistent=true`, so a slot missed while the computer was off runs at the next login.
  - The service runs at `Nice=10` with idle I/O priority.
  - Every launch of the window reconciles the units with the settings. That covers edits from another window, a moved executable and a removed profile.
- **`stellarshot --scheduled <profile-id>`** is the timer's command:
  - It reads the profile from the settings and the password from the keyring.
  - It runs the backup through `runner::run`, the same function the window's child process uses.
  - Then, in order: forget under the retention policy, a check if one is due, and prune if it is enabled and the last check found no damage.
  - An unreachable destination (a drive not plugged in, no network) or a repository another process is writing to is skipped quietly, and the next slot tries again. Anything else is recorded and notified.
- **Run facts** are kept in cosmic-config *state*, one key per profile: `last_success`, `last_check`, the last `failure` and `damaged`. They are not kept in the profile list, so a scheduled run can never overwrite a settings edit made in the window at the same moment. The window re-reads them on its 30-second tick.
- **`engine::maintenance`** gains two operations:
  - `forget(&KeepRules, host)`: only this computer's snapshots are considered, grouped by host, label and paths, because another computer backing up to the same repository has its own policy.
  - `prune()`: rustic's defaults, including the 23-hour delay before packs marked unused are deleted, which protects a backup running elsewhere at the same time.
- **The runner** gains `--run maintain`, which forgets and prunes (the window's **Clean Up Now**). `Job` gains `keep` and `prune`.
- **`notify`** sends `org.freedesktop.Notifications.Notify` over zbus, which is already in the dependency tree through oo7. It waits a bounded time (`NOTIFICATION_WAIT`) for the default action, then starts `stellarshot --profile <id>`.
- **UI:**
  - A **When** step in the wizard (create, open and Déjà Dup import), with automatic backups, how often, what to keep and "free up space automatically".
  - A **Schedule and cleanup** editor on the profile page.
  - **Check Now** and **Clean Up Now**.
  - A banner for the last scheduled failure or found damage, and a warning icon in the sidebar.

**Retention deviates from the spec:** `Custom(KeepSpec)` becomes `KeepFor { days }`. It is Déjà Dup's "Keep at least…" choice (3 months, 6 months, a year), which is what people pick and what Déjà Dup's `delete-after` imports into. Arbitrary keep-rules remain possible with `rustic forget` on the command line.

**Spec:** §3 (M5), §4, §5.3, §8

## Review focus

1. **A missed slot.** The computer was off at midnight, so the backup runs at the next login (`Persistent=true`). Test: `timer_units_catch_up_missed_runs`.
2. **The drive is not plugged in.** The run exits 0 with no failure and no notification, and the next slot tries again. Test: `an_unplugged_destination_is_skipped_quietly`.
3. **Forget only touches this computer's snapshots.** Test: `forget_leaves_other_computers_alone`.
4. **Prune after a check found damage.** It is skipped until a check passes. Test: `prune_waits_for_a_clean_check`.
5. **An executable path with spaces, `%` or `$`**, as in a home-built copy in `~/My Apps`. Test: `exec_paths_are_escaped_for_systemd`.

---

### Task 1: Engine (`src/engine/maintenance.rs`)
- `KeepRules { last, hourly, daily, weekly, monthly, yearly: Option<u32>, within_days: Option<u32> }` maps to rustic's `KeepOptions`.
- `Repo::forget(&self, &KeepRules, host: &str) -> ForgetReport { removed, kept }`.
- `Repo::prune(&self) -> PruneReport { bytes }`.
- `engine::hostname()`.
- `BackupRequest.time: Option<i64>`, for tests and the demo repository.
- **Tests:** `forget_applies_the_rules`, `forget_leaves_other_computers_alone`, `forget_keeps_everything_without_rules`, `prune_reclaims_forgotten_data`.

### Task 2: Profile, run state and runner
- `Retention::{KeepForever, Smart, KeepFor { days }}` with `keep_rules()`.
- `Profile.prune: Option<bool>` with `prune_enabled()`, which defaults to on for Local and Removable destinations.
- `RunState` in cosmic-config state.
- `Operation::Maintain`.
- **Tests:** retention mapping, prune defaults, `runner_maintains`.

### Task 3: Scheduling (`src/schedule.rs`, `src/scheduled.rs`, `src/notify.rs`)
- Unit text, install, remove and reconcile.
- The `--scheduled` flow, built as a pure `plan(state, profile, now)` for what runs after the backup.
- Notifications.
- **Tests:** unit text, escaping, ID validation, the plan's decisions, and `tests/scheduled.rs` end to end: a real backup through `--scheduled` with the password in the keyring, and a quiet skip.

### Task 4: UI
- The When step, the schedule editor, Check Now, Clean Up Now, the failure banner, the sidebar icon, `--profile <id>` and the Déjà Dup schedule and retention.
- Strings in all five locales.

### Task 5: Verify, document, commit, push
