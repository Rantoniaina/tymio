//! Time & attendance — days worked, hours worked and overtime, per employee
//! per month.
//!
//! Payroll reads these numbers directly, so two things matter more than
//! anything else here: they are exact (integers throughout, never a float),
//! and they can be reproduced. "Fill from standard schedule" derives them from
//! the project's work calendar; a human can then override them, and `source`
//! remembers which happened last.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::calendar::{HolidaySet, WorkCalendar, YearMonth};
use super::employee::EmployeeId;
use super::project::ProjectId;
use super::{id_type, ValidationErrors};

/// Nobody works more than this in a day, so nothing recorded for a month may
/// exceed it times the days in that month.
const MINUTES_PER_DAY: i64 = 24 * 60;

id_type! {
    /// Identifies one month's attendance for one employee.
    AttendanceId
}

/// Days worked, counted in half-days.
///
/// Half a day is the finest granularity the design asks for — leave is taken
/// in half-days — and counting halves keeps the type an integer, so a month's
/// total can never drift the way a float would.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct WorkedDays(i32);

impl WorkedDays {
    pub const ZERO: Self = WorkedDays(0);

    pub fn from_halves(halves: i64) -> Result<Self, AttendanceError> {
        if halves < 0 {
            return Err(AttendanceError::Negative("days worked"));
        }
        i32::try_from(halves)
            .map(WorkedDays)
            .map_err(|_| AttendanceError::OutOfRange("days worked"))
    }

    pub fn from_days(days: u32) -> Self {
        WorkedDays(days as i32 * 2)
    }

    pub fn halves(self) -> i32 {
        self.0
    }

    pub fn whole_days(self) -> i32 {
        self.0 / 2
    }

    pub fn has_half(self) -> bool {
        self.0 % 2 == 1
    }

    /// Saturating, because days worked can never be negative: subtracting more
    /// leave than there are working days leaves zero, not a debt.
    pub fn saturating_sub(self, other: Self) -> Self {
        WorkedDays((self.0 - other.0).max(0))
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(WorkedDays)
    }
}

impl fmt::Display for WorkedDays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_half() {
            write!(f, "{}.5", self.whole_days())
        } else {
            write!(f, "{}", self.whole_days())
        }
    }
}

impl TryFrom<i64> for WorkedDays {
    type Error = AttendanceError;

    fn try_from(halves: i64) -> Result<Self, Self::Error> {
        Self::from_halves(halves)
    }
}

impl From<WorkedDays> for i64 {
    fn from(days: WorkedDays) -> Self {
        i64::from(days.0)
    }
}

/// A span of worked time, held in whole minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct WorkedTime(i32);

impl WorkedTime {
    pub const ZERO: Self = WorkedTime(0);

    pub fn from_minutes(minutes: i64) -> Result<Self, AttendanceError> {
        if minutes < 0 {
            return Err(AttendanceError::Negative("worked time"));
        }
        i32::try_from(minutes)
            .map(WorkedTime)
            .map_err(|_| AttendanceError::OutOfRange("worked time"))
    }

    pub fn from_hours(hours: u32) -> Self {
        WorkedTime(hours as i32 * 60)
    }

    pub fn minutes(self) -> i32 {
        self.0
    }

    /// The whole-hours and leftover-minutes split, for display.
    pub fn split(self) -> (i32, i32) {
        (self.0 / 60, self.0 % 60)
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(WorkedTime)
    }

    /// `days × day length`, saturating rather than wrapping on absurd input.
    pub fn for_days(days: WorkedDays, day_length_minutes: u32) -> Self {
        let minutes = i64::from(days.halves()) * i64::from(day_length_minutes) / 2;
        WorkedTime(i32::try_from(minutes).unwrap_or(i32::MAX))
    }
}

impl fmt::Display for WorkedTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.split() {
            (h, 0) => write!(f, "{h} h"),
            (h, m) => write!(f, "{h} h {m:02}"),
        }
    }
}

impl TryFrom<i64> for WorkedTime {
    type Error = AttendanceError;

    fn try_from(minutes: i64) -> Result<Self, Self::Error> {
        Self::from_minutes(minutes)
    }
}

impl From<WorkedTime> for i64 {
    fn from(time: WorkedTime) -> Self {
        i64::from(time.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttendanceError {
    #[error("{0} cannot be negative")]
    Negative(&'static str),
    #[error("{0} is too large to record")]
    OutOfRange(&'static str),
    #[error("{0:?} is not an attendance source (expected schedule or manual)")]
    UnknownSource(String),
}

/// Where a row's numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttendanceSource {
    /// Seeded from the project's work calendar.
    #[default]
    Schedule,
    /// Typed in, or adjusted, by a person.
    Manual,
}

impl AttendanceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AttendanceSource::Schedule => "schedule",
            AttendanceSource::Manual => "manual",
        }
    }
}

impl fmt::Display for AttendanceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AttendanceSource {
    type Err = AttendanceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "schedule" => Ok(AttendanceSource::Schedule),
            "manual" => Ok(AttendanceSource::Manual),
            other => Err(AttendanceError::UnknownSource(other.to_owned())),
        }
    }
}

/// One employee's month, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceEntry {
    pub id: AttendanceId,
    pub employee_id: EmployeeId,
    pub period: YearMonth,
    pub days_worked: WorkedDays,
    pub hours_worked: WorkedTime,
    pub overtime: WorkedTime,
    pub source: AttendanceSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What the attendance grid submits for one row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceDraft {
    /// In half-days: 43 is 21.5 days.
    pub days_worked_halves: i64,
    pub hours_worked_minutes: i64,
    pub overtime_minutes: i64,
}

impl AttendanceDraft {
    pub fn new(days_worked_halves: i64, hours_worked_minutes: i64, overtime_minutes: i64) -> Self {
        AttendanceDraft { days_worked_halves, hours_worked_minutes, overtime_minutes }
    }

    /// A whole number of days at a given day length, with no overtime.
    pub fn of_days(days: u32, day_length_minutes: u32) -> Self {
        let worked = WorkedDays::from_days(days);
        AttendanceDraft {
            days_worked_halves: i64::from(worked.halves()),
            hours_worked_minutes: i64::from(WorkedTime::for_days(worked, day_length_minutes).minutes()),
            overtime_minutes: 0,
        }
    }

    /// Checks every rule and reports all the failures at once.
    pub fn validate(self, context: AttendanceContext) -> Result<ValidAttendance, ValidationErrors> {
        let mut errors = ValidationErrors::new();
        let days_in_month = i64::from(context.period.day_count());

        let days_worked = match WorkedDays::from_halves(self.days_worked_halves) {
            Ok(days) if i64::from(days.halves()) > days_in_month * 2 => {
                errors.push(
                    "daysWorked",
                    format!("{} has only {days_in_month} days", context.period),
                );
                WorkedDays::ZERO
            }
            Ok(days) => days,
            Err(error) => {
                errors.push("daysWorked", error.to_string());
                WorkedDays::ZERO
            }
        };

        let hours_worked = time_field(&mut errors, self.hours_worked_minutes, "hoursWorked");
        let overtime = time_field(&mut errors, self.overtime_minutes, "overtime");

        // Nobody logs more than 24 hours in a day, ordinary and overtime
        // together. This is the check that catches a fat-fingered zero.
        let logged = i64::from(hours_worked.minutes()) + i64::from(overtime.minutes());
        if logged > days_in_month * MINUTES_PER_DAY {
            errors.push(
                "hoursWorked",
                format!(
                    "{} cannot hold more than {} hours",
                    context.period,
                    days_in_month * 24
                ),
            );
        }

        // Attendance before somebody was hired is a data-entry error, not a
        // month with nothing in it.
        if context.period < YearMonth::of(context.hired) {
            errors.push(
                "period",
                format!(
                    "{} is before this employee was hired in {}",
                    context.period,
                    YearMonth::of(context.hired)
                ),
            );
        }

        errors.into_result(ValidAttendance {
            days_worked,
            hours_worked,
            overtime,
            source: AttendanceSource::Manual,
        })
    }
}

fn time_field(errors: &mut ValidationErrors, minutes: i64, field: &'static str) -> WorkedTime {
    match WorkedTime::from_minutes(minutes) {
        Ok(time) => time,
        Err(error) => {
            errors.push(field, error.to_string());
            WorkedTime::ZERO
        }
    }
}

/// What a draft has to be checked against: which month, and when the person
/// started. Deliberately small, so validation does not need a whole `Employee`.
#[derive(Debug, Clone, Copy)]
pub struct AttendanceContext {
    pub period: YearMonth,
    pub hired: NaiveDate,
}

impl AttendanceContext {
    pub fn new(period: YearMonth, hired: NaiveDate) -> Self {
        AttendanceContext { period, hired }
    }
}

/// A draft that has passed validation. The repository accepts nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidAttendance {
    days_worked: WorkedDays,
    hours_worked: WorkedTime,
    overtime: WorkedTime,
    source: AttendanceSource,
}

impl ValidAttendance {
    /// What "Fill from standard schedule" produces for one employee.
    ///
    /// The month's working days from the project calendar, clipped to the part
    /// of the month the person was actually employed for, less approved leave,
    /// at the project's standard day length. Overtime is carried over rather
    /// than reset — it is the one number the calendar cannot know.
    pub fn from_standard_schedule(
        calendar: &WorkCalendar,
        holidays: &HolidaySet,
        period: YearMonth,
        hired: NaiveDate,
        leave: WorkedDays,
        keep_overtime: WorkedTime,
    ) -> Self {
        let month_start = period.first_day();
        let month_end = period.last_day();
        // Someone hired after the month ended worked none of it.
        let start = month_start.max(hired);

        let scheduled = if start > month_end {
            WorkedDays::ZERO
        } else {
            WorkedDays::from_days(
                calendar.working_days_between(start, month_end, holidays).unwrap_or(0),
            )
        };

        let days_worked = scheduled.saturating_sub(leave);
        ValidAttendance {
            days_worked,
            hours_worked: WorkedTime::for_days(days_worked, calendar.day_length.minutes()),
            overtime: keep_overtime,
            source: AttendanceSource::Schedule,
        }
    }

    pub fn days_worked(&self) -> WorkedDays {
        self.days_worked
    }

    pub fn hours_worked(&self) -> WorkedTime {
        self.hours_worked
    }

    pub fn overtime(&self) -> WorkedTime {
        self.overtime
    }

    pub fn source(&self) -> AttendanceSource {
        self.source
    }

    pub fn into_entry(
        self,
        id: AttendanceId,
        employee_id: EmployeeId,
        period: YearMonth,
        now: DateTime<Utc>,
    ) -> AttendanceEntry {
        AttendanceEntry {
            id,
            employee_id,
            period,
            days_worked: self.days_worked,
            hours_worked: self.hours_worked,
            overtime: self.overtime,
            source: self.source,
            created_at: now,
            updated_at: now,
        }
    }

    /// Replaces the numbers on an existing row, keeping identity and creation
    /// time — recording a month again overwrites it rather than adding to it.
    pub fn onto(self, existing: &AttendanceEntry, now: DateTime<Utc>) -> AttendanceEntry {
        AttendanceEntry {
            created_at: existing.created_at,
            ..self.into_entry(
                existing.id.clone(),
                existing.employee_id.clone(),
                existing.period,
                now,
            )
        }
    }
}

/// One line of the attendance grid. `entry` is absent for a month nobody has
/// recorded yet — which is different from a month recorded as zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceRow {
    pub employee_id: EmployeeId,
    pub entry: Option<AttendanceEntry>,
}

/// The totals under the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceTotals {
    pub days_worked: i32,
    pub hours_worked_minutes: i32,
    pub overtime_minutes: i32,
    /// How many people have a row, and how many are still blank.
    pub recorded: u32,
    pub missing: u32,
}

/// One project's grid for one month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceSheet {
    pub project_id: ProjectId,
    pub period: YearMonth,
    pub rows: Vec<AttendanceRow>,
    pub totals: AttendanceTotals,
}

impl AttendanceSheet {
    pub fn new(project_id: ProjectId, period: YearMonth, rows: Vec<AttendanceRow>) -> Self {
        let mut totals = AttendanceTotals::default();
        for row in &rows {
            match &row.entry {
                Some(entry) => {
                    totals.recorded += 1;
                    // Half-days summed as halves, then halved once at the end,
                    // so two half-days make one whole one.
                    totals.days_worked += entry.days_worked.halves();
                    totals.hours_worked_minutes += entry.hours_worked.minutes();
                    totals.overtime_minutes += entry.overtime.minutes();
                }
                None => totals.missing += 1,
            }
        }
        // `days_worked` is reported in halves for the same reason it is stored
        // that way: a total of 21.5 days must survive the trip.
        AttendanceSheet { project_id, period, rows, totals }
    }

    /// The grid's day total as a `WorkedDays`, halves and all.
    pub fn total_days(&self) -> WorkedDays {
        WorkedDays::from_halves(i64::from(self.totals.days_worked)).unwrap_or(WorkedDays::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::calendar::{DayLength, WeekdayMask};

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    fn month(year: i32, month: u32) -> YearMonth {
        YearMonth::new(year, month).expect("valid month")
    }

    fn context(period: YearMonth, hired: &str) -> AttendanceContext {
        AttendanceContext::new(period, date(hired))
    }

    mod units {
        use super::*;

        #[test]
        fn days_are_counted_in_halves_and_print_as_decimals() {
            assert_eq!(WorkedDays::from_days(21).to_string(), "21");
            assert_eq!(WorkedDays::from_halves(43).expect("valid").to_string(), "21.5");
            assert_eq!(WorkedDays::ZERO.to_string(), "0");
        }

        #[test]
        fn two_half_days_make_one_whole_day() {
            let half = WorkedDays::from_halves(1).expect("valid");
            let whole = half.checked_add(half).expect("no overflow");
            assert_eq!(whole, WorkedDays::from_days(1));
            assert_eq!(whole.to_string(), "1");
        }

        #[test]
        fn days_worked_cannot_be_negative() {
            assert_eq!(
                WorkedDays::from_halves(-1),
                Err(AttendanceError::Negative("days worked"))
            );
        }

        #[test]
        fn subtracting_more_leave_than_working_days_leaves_zero_not_a_debt() {
            let scheduled = WorkedDays::from_days(3);
            assert_eq!(scheduled.saturating_sub(WorkedDays::from_days(5)), WorkedDays::ZERO);
        }

        #[test]
        fn worked_time_holds_part_hours_exactly() {
            let time = WorkedTime::from_minutes(9_930).expect("valid");
            assert_eq!(time.split(), (165, 30));
            assert_eq!(time.to_string(), "165 h 30");
            assert_eq!(WorkedTime::from_hours(176).to_string(), "176 h");
        }

        #[test]
        fn worked_time_cannot_be_negative() {
            assert_eq!(
                WorkedTime::from_minutes(-1),
                Err(AttendanceError::Negative("worked time"))
            );
        }

        #[test]
        fn hours_for_a_half_day_are_half_the_day_length() {
            // 21.5 days at 7h30 is 161 h 15 exactly.
            let days = WorkedDays::from_halves(43).expect("valid");
            assert_eq!(WorkedTime::for_days(days, 450).minutes(), 9_675);
            assert_eq!(WorkedTime::for_days(days, 450).to_string(), "161 h 15");
        }

        #[test]
        fn sources_round_trip_through_their_stored_spelling() {
            for source in [AttendanceSource::Schedule, AttendanceSource::Manual] {
                assert_eq!(source.as_str().parse::<AttendanceSource>(), Ok(source));
            }
            assert!("guessed".parse::<AttendanceSource>().is_err());
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn a_plain_month_validates() {
            let valid = AttendanceDraft::of_days(22, 480)
                .validate(context(month(2026, 9), "2026-02-01"))
                .expect("22 eight-hour days is an ordinary month");

            assert_eq!(valid.days_worked(), WorkedDays::from_days(22));
            assert_eq!(valid.hours_worked(), WorkedTime::from_hours(176));
            assert_eq!(valid.overtime(), WorkedTime::ZERO);
            // Anything a person submits is a manual entry by definition.
            assert_eq!(valid.source(), AttendanceSource::Manual);
        }

        #[test]
        fn half_days_survive_validation() {
            let valid = AttendanceDraft::new(43, 9_675, 0)
                .validate(context(month(2026, 9), "2026-02-01"))
                .expect("21.5 days is allowed");
            assert_eq!(valid.days_worked().to_string(), "21.5");
        }

        #[test]
        fn days_cannot_exceed_the_days_in_that_month() {
            // September has 30 days, so 31 is impossible.
            let errors = AttendanceDraft::of_days(31, 480)
                .validate(context(month(2026, 9), "2026-02-01"))
                .expect_err("31 days in September");
            assert!(errors.has("daysWorked"));

            // …and 30 is fine, if unlikely.
            assert!(AttendanceDraft::of_days(30, 480)
                .validate(context(month(2026, 9), "2026-02-01"))
                .is_ok());
        }

        #[test]
        fn february_is_shorter_and_the_rule_knows_it() {
            assert!(AttendanceDraft::of_days(29, 480)
                .validate(context(month(2026, 2), "2026-01-01"))
                .is_err());
            assert!(AttendanceDraft::of_days(29, 480)
                .validate(context(month(2028, 2), "2026-01-01"))
                .is_ok(), "2028 is a leap year");
        }

        #[test]
        fn negative_numbers_are_rejected_field_by_field() {
            let errors = AttendanceDraft::new(-2, -60, -30)
                .validate(context(month(2026, 9), "2026-02-01"))
                .expect_err("nothing here can be negative");
            assert!(errors.has("daysWorked"));
            assert!(errors.has("hoursWorked"));
            assert!(errors.has("overtime"));
        }

        #[test]
        fn a_month_cannot_hold_more_than_twenty_four_hours_a_day() {
            // September: 30 × 24 = 720 hours. Ordinary and overtime together.
            let errors = AttendanceDraft::new(0, 700 * 60, 21 * 60)
                .validate(context(month(2026, 9), "2026-02-01"))
                .expect_err("721 hours in a 720-hour month");
            assert!(errors.has("hoursWorked"));

            assert!(AttendanceDraft::new(0, 700 * 60, 20 * 60)
                .validate(context(month(2026, 9), "2026-02-01"))
                .is_ok());
        }

        #[test]
        fn attendance_cannot_predate_the_hire_month() {
            let errors = AttendanceDraft::of_days(20, 480)
                .validate(context(month(2026, 1), "2026-02-01"))
                .expect_err("hired in February, recorded in January");
            assert!(errors.has("period"));

            // The hire month itself is fine — someone hired mid-month still
            // works part of it.
            assert!(AttendanceDraft::of_days(20, 480)
                .validate(context(month(2026, 2), "2026-02-15"))
                .is_ok());
        }

        #[test]
        fn every_problem_is_reported_at_once() {
            let errors = AttendanceDraft::new(-1, -1, -1)
                .validate(context(month(2026, 1), "2026-02-01"))
                .expect_err("four things are wrong");
            assert_eq!(errors.len(), 4);
        }
    }

    mod standard_schedule {
        use super::*;

        fn calendar() -> WorkCalendar {
            WorkCalendar::new(WeekdayMask::MON_FRI, DayLength::EIGHT_HOURS)
        }

        fn seed(
            period: YearMonth,
            hired: &str,
            holidays: &HolidaySet,
            leave: WorkedDays,
            overtime: WorkedTime,
        ) -> ValidAttendance {
            ValidAttendance::from_standard_schedule(
                &calendar(),
                holidays,
                period,
                date(hired),
                leave,
                overtime,
            )
        }

        #[test]
        fn a_full_month_is_the_calendars_working_days() {
            let filled = seed(
                month(2026, 9),
                "2020-01-06",
                &HolidaySet::new(),
                WorkedDays::ZERO,
                WorkedTime::ZERO,
            );
            // September 2026 has 22 weekdays.
            assert_eq!(filled.days_worked(), WorkedDays::from_days(22));
            assert_eq!(filled.hours_worked(), WorkedTime::from_hours(176));
            assert_eq!(filled.source(), AttendanceSource::Schedule);
        }

        #[test]
        fn holidays_come_off_the_seeded_days() {
            let holidays: HolidaySet =
                ["2026-09-07", "2026-09-08"].iter().map(|d| date(d)).collect();
            let filled =
                seed(month(2026, 9), "2020-01-06", &holidays, WorkedDays::ZERO, WorkedTime::ZERO);
            assert_eq!(filled.days_worked(), WorkedDays::from_days(20));
            assert_eq!(filled.hours_worked(), WorkedTime::from_hours(160));
        }

        #[test]
        fn someone_hired_mid_month_is_only_seeded_from_their_start() {
            // Hired Tuesday 2026-09-15; 2026-09-15..30 has 12 weekdays.
            let filled = seed(
                month(2026, 9),
                "2026-09-15",
                &HolidaySet::new(),
                WorkedDays::ZERO,
                WorkedTime::ZERO,
            );
            assert_eq!(filled.days_worked(), WorkedDays::from_days(12));
        }

        #[test]
        fn someone_hired_after_the_month_ended_worked_none_of_it() {
            let filled = seed(
                month(2026, 9),
                "2026-10-01",
                &HolidaySet::new(),
                WorkedDays::ZERO,
                WorkedTime::ZERO,
            );
            assert_eq!(filled.days_worked(), WorkedDays::ZERO);
            assert_eq!(filled.hours_worked(), WorkedTime::ZERO);
        }

        #[test]
        fn approved_leave_is_taken_off_the_seeded_days() {
            // The mockup's rule: filled days are working days less leave.
            let filled = seed(
                month(2026, 9),
                "2020-01-06",
                &HolidaySet::new(),
                WorkedDays::from_days(5),
                WorkedTime::ZERO,
            );
            assert_eq!(filled.days_worked(), WorkedDays::from_days(17));
            assert_eq!(filled.hours_worked(), WorkedTime::from_hours(136));
        }

        #[test]
        fn half_a_day_of_leave_leaves_half_a_day_of_work() {
            let filled = seed(
                month(2026, 9),
                "2020-01-06",
                &HolidaySet::new(),
                WorkedDays::from_halves(1).expect("half a day"),
                WorkedTime::ZERO,
            );
            assert_eq!(filled.days_worked().to_string(), "21.5");
            // 21.5 × 8 h = 172 h exactly.
            assert_eq!(filled.hours_worked(), WorkedTime::from_hours(172));
        }

        #[test]
        fn more_leave_than_working_days_seeds_zero() {
            let filled = seed(
                month(2026, 9),
                "2020-01-06",
                &HolidaySet::new(),
                WorkedDays::from_days(40),
                WorkedTime::ZERO,
            );
            assert_eq!(filled.days_worked(), WorkedDays::ZERO);
        }

        #[test]
        fn overtime_is_carried_over_because_the_calendar_cannot_know_it() {
            let overtime = WorkedTime::from_hours(9);
            let filled =
                seed(month(2026, 9), "2020-01-06", &HolidaySet::new(), WorkedDays::ZERO, overtime);
            assert_eq!(filled.overtime(), overtime);
        }

        #[test]
        fn a_six_day_project_at_seven_and_a_half_hours_seeds_its_own_numbers() {
            let calendar = WorkCalendar::new(
                WeekdayMask::MON_SAT,
                DayLength::from_hours_and_minutes(7, 30).expect("7h30"),
            );
            let filled = ValidAttendance::from_standard_schedule(
                &calendar,
                &HolidaySet::new(),
                month(2026, 9),
                date("2020-01-06"),
                WorkedDays::ZERO,
                WorkedTime::ZERO,
            );
            // 26 Mon–Sat days × 450 minutes = 195 h.
            assert_eq!(filled.days_worked(), WorkedDays::from_days(26));
            assert_eq!(filled.hours_worked().minutes(), 26 * 450);
            assert_eq!(filled.hours_worked().to_string(), "195 h");
        }
    }

    mod sheets {
        use super::*;

        fn entry(employee: &str, days_halves: i64, minutes: i64, overtime: i64) -> AttendanceEntry {
            let valid = AttendanceDraft::new(days_halves, minutes, overtime)
                .validate(context(month(2026, 9), "2020-01-06"))
                .expect("valid row");
            valid.into_entry(
                AttendanceId::new(),
                EmployeeId::from(employee),
                month(2026, 9),
                Utc::now(),
            )
        }

        #[test]
        fn totals_add_up_the_recorded_rows_and_count_the_blank_ones() {
            let sheet = AttendanceSheet::new(
                ProjectId::from("p1"),
                month(2026, 9),
                vec![
                    AttendanceRow {
                        employee_id: EmployeeId::from("e1"),
                        entry: Some(entry("e1", 44, 176 * 60, 3 * 60)),
                    },
                    AttendanceRow {
                        employee_id: EmployeeId::from("e2"),
                        entry: Some(entry("e2", 43, 172 * 60, 0)),
                    },
                    AttendanceRow { employee_id: EmployeeId::from("e3"), entry: None },
                ],
            );

            assert_eq!(sheet.totals.recorded, 2);
            assert_eq!(sheet.totals.missing, 1);
            // 22 + 21.5 = 43.5 days.
            assert_eq!(sheet.total_days().to_string(), "43.5");
            assert_eq!(sheet.totals.hours_worked_minutes, (176 + 172) * 60);
            assert_eq!(sheet.totals.overtime_minutes, 3 * 60);
        }

        #[test]
        fn two_half_days_across_two_people_total_one_whole_day() {
            let sheet = AttendanceSheet::new(
                ProjectId::from("p1"),
                month(2026, 9),
                vec![
                    AttendanceRow {
                        employee_id: EmployeeId::from("e1"),
                        entry: Some(entry("e1", 1, 240, 0)),
                    },
                    AttendanceRow {
                        employee_id: EmployeeId::from("e2"),
                        entry: Some(entry("e2", 1, 240, 0)),
                    },
                ],
            );
            assert_eq!(sheet.total_days().to_string(), "1");
        }

        #[test]
        fn an_empty_project_totals_zero_rather_than_failing() {
            let sheet = AttendanceSheet::new(ProjectId::from("p1"), month(2026, 9), Vec::new());
            assert_eq!(sheet.totals, AttendanceTotals::default());
            assert_eq!(sheet.total_days(), WorkedDays::ZERO);
        }

        #[test]
        fn a_month_recorded_as_zero_is_not_the_same_as_a_month_not_recorded() {
            let sheet = AttendanceSheet::new(
                ProjectId::from("p1"),
                month(2026, 9),
                vec![
                    AttendanceRow {
                        employee_id: EmployeeId::from("e1"),
                        entry: Some(entry("e1", 0, 0, 0)),
                    },
                    AttendanceRow { employee_id: EmployeeId::from("e2"), entry: None },
                ],
            );
            assert_eq!(sheet.totals.recorded, 1);
            assert_eq!(sheet.totals.missing, 1);
        }
    }
}
