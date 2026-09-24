// SPDX-License-Identifier: GPL-3.0-only

//! Keeping a repository healthy: integrity checks, forgetting old snapshots
//! under a retention policy, and pruning the data no snapshot needs.

use rustic_core::{
    CheckOptions, ForgetGroups, Grouped, KeepOptions, PruneOptions, SnapshotGroupCriterion,
};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

/// Which snapshots to keep. A snapshot is kept if any rule keeps it; with no
/// rules at all, everything is kept.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepRules {
    /// The most recent `n` snapshots.
    pub last: Option<u32>,
    /// The newest snapshot of each of the last `n` hours, days, … that have
    /// one.
    pub hourly: Option<u32>,
    pub daily: Option<u32>,
    pub weekly: Option<u32>,
    pub monthly: Option<u32>,
    pub yearly: Option<u32>,
    /// Every snapshot taken in the last `n` days.
    pub within_days: Option<u32>,
}

impl KeepRules {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn options(&self) -> KeepOptions {
        let count = |n: Option<u32>| n.map(|n| i32::try_from(n).unwrap_or(i32::MAX));
        let mut options = KeepOptions::default();
        options.keep_last = count(self.last);
        options.keep_hourly = count(self.hourly);
        options.keep_daily = count(self.daily);
        options.keep_weekly = count(self.weekly);
        options.keep_monthly = count(self.monthly);
        options.keep_yearly = count(self.yearly);
        options.keep_within = self
            .within_days
            .map(|days| jiff::Span::new().days(i64::from(days)));
        options
    }
}

/// What forgetting did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetReport {
    /// Snapshots removed from the repository's list.
    pub removed: u64,
    /// This computer's snapshots that the rules kept.
    pub kept: u64,
}

/// What pruning did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneReport {
    /// Bytes of data no snapshot needs any more. rustic keeps unused data for
    /// a day before deleting it, in case a backup elsewhere is still using
    /// it, so some of this is freed by the next prune rather than this one.
    pub bytes: u64,
}

/// The name this computer puts on its snapshots.
pub fn hostname() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

impl Repo {
    /// Verify the repository's structure: that every snapshot, tree and index
    /// entry is present and consistent. Pack contents are not re-read.
    pub fn check(&self) -> Result<(), EngineError> {
        let results = self.inner.check(CheckOptions::default())?;
        debug_log!(ENGINE, "check found {} findings", results.0.len());
        results
            .is_ok()
            .map_err(|err| EngineError::new(ErrorKind::RepositoryDamaged, err.to_string()))
    }

    /// Forget the snapshots `rules` do not keep, considering only those
    /// taken on `host`. Another computer backing up to the same repository
    /// has its own policy, and its snapshots are never touched.
    ///
    /// Snapshots are grouped by host, label and folders, as restic does, so
    /// that changing what a backup covers starts a new history instead of
    /// counting against the old one.
    pub fn forget(&self, rules: &KeepRules, host: &str) -> Result<ForgetReport, EngineError> {
        let snapshots = self
            .inner
            .get_matching_snapshots(|snapshot| snapshot.hostname == host)?;
        let total = snapshots.len() as u64;
        if rules.is_empty() {
            return Ok(ForgetReport {
                removed: 0,
                kept: total,
            });
        }
        let grouped = Grouped::from_items(snapshots, SnapshotGroupCriterion::default());
        let ids = ForgetGroups::from_grouped_snapshots_with_retention(
            grouped,
            &rules.options(),
            &jiff::Zoned::now(),
        )?
        .into_forget_ids();
        self.inner.delete_snapshots(&ids)?;
        let removed = ids.len() as u64;
        debug_log!(ENGINE, "forget on {host}: removed {removed} of {total}");
        Ok(ForgetReport {
            removed,
            kept: total - removed,
        })
    }

    /// Remove the data no snapshot needs any more.
    pub fn prune(&self) -> Result<PruneReport, EngineError> {
        let options = PruneOptions::default();
        let plan = self.inner.prune_plan(&options)?;
        // Unused data in packs that go entirely, plus the unused part of
        // packs that are rewritten without it.
        let sizes = plan.stats.size_sum();
        let bytes = sizes.remove + sizes.repackrm;
        self.inner.prune(&options, plan)?;
        debug_log!(ENGINE, "prune: {bytes} bytes unused");
        Ok(PruneReport { bytes })
    }
}

#[cfg(test)]
mod tests {
    //! What "Smart" keeps, rule by rule, exactly as the README and the app
    //! describe it. rustic applies the rules; these tests hold the
    //! description to what it does.

    use crate::profile::Retention;
    use jiff::{Span, Zoned, civil::date, tz::TimeZone};
    use rustic_core::ForgetSnapshot;
    use rustic_core::repofile::SnapshotFile;

    /// `days` before 2026-09-24, at `hour`:00 UTC.
    fn at(days: i64, hour: i8) -> Zoned {
        date(2026, 9, 24)
            .at(hour, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
            .unwrap()
            .checked_sub(Span::new().days(days))
            .unwrap()
    }

    fn smart(times: Vec<Zoned>) -> Vec<ForgetSnapshot> {
        let snapshots = times
            .into_iter()
            .map(|time| SnapshotFile {
                time,
                ..SnapshotFile::default()
            })
            .collect();
        Retention::Smart
            .keep_rules()
            .unwrap()
            .options()
            .apply(snapshots, &at(0, 12))
            .unwrap()
    }

    fn kept_for<'a>(result: &'a [ForgetSnapshot], reason: &str) -> Vec<&'a Zoned> {
        result
            .iter()
            .filter(|s| s.reasons.iter().any(|r| r == reason))
            .map(|s| &s.snapshot.time)
            .collect()
    }

    /// Two backups a day, at 9:00 and 11:00, for 400 days, with none on the
    /// 2nd, 3rd and 4th days back.
    fn history() -> Vec<Zoned> {
        (0..400)
            .filter(|day| !(2..=4).contains(day))
            .flat_map(|day| [at(day, 9), at(day, 11)])
            .collect()
    }

    #[test]
    fn only_the_newest_snapshot_of_a_day_is_kept() {
        let result = smart(history());
        assert!(
            result
                .iter()
                .filter(|s| s.keep)
                .all(|s| s.snapshot.time.hour() == 11),
            "a 9:00 snapshot was kept though 11:00 the same day was newer"
        );
    }

    #[test]
    fn days_without_a_backup_are_passed_over_not_counted() {
        let result = smart(history());
        let daily = kept_for(&result, "daily");
        let expected: Vec<Zoned> = [0, 1, 5, 6, 7, 8, 9].iter().map(|d| at(*d, 11)).collect();
        assert_eq!(daily, expected.iter().collect::<Vec<_>>());
    }

    #[test]
    fn weeks_and_months_are_counted_the_same_way_and_overlap_with_days() {
        let result = smart(history());
        assert_eq!(kept_for(&result, "weekly").len(), 4);
        assert_eq!(kept_for(&result, "monthly").len(), 12);
        // Today's snapshot is the newest of its day, week and month at once.
        let today = result
            .iter()
            .find(|s| s.snapshot.time == at(0, 11))
            .unwrap();
        for reason in ["daily", "weekly", "monthly"] {
            assert!(today.reasons.iter().any(|r| r == reason), "{reason}");
        }
        let kept = result.iter().filter(|s| s.keep).count();
        assert!(kept < 7 + 4 + 12, "overlapping rules keep fewer than 23");
        // Weeks run Monday to Sunday. The week of Monday 14 to Sunday 20
        // September has no backup on the 20th, so its newest is the 19th.
        let weekly = kept_for(&result, "weekly");
        assert_eq!(weekly[..2], [&at(0, 11), &at(5, 11)]);
    }

    #[test]
    fn the_first_snapshot_is_kept_until_twelve_months_are_covered() {
        // Two days of backups: the very first one stays, though a newer one
        // exists on its day.
        let result = smart(vec![at(1, 9), at(1, 11), at(0, 11)]);
        let first = result.iter().find(|s| s.snapshot.time == at(1, 9)).unwrap();
        assert!(first.keep);

        // More than a year of backups: it goes like any other.
        let result = smart(history());
        let first = result
            .iter()
            .find(|s| s.snapshot.time == at(399, 9))
            .unwrap();
        assert!(!first.keep);
    }
}
