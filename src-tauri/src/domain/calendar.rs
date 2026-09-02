//! The project work calendar: which weekdays are worked, how long a standard
//! day is, and which dates are holidays.
//!
//! Payroll derives worked days from this (README: "Worked days for a month =
//! working days in the calendar − leave − absences"), so every count here has
//! to be exact and reproducible — no floats, no timezone-sensitive types.

use std::collections::BTreeSet;
use std::fmt;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Things a calendar value can be wrong about.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalendarError {
    #[error("a project must work at least one day a week")]
    NoWorkingDays,
    #[error("weekday mask {0} is out of range (1..=127)")]
    MaskOutOfRange(u8),
    #[error("a standard day must be between 1 minute and 24 hours, got {0} minutes")]
    DayLengthOutOfRange(i64),
    #[error("{0} is not a month (expected 1..=12)")]
    MonthOutOfRange(u32),
    #[error("expected a month as YYYY-MM, got {0:?}")]
    MalformedYearMonth(String),
    #[error("range ends ({end}) before it starts ({start})")]
    ReversedRange { start: NaiveDate, end: NaiveDate },
}

/// The set of weekdays a project treats as working days, as a bitmask.
///
/// Bit 0 is Monday through bit 6 for Sunday, which is also how it is stored —
/// one small integer column instead of seven booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct WeekdayMask(u8);

impl WeekdayMask {
    /// Monday to Friday — the default for a new project.
    pub const MON_FRI: Self = WeekdayMask(0b0001_1111);
    /// Monday to Saturday, common on construction sites.
    pub const MON_SAT: Self = WeekdayMask(0b0011_1111);

    pub fn from_bits(bits: u8) -> Result<Self, CalendarError> {
        if bits == 0 {
            return Err(CalendarError::NoWorkingDays);
        }
        if bits > 0b0111_1111 {
            return Err(CalendarError::MaskOutOfRange(bits));
        }
        Ok(WeekdayMask(bits))
    }

    pub fn from_weekdays(days: &[Weekday]) -> Result<Self, CalendarError> {
        let bits = days
            .iter()
            .fold(0u8, |acc, d| acc | (1 << d.num_days_from_monday()));
        Self::from_bits(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.num_days_from_monday()) != 0
    }

    /// How many days a full week has under this mask.
    pub fn days_per_week(self) -> u32 {
        self.0.count_ones()
    }

    pub fn weekdays(self) -> Vec<Weekday> {
        [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .filter(|d| self.contains(*d))
        .collect()
    }
}

impl Default for WeekdayMask {
    fn default() -> Self {
        Self::MON_FRI
    }
}

impl TryFrom<u8> for WeekdayMask {
    type Error = CalendarError;

    fn try_from(bits: u8) -> Result<Self, Self::Error> {
        Self::from_bits(bits)
    }
}

impl From<WeekdayMask> for u8 {
    fn from(mask: WeekdayMask) -> Self {
        mask.0
    }
}

/// The length of a standard working day, held in whole minutes.
///
/// Minutes rather than hours because 7.5 h and 8 h 20 both have to be exact:
/// a float here would drift straight into the payroll numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct DayLength(u16);

impl DayLength {
    pub const EIGHT_HOURS: Self = DayLength(8 * 60);

    pub fn from_minutes(minutes: i64) -> Result<Self, CalendarError> {
        if !(1..=24 * 60).contains(&minutes) {
            return Err(CalendarError::DayLengthOutOfRange(minutes));
        }
        Ok(DayLength(minutes as u16))
    }

    pub fn from_hours_and_minutes(hours: u8, minutes: u8) -> Result<Self, CalendarError> {
        Self::from_minutes(i64::from(hours) * 60 + i64::from(minutes))
    }

    pub fn minutes(self) -> u32 {
        u32::from(self.0)
    }

    /// The whole-hours and leftover-minutes split, for display.
    pub fn split(self) -> (u32, u32) {
        (self.minutes() / 60, self.minutes() % 60)
    }
}

impl Default for DayLength {
    fn default() -> Self {
        Self::EIGHT_HOURS
    }
}

impl TryFrom<i64> for DayLength {
    type Error = CalendarError;

    fn try_from(minutes: i64) -> Result<Self, Self::Error> {
        Self::from_minutes(minutes)
    }
}

impl From<DayLength> for i64 {
    fn from(len: DayLength) -> Self {
        i64::from(len.0)
    }
}

impl fmt::Display for DayLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.split() {
            (h, 0) => write!(f, "{h} h"),
            (h, m) => write!(f, "{h} h {m:02}"),
        }
    }
}

/// A calendar month. Payroll periods, attendance and leave are all monthly,
/// and a month is not a date — giving it a type stops the two being confused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct YearMonth {
    pub year: i32,
    pub month: u32,
}

impl YearMonth {
    pub fn new(year: i32, month: u32) -> Result<Self, CalendarError> {
        if !(1..=12).contains(&month) {
            return Err(CalendarError::MonthOutOfRange(month));
        }
        Ok(YearMonth { year, month })
    }

    pub fn of(date: NaiveDate) -> Self {
        YearMonth {
            year: date.year(),
            month: date.month(),
        }
    }

    pub fn parse(s: &str) -> Result<Self, CalendarError> {
        let malformed = || CalendarError::MalformedYearMonth(s.to_owned());
        let (year, month) = s.split_once('-').ok_or_else(malformed)?;
        if month.len() != 2 {
            return Err(malformed());
        }
        let year: i32 = year.parse().map_err(|_| malformed())?;
        let month: u32 = month.parse().map_err(|_| malformed())?;
        Self::new(year, month)
    }

    pub fn first_day(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month, 1).expect("month is validated on construction")
    }

    pub fn last_day(self) -> NaiveDate {
        self.next().first_day().pred_opt().expect("no month starts at the minimum date")
    }

    pub fn next(self) -> Self {
        if self.month == 12 {
            YearMonth { year: self.year + 1, month: 1 }
        } else {
            YearMonth { year: self.year, month: self.month + 1 }
        }
    }

    pub fn days(self) -> impl Iterator<Item = NaiveDate> {
        let last = self.last_day();
        std::iter::successors(Some(self.first_day()), move |d| {
            d.succ_opt().filter(|next| *next <= last)
        })
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

/// The project's holidays, as a set of dates.
///
/// A holiday on a day the project does not work changes nothing — it is the
/// intersection with the weekday mask that matters, which is why this is a
/// plain set and not a count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolidaySet(BTreeSet<NaiveDate>);

impl HolidaySet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.0.contains(&date)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn dates(&self) -> impl Iterator<Item = NaiveDate> + '_ {
        self.0.iter().copied()
    }
}

impl FromIterator<NaiveDate> for HolidaySet {
    fn from_iter<T: IntoIterator<Item = NaiveDate>>(iter: T) -> Self {
        HolidaySet(iter.into_iter().collect())
    }
}

/// Which days the project works, and for how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkCalendar {
    pub working_days: WeekdayMask,
    pub day_length: DayLength,
}

impl WorkCalendar {
    pub fn new(working_days: WeekdayMask, day_length: DayLength) -> Self {
        WorkCalendar { working_days, day_length }
    }

    /// A working day is one the mask includes and no holiday falls on.
    pub fn is_working_day(&self, date: NaiveDate, holidays: &HolidaySet) -> bool {
        self.working_days.contains(date.weekday()) && !holidays.contains(date)
    }

    /// Working days in `start..=end`, both ends included.
    pub fn working_days_between(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        holidays: &HolidaySet,
    ) -> Result<u32, CalendarError> {
        if end < start {
            return Err(CalendarError::ReversedRange { start, end });
        }
        let mut count = 0;
        let mut day = start;
        loop {
            if self.is_working_day(day, holidays) {
                count += 1;
            }
            if day == end {
                break;
            }
            day = day.succ_opt().expect("end bounds the walk below the maximum date");
        }
        Ok(count)
    }

    pub fn working_days_in_month(&self, month: YearMonth, holidays: &HolidaySet) -> u32 {
        month.days().filter(|d| self.is_working_day(*d, holidays)).count() as u32
    }

    /// What "Fill from standard schedule" puts in the attendance grid: the
    /// month's working days at the standard day length, in whole minutes.
    pub fn working_minutes_in_month(&self, month: YearMonth, holidays: &HolidaySet) -> u64 {
        u64::from(self.working_days_in_month(month, holidays)) * u64::from(self.day_length.minutes())
    }
}

impl Default for WorkCalendar {
    fn default() -> Self {
        WorkCalendar::new(WeekdayMask::default(), DayLength::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    fn holidays(dates: &[&str]) -> HolidaySet {
        dates.iter().map(|s| date(s)).collect()
    }

    #[test]
    fn mon_fri_mask_covers_weekdays_only() {
        let mask = WeekdayMask::MON_FRI;
        assert!(mask.contains(Weekday::Mon));
        assert!(mask.contains(Weekday::Fri));
        assert!(!mask.contains(Weekday::Sat));
        assert!(!mask.contains(Weekday::Sun));
        assert_eq!(mask.days_per_week(), 5);
    }

    #[test]
    fn mask_round_trips_through_weekdays_and_bits() {
        let days = [Weekday::Mon, Weekday::Wed, Weekday::Sat];
        let mask = WeekdayMask::from_weekdays(&days).expect("three days is a valid mask");
        assert_eq!(mask.weekdays(), days);
        assert_eq!(WeekdayMask::from_bits(mask.bits()), Ok(mask));
    }

    #[test]
    fn a_project_cannot_work_zero_days_a_week() {
        assert_eq!(WeekdayMask::from_weekdays(&[]), Err(CalendarError::NoWorkingDays));
        assert_eq!(WeekdayMask::from_bits(0), Err(CalendarError::NoWorkingDays));
    }

    #[test]
    fn mask_rejects_bits_above_sunday() {
        assert_eq!(WeekdayMask::from_bits(0b1000_0000), Err(CalendarError::MaskOutOfRange(128)));
        assert_eq!(WeekdayMask::from_bits(0b0111_1111).map(|m| m.days_per_week()), Ok(7));
    }

    #[test]
    fn day_length_holds_half_hours_exactly() {
        let seven_thirty = DayLength::from_hours_and_minutes(7, 30).expect("7h30 is a valid day");
        assert_eq!(seven_thirty.minutes(), 450);
        assert_eq!(seven_thirty.split(), (7, 30));
        assert_eq!(seven_thirty.to_string(), "7 h 30");
        assert_eq!(DayLength::EIGHT_HOURS.to_string(), "8 h");
    }

    #[test]
    fn day_length_rejects_zero_and_more_than_a_day() {
        assert_eq!(DayLength::from_minutes(0), Err(CalendarError::DayLengthOutOfRange(0)));
        assert_eq!(DayLength::from_minutes(-30), Err(CalendarError::DayLengthOutOfRange(-30)));
        assert_eq!(DayLength::from_minutes(1441), Err(CalendarError::DayLengthOutOfRange(1441)));
        assert!(DayLength::from_minutes(1440).is_ok());
    }

    #[test]
    fn year_month_parses_and_prints_the_mockup_format() {
        let september = YearMonth::parse("2026-09").expect("well-formed month");
        assert_eq!(september, YearMonth { year: 2026, month: 9 });
        assert_eq!(september.to_string(), "2026-09");
    }

    #[test]
    fn year_month_rejects_junk() {
        assert!(YearMonth::parse("2026").is_err());
        assert!(YearMonth::parse("2026-9").is_err());
        assert!(YearMonth::parse("2026-13").is_err());
        assert!(YearMonth::parse("2026-00").is_err());
        assert!(YearMonth::parse("september").is_err());
    }

    #[test]
    fn year_month_bounds_handle_february_and_december() {
        let feb_leap = YearMonth::new(2028, 2).expect("february");
        assert_eq!(feb_leap.last_day(), date("2028-02-29"));
        let feb_common = YearMonth::new(2026, 2).expect("february");
        assert_eq!(feb_common.last_day(), date("2026-02-28"));
        assert_eq!(feb_common.days().count(), 28);

        let december = YearMonth::new(2026, 12).expect("december");
        assert_eq!(december.next(), YearMonth { year: 2027, month: 1 });
        assert_eq!(december.last_day(), date("2026-12-31"));
    }

    #[test]
    fn working_days_in_a_plain_month() {
        let calendar = WorkCalendar::default();
        // September 2026 starts on a Tuesday and has 22 weekdays.
        let september = YearMonth::new(2026, 9).expect("september");
        assert_eq!(calendar.working_days_in_month(september, &HolidaySet::new()), 22);
    }

    #[test]
    fn holidays_on_working_days_reduce_the_count() {
        let calendar = WorkCalendar::default();
        let november = YearMonth::new(2026, 11).expect("november");
        let base = calendar.working_days_in_month(november, &HolidaySet::new());

        // 2026-11-02 is a Monday, 2026-11-03 a Tuesday.
        let off = holidays(&["2026-11-02", "2026-11-03"]);
        assert_eq!(calendar.working_days_in_month(november, &off), base - 2);
    }

    #[test]
    fn a_holiday_on_a_non_working_day_changes_nothing() {
        let calendar = WorkCalendar::default();
        let november = YearMonth::new(2026, 11).expect("november");
        let base = calendar.working_days_in_month(november, &HolidaySet::new());

        // 2026-11-07 is a Saturday, already not worked under Mon–Fri.
        let off = holidays(&["2026-11-07"]);
        assert_eq!(calendar.working_days_in_month(november, &off), base);

        // …but a six-day project does lose it.
        let six_day = WorkCalendar::new(WeekdayMask::MON_SAT, DayLength::EIGHT_HOURS);
        let six_day_base = six_day.working_days_in_month(november, &HolidaySet::new());
        assert_eq!(six_day.working_days_in_month(november, &off), six_day_base - 1);
    }

    #[test]
    fn working_days_between_includes_both_ends() {
        let calendar = WorkCalendar::default();
        // Monday 2026-09-07 to Friday 2026-09-11.
        let count = calendar
            .working_days_between(date("2026-09-07"), date("2026-09-11"), &HolidaySet::new())
            .expect("forward range");
        assert_eq!(count, 5);

        // A single working day is one day, not zero.
        let one = calendar
            .working_days_between(date("2026-09-07"), date("2026-09-07"), &HolidaySet::new())
            .expect("single-day range");
        assert_eq!(one, 1);

        // A single non-working day is zero.
        let none = calendar
            .working_days_between(date("2026-09-12"), date("2026-09-13"), &HolidaySet::new())
            .expect("weekend range");
        assert_eq!(none, 0);
    }

    #[test]
    fn working_days_between_spans_month_and_year_boundaries() {
        let calendar = WorkCalendar::default();
        let across_new_year = calendar
            .working_days_between(date("2026-12-28"), date("2027-01-08"), &holidays(&["2027-01-01"]))
            .expect("forward range");
        // 28–31 Dec (Mon–Thu) + 1 Jan is a holiday + 4–8 Jan (Mon–Fri).
        assert_eq!(across_new_year, 9);
    }

    #[test]
    fn working_days_between_rejects_a_reversed_range() {
        let calendar = WorkCalendar::default();
        let start = date("2026-09-11");
        let end = date("2026-09-07");
        assert_eq!(
            calendar.working_days_between(start, end, &HolidaySet::new()),
            Err(CalendarError::ReversedRange { start, end })
        );
    }

    #[test]
    fn standard_schedule_hours_come_out_in_whole_minutes() {
        let calendar = WorkCalendar::new(
            WeekdayMask::MON_FRI,
            DayLength::from_hours_and_minutes(7, 30).expect("7h30"),
        );
        let september = YearMonth::new(2026, 9).expect("september");
        // 22 working days × 450 minutes = 9 900 minutes = 165 h exactly.
        assert_eq!(calendar.working_minutes_in_month(september, &HolidaySet::new()), 9_900);
    }
}
