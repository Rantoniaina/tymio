//! Contracts — pay basis, duration and leave entitlement.
//!
//! The rule that shapes this whole module: **a contract is never edited in
//! place.** A raise inserts a new effective-dated version and closes the
//! previous one; the old terms stay exactly as they were, so a payroll run
//! computed in March still reproduces in December after two raises.
//!
//! The design mockup has an "Edit contract" modal that mutates a single object
//! hanging off the employee. That is the prototype's data handling, not the
//! spec — the screens and the business rules are.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::prelude::*;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use super::employee::EmployeeId;
use super::{id_type, ValidationErrors};

/// Rates are held at four decimal places. Ariary is effectively a zero-decimal
/// currency, but `rate ÷ 26`, `÷ 173` and `÷ 8` all need the headroom.
pub const RATE_SCALE: u32 = 4;

/// Pay-basis conversions, exactly as the design mockup states them. They are
/// fixed conventions, not derived from the project's own calendar — a project
/// working 7½-hour days still converts a daily rate at ÷ 8.
pub const MONTHLY_DAYS: i64 = 26;
pub const MONTHLY_HOURS: i64 = 173;
pub const DAILY_HOURS: i64 = 8;

const MAX_WEEKLY_MINUTES: i64 = 7 * 24 * 60;
const MAX_PROBATION_MONTHS: i64 = 24;
/// Half-days, so 730 is a year.
const MAX_ANNUAL_GRANT_HALVES: i64 = 730;
const MAX_MONTHLY_ACCRUAL_HALVES: i64 = 62;

id_type! {
    /// Identifies one version of one employee's contract.
    ContractId
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error("a rate must be greater than zero")]
    RateNotPositive,
    #[error("a rate cannot be given to more than {RATE_SCALE} decimal places")]
    RateTooPrecise,
    #[error("that rate is too large to store")]
    RateOutOfRange,
    #[error("{0:?} is not a pay basis (expected monthly, daily or hourly)")]
    UnknownPayType(String),
    #[error("{0} cannot be negative")]
    Negative(&'static str),
    #[error("{what} cannot exceed {limit}")]
    TooLarge { what: &'static str, limit: i64 },
}

/// How the rate is read: per month, per day or per hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayType {
    #[default]
    Monthly,
    Daily,
    Hourly,
}

impl PayType {
    pub const ALL: [PayType; 3] = [PayType::Monthly, PayType::Daily, PayType::Hourly];

    pub fn as_str(self) -> &'static str {
        match self {
            PayType::Monthly => "monthly",
            PayType::Daily => "daily",
            PayType::Hourly => "hourly",
        }
    }
}

impl fmt::Display for PayType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PayType {
    type Err = ContractError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "monthly" => Ok(PayType::Monthly),
            "daily" => Ok(PayType::Daily),
            "hourly" => Ok(PayType::Hourly),
            other => Err(ContractError::UnknownPayType(other.to_owned())),
        }
    }
}

/// A pay rate, at scale 4 and always positive.
///
/// Serialised as a decimal string rather than a JSON number: `123456.7891`
/// does not survive an f64 round trip, and this is the salary path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rate(Decimal);

impl Rate {
    pub fn from_decimal(value: Decimal) -> Result<Self, ContractError> {
        if value <= Decimal::ZERO {
            return Err(ContractError::RateNotPositive);
        }
        if value.scale() > RATE_SCALE {
            return Err(ContractError::RateTooPrecise);
        }
        let mut scaled = value;
        scaled.rescale(RATE_SCALE);
        scaled.to_i64().ok_or(ContractError::RateOutOfRange)?;
        Ok(Rate(scaled))
    }

    /// Parses what a text field submits: `3200000`, `92 000`, `12.5`.
    pub fn parse(text: &str) -> Result<Self, ContractError> {
        let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let value = Decimal::from_str(&cleaned).map_err(|_| ContractError::RateNotPositive)?;
        Self::from_decimal(value)
    }

    /// How the rate is stored: the scale-4 value as a whole number.
    pub fn to_scaled(self) -> i64 {
        (self.0 * Decimal::from(10_i64.pow(RATE_SCALE)))
            .to_i64()
            .expect("a constructed rate always fits")
    }

    pub fn from_scaled(scaled: i64) -> Result<Self, ContractError> {
        Self::from_decimal(Decimal::new(scaled, RATE_SCALE))
    }

    pub fn as_decimal(self) -> Decimal {
        self.0
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Rate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Rate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rate::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Contracted hours a week, in whole minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct WeeklyHours(i32);

impl WeeklyHours {
    /// The mockup's default.
    pub const FORTY: Self = WeeklyHours(40 * 60);

    pub fn from_minutes(minutes: i64) -> Result<Self, ContractError> {
        if minutes <= 0 {
            return Err(ContractError::Negative("weekly hours"));
        }
        if minutes > MAX_WEEKLY_MINUTES {
            return Err(ContractError::TooLarge {
                what: "weekly hours",
                limit: MAX_WEEKLY_MINUTES,
            });
        }
        Ok(WeeklyHours(minutes as i32))
    }

    pub fn minutes(self) -> i32 {
        self.0
    }

    pub fn split(self) -> (i32, i32) {
        (self.0 / 60, self.0 % 60)
    }
}

impl Default for WeeklyHours {
    fn default() -> Self {
        Self::FORTY
    }
}

impl TryFrom<i64> for WeeklyHours {
    type Error = ContractError;

    fn try_from(minutes: i64) -> Result<Self, Self::Error> {
        Self::from_minutes(minutes)
    }
}

impl From<WeeklyHours> for i64 {
    fn from(hours: WeeklyHours) -> Self {
        i64::from(hours.0)
    }
}

impl fmt::Display for WeeklyHours {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.split() {
            (h, 0) => write!(f, "{h} h"),
            (h, m) => write!(f, "{h} h {m:02}"),
        }
    }
}

/// A leave entitlement in days, counted in half-days.
///
/// Separate from `attendance::WorkedDays` despite the same shape: a grant of
/// 30 days a year and a month's worked days have different bounds, and
/// conflating them would let one type's validation stand in for the other's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct LeaveDays(i32);

impl LeaveDays {
    pub const ZERO: Self = LeaveDays(0);

    pub fn from_halves(halves: i64) -> Result<Self, ContractError> {
        if halves < 0 {
            return Err(ContractError::Negative("leave days"));
        }
        i32::try_from(halves)
            .map(LeaveDays)
            .map_err(|_| ContractError::TooLarge { what: "leave days", limit: i64::from(i32::MAX) })
    }

    pub fn from_days(days: u32) -> Self {
        LeaveDays(days as i32 * 2)
    }

    pub fn halves(self) -> i32 {
        self.0
    }

    pub fn as_decimal(self) -> Decimal {
        Decimal::new(i64::from(self.0), 0) / Decimal::TWO
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for LeaveDays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 % 2 == 1 {
            write!(f, "{}.5", self.0 / 2)
        } else {
            write!(f, "{}", self.0 / 2)
        }
    }
}

impl TryFrom<i64> for LeaveDays {
    type Error = ContractError;

    fn try_from(halves: i64) -> Result<Self, Self::Error> {
        Self::from_halves(halves)
    }
}

impl From<LeaveDays> for i64 {
    fn from(days: LeaveDays) -> Self {
        i64::from(days.0)
    }
}

/// The terms themselves. Immutable once stored: an amendment is a new version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractTerms {
    pub pay_type: PayType,
    pub rate: Rate,
    /// The employment contract's own duration — a term, not the version window.
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub weekly_hours: WeeklyHours,
    pub probation_months: u8,
    /// Granted whole on 1 January. Zero if the contract has no fixed grant.
    pub annual_grant: LeaveDays,
    /// Added per month worked. Zero if the contract does not accrue.
    pub monthly_accrual: LeaveDays,
}

impl ContractTerms {
    /// The day rate this contract implies, unrounded.
    ///
    /// Deliberately not rounded: README says keep rates at scale 4 and round
    /// only at the final amount, so payroll rounds once, at the end.
    pub fn daily_equivalent(&self) -> Decimal {
        match self.pay_type {
            PayType::Monthly => self.rate.as_decimal() / Decimal::from(MONTHLY_DAYS),
            PayType::Daily => self.rate.as_decimal(),
            PayType::Hourly => self.rate.as_decimal() * Decimal::from(DAILY_HOURS),
        }
    }

    /// The hour rate this contract implies, unrounded.
    pub fn hourly_equivalent(&self) -> Decimal {
        match self.pay_type {
            PayType::Monthly => self.rate.as_decimal() / Decimal::from(MONTHLY_HOURS),
            PayType::Daily => self.rate.as_decimal() / Decimal::from(DAILY_HOURS),
            PayType::Hourly => self.rate.as_decimal(),
        }
    }

    /// What the mockup's contract preview shows: grant plus a full year of
    /// accrual.
    pub fn leave_days_per_year(&self) -> Decimal {
        self.annual_grant.as_decimal() + self.monthly_accrual.as_decimal() * Decimal::from(12)
    }

    pub fn accrues_leave(&self) -> bool {
        !self.annual_grant.is_zero() || !self.monthly_accrual.is_zero()
    }

    /// Whether probation still covers `date`.
    pub fn in_probation_on(&self, date: NaiveDate) -> bool {
        if self.probation_months == 0 {
            return false;
        }
        let ends = add_months(self.start, u32::from(self.probation_months));
        date >= self.start && date < ends
    }
}

/// `date` plus `months`, clamped to the end of the target month so that
/// 31 January plus one month is 28 February rather than an error.
fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    use chrono::Datelike;

    let zero_based = date.month0() + months;
    let year = date.year() + (zero_based / 12) as i32;
    let month = zero_based % 12 + 1;

    let last_day = super::calendar::YearMonth::new(year, month)
        .expect("month is derived modulo 12")
        .day_count();
    NaiveDate::from_ymd_opt(year, month, date.day().min(last_day))
        .expect("day is clamped to the month")
}

/// Rounds a computed amount to whole Ariary, half away from zero.
///
/// The one place rounding is allowed: at the final amount, never on the way.
pub fn round_ariary(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
}

/// One version of one employee's contract, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: ContractId,
    pub employee_id: EmployeeId,
    /// The first day these terms apply.
    pub valid_from: NaiveDate,
    /// The last day they apply, inclusive. `None` while still in force.
    pub valid_to: Option<NaiveDate>,
    pub terms: ContractTerms,
    pub created_at: DateTime<Utc>,
}

impl Contract {
    pub fn covers(&self, date: NaiveDate) -> bool {
        date >= self.valid_from && self.valid_to.is_none_or(|end| date <= end)
    }

    pub fn is_current(&self) -> bool {
        self.valid_to.is_none()
    }
}

/// What the contract form submits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDraft {
    /// The first day the new terms apply. For a first contract this is
    /// usually the hire date; for a raise it is the day it takes effect.
    pub effective_from: NaiveDate,
    pub pay_type: PayType,
    /// As typed: `3200000`, `92 000` and `12.5` are all accepted.
    pub rate: String,
    pub start: NaiveDate,
    #[serde(default)]
    pub end: Option<NaiveDate>,
    #[serde(default)]
    pub weekly_minutes: Option<i64>,
    #[serde(default)]
    pub probation_months: i64,
    /// Half-days, so 60 is a 30-day grant.
    #[serde(default)]
    pub annual_grant_halves: i64,
    #[serde(default)]
    pub monthly_accrual_halves: i64,
}

impl ContractDraft {
    /// A monthly contract on the mockup's defaults: 40 hours, three months'
    /// probation, no leave policy.
    pub fn monthly(rate: impl Into<String>, effective_from: NaiveDate) -> Self {
        ContractDraft {
            effective_from,
            pay_type: PayType::Monthly,
            rate: rate.into(),
            start: effective_from,
            end: None,
            weekly_minutes: None,
            probation_months: 3,
            annual_grant_halves: 0,
            monthly_accrual_halves: 0,
        }
    }

    /// Checks every rule and reports all the failures at once.
    pub fn validate(self, context: ContractContext) -> Result<ValidContract, ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let rate = match Rate::parse(&self.rate) {
            Ok(rate) => Some(rate),
            Err(error) => {
                errors.push("rate", error.to_string());
                None
            }
        };

        let weekly_hours = match self.weekly_minutes {
            None => Some(WeeklyHours::default()),
            Some(minutes) => match WeeklyHours::from_minutes(minutes) {
                Ok(hours) => Some(hours),
                Err(error) => {
                    errors.push("weeklyHours", error.to_string());
                    None
                }
            },
        };

        let probation_months = if self.probation_months < 0 {
            errors.push("probationMonths", "Probation cannot be negative");
            0
        } else if self.probation_months > MAX_PROBATION_MONTHS {
            errors.push(
                "probationMonths",
                format!("Probation cannot exceed {MAX_PROBATION_MONTHS} months"),
            );
            0
        } else {
            self.probation_months as u8
        };

        let annual_grant = leave_field(
            &mut errors,
            self.annual_grant_halves,
            "annualGrant",
            "An annual grant",
            MAX_ANNUAL_GRANT_HALVES,
        );
        let monthly_accrual = leave_field(
            &mut errors,
            self.monthly_accrual_halves,
            "monthlyAccrual",
            "A monthly accrual",
            MAX_MONTHLY_ACCRUAL_HALVES,
        );

        if self.end.is_some_and(|end| end < self.start) {
            errors.push("end", "A contract cannot end before it starts");
        }

        // The version window has to sit inside the employment relationship,
        // and after whatever it supersedes.
        if self.effective_from < context.hired {
            errors.push(
                "effectiveFrom",
                format!("Terms cannot take effect before the hire date, {}", context.hired),
            );
        }
        if let Some(current) = context.current_from {
            if self.effective_from <= current {
                errors.push(
                    "effectiveFrom",
                    format!("New terms must take effect after the current version, which began {current}"),
                );
            }
        }

        errors.into_result(ValidContract {
            effective_from: self.effective_from,
            terms: ContractTerms {
                pay_type: self.pay_type,
                rate: rate.unwrap_or(Rate(Decimal::ONE)),
                start: self.start,
                end: self.end,
                weekly_hours: weekly_hours.unwrap_or_default(),
                probation_months,
                annual_grant,
                monthly_accrual,
            },
        })
    }
}

fn leave_field(
    errors: &mut ValidationErrors,
    halves: i64,
    field: &'static str,
    label: &str,
    limit: i64,
) -> LeaveDays {
    match LeaveDays::from_halves(halves) {
        Err(error) => {
            errors.push(field, error.to_string());
            LeaveDays::ZERO
        }
        Ok(_) if halves > limit => {
            errors.push(field, format!("{label} cannot exceed {} days", limit / 2));
            LeaveDays::ZERO
        }
        Ok(days) => days,
    }
}

/// What a draft is checked against: when the person was hired, and what the
/// new version would be replacing.
#[derive(Debug, Clone, Copy)]
pub struct ContractContext {
    pub hired: NaiveDate,
    /// The start of the version currently in force, if there is one.
    pub current_from: Option<NaiveDate>,
}

impl ContractContext {
    /// The context for an employee's very first contract.
    pub fn first(hired: NaiveDate) -> Self {
        ContractContext { hired, current_from: None }
    }

    pub fn amending(hired: NaiveDate, current: &Contract) -> Self {
        ContractContext { hired, current_from: Some(current.valid_from) }
    }
}

/// A draft that has passed validation. The repository accepts nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidContract {
    effective_from: NaiveDate,
    terms: ContractTerms,
}

impl ValidContract {
    pub fn effective_from(&self) -> NaiveDate {
        self.effective_from
    }

    pub fn terms(&self) -> &ContractTerms {
        &self.terms
    }

    pub fn into_contract(
        self,
        id: ContractId,
        employee_id: EmployeeId,
        now: DateTime<Utc>,
    ) -> Contract {
        Contract {
            id,
            employee_id,
            valid_from: self.effective_from,
            valid_to: None,
            terms: self.terms,
            created_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).expect("test decimal is well formed")
    }

    /// Rakoto's contract from the mockup: 3 200 000 MGA a month.
    fn rakoto() -> ContractDraft {
        let mut draft = ContractDraft::monthly("3200000", date("2026-02-01"));
        draft.end = Some(date("2027-06-30"));
        draft.annual_grant_halves = 60;
        draft
    }

    fn context() -> ContractContext {
        ContractContext::first(date("2026-02-01"))
    }

    fn terms(draft: ContractDraft) -> ContractTerms {
        *draft.validate(context()).expect("valid draft").terms()
    }

    mod rates {
        use super::*;

        #[test]
        fn a_rate_round_trips_through_its_stored_integer() {
            let rate = Rate::parse("3200000").expect("valid rate");
            assert_eq!(rate.to_scaled(), 32_000_000_000);
            assert_eq!(Rate::from_scaled(32_000_000_000), Ok(rate));
            assert_eq!(rate.as_decimal(), dec("3200000.0000"));
        }

        #[test]
        fn a_rate_keeps_four_decimal_places() {
            let rate = Rate::parse("12.5").expect("valid rate");
            assert_eq!(rate.to_string(), "12.5000");
            assert_eq!(Rate::parse("0.0001").expect("valid").to_scaled(), 1);
        }

        #[test]
        fn a_rate_accepts_the_spacing_ariary_is_written_with() {
            assert_eq!(Rate::parse("3 200 000"), Rate::parse("3200000"));
        }

        #[test]
        fn a_rate_must_be_positive() {
            assert_eq!(Rate::parse("0"), Err(ContractError::RateNotPositive));
            assert_eq!(Rate::parse("-1"), Err(ContractError::RateNotPositive));
            assert_eq!(Rate::parse("nonsense"), Err(ContractError::RateNotPositive));
        }

        #[test]
        fn a_rate_beyond_four_decimals_is_refused_rather_than_rounded() {
            // Silently rounding somebody's rate is how payslips go wrong.
            assert_eq!(Rate::parse("12.00001"), Err(ContractError::RateTooPrecise));
        }

        #[test]
        fn a_rate_survives_serialisation_as_a_string_not_a_float() {
            let rate = Rate::parse("123456.7891").expect("valid rate");
            let json = serde_json::to_string(&rate).expect("serialisable");
            assert_eq!(json, r#""123456.7891""#);
            assert_eq!(serde_json::from_str::<Rate>(&json).expect("round trip"), rate);
        }
    }

    mod pay_basis {
        use super::*;

        fn at(pay_type: PayType, rate: &str) -> ContractTerms {
            let mut draft = ContractDraft::monthly(rate, date("2026-02-01"));
            draft.pay_type = pay_type;
            terms(draft)
        }

        #[test]
        fn a_monthly_rate_converts_by_twenty_six_and_a_hundred_and_seventy_three() {
            let monthly = at(PayType::Monthly, "3200000");
            assert_eq!(round_ariary(monthly.daily_equivalent()), dec("123077"));
            assert_eq!(round_ariary(monthly.hourly_equivalent()), dec("18497"));
        }

        #[test]
        fn a_daily_rate_is_its_own_day_equivalent_and_divides_by_eight_for_hours() {
            let daily = at(PayType::Daily, "92000");
            assert_eq!(daily.daily_equivalent(), dec("92000.0000"));
            assert_eq!(daily.hourly_equivalent(), dec("11500"));
        }

        #[test]
        fn an_hourly_rate_multiplies_by_eight_for_a_day() {
            let hourly = at(PayType::Hourly, "14500");
            assert_eq!(hourly.daily_equivalent(), dec("116000"));
            assert_eq!(hourly.hourly_equivalent(), dec("14500.0000"));
        }

        #[test]
        fn conversions_are_not_rounded_on_the_way() {
            // 3 200 000 ÷ 26 is 123076.923…, and payroll must round once at
            // the end rather than here.
            let monthly = at(PayType::Monthly, "3200000");
            assert_ne!(monthly.daily_equivalent(), dec("123077"));
            assert!(monthly.daily_equivalent() > dec("123076.9"));
            assert!(monthly.daily_equivalent() < dec("123077"));
        }

        #[test]
        fn rounding_to_ariary_goes_half_away_from_zero() {
            assert_eq!(round_ariary(dec("0.5")), dec("1"));
            assert_eq!(round_ariary(dec("1.5")), dec("2"));
            assert_eq!(round_ariary(dec("2.5")), dec("3"));
            assert_eq!(round_ariary(dec("2.4")), dec("2"));
        }
    }

    mod leave_policy {
        use super::*;

        #[test]
        fn a_contract_may_have_a_grant_an_accrual_both_or_neither() {
            let mut none = ContractDraft::monthly("2000000", date("2026-02-01"));
            none.annual_grant_halves = 0;
            none.monthly_accrual_halves = 0;
            assert!(!terms(none).accrues_leave());

            let mut grant = ContractDraft::monthly("2000000", date("2026-02-01"));
            grant.annual_grant_halves = 60;
            assert_eq!(terms(grant).leave_days_per_year(), dec("30"));

            let mut accrual = ContractDraft::monthly("2000000", date("2026-02-01"));
            accrual.monthly_accrual_halves = 5;
            assert_eq!(terms(accrual).leave_days_per_year(), dec("30"));

            let mut both = ContractDraft::monthly("2000000", date("2026-02-01"));
            both.annual_grant_halves = 24;
            both.monthly_accrual_halves = 2;
            let both = terms(both);
            assert_eq!(both.leave_days_per_year(), dec("24"));
            assert!(both.accrues_leave());
        }

        #[test]
        fn half_day_accruals_are_exact() {
            // The mockup's 2.5 days a month.
            let mut draft = ContractDraft::monthly("2450000", date("2026-02-15"));
            draft.monthly_accrual_halves = 5;
            let policy = terms(draft);
            assert_eq!(policy.monthly_accrual.to_string(), "2.5");
            assert_eq!(policy.leave_days_per_year(), dec("30"));
        }
    }

    mod probation {
        use super::*;

        #[test]
        fn probation_runs_from_the_contract_start_for_its_months() {
            let policy = terms(rakoto()); // starts 2026-02-01, three months
            assert!(policy.in_probation_on(date("2026-02-01")));
            assert!(policy.in_probation_on(date("2026-04-30")));
            assert!(!policy.in_probation_on(date("2026-05-01")));
            assert!(!policy.in_probation_on(date("2026-01-31")));
        }

        #[test]
        fn no_probation_means_never_in_probation() {
            let mut draft = rakoto();
            draft.probation_months = 0;
            assert!(!terms(draft).in_probation_on(date("2026-02-02")));
        }

        #[test]
        fn a_probation_from_the_end_of_a_long_month_lands_inside_a_short_one() {
            // 31 December plus two months is 28 February, not an error.
            let mut draft = ContractDraft::monthly("2000000", date("2025-12-31"));
            draft.probation_months = 2;
            let policy = *draft
                .validate(ContractContext::first(date("2025-12-31")))
                .expect("valid draft")
                .terms();
            assert!(policy.in_probation_on(date("2026-02-27")));
            assert!(!policy.in_probation_on(date("2026-02-28")));
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn the_mockup_defaults_validate() {
            let valid = rakoto().validate(context()).expect("a plain monthly contract");
            assert_eq!(valid.effective_from(), date("2026-02-01"));
            assert_eq!(valid.terms().pay_type, PayType::Monthly);
            assert_eq!(valid.terms().weekly_hours, WeeklyHours::FORTY);
            assert_eq!(valid.terms().probation_months, 3);
            assert_eq!(valid.terms().annual_grant, LeaveDays::from_days(30));
        }

        #[test]
        fn weekly_hours_have_to_fit_in_a_week() {
            let mut draft = rakoto();
            draft.weekly_minutes = Some(0);
            assert!(draft.clone().validate(context()).expect_err("zero").has("weeklyHours"));

            draft.weekly_minutes = Some(8 * 24 * 60);
            assert!(draft.validate(context()).expect_err("more than a week").has("weeklyHours"));
        }

        #[test]
        fn probation_is_bounded() {
            let mut draft = rakoto();
            draft.probation_months = -1;
            assert!(draft.clone().validate(context()).expect_err("negative").has("probationMonths"));

            draft.probation_months = 25;
            assert!(draft.validate(context()).expect_err("two years").has("probationMonths"));
        }

        #[test]
        fn a_leave_grant_cannot_exceed_a_year() {
            let mut draft = rakoto();
            draft.annual_grant_halves = 732;
            assert!(draft.validate(context()).expect_err("more than a year").has("annualGrant"));
        }

        #[test]
        fn a_monthly_accrual_cannot_exceed_a_month() {
            let mut draft = rakoto();
            draft.monthly_accrual_halves = 64;
            assert!(draft.validate(context()).expect_err("more than a month").has("monthlyAccrual"));
        }

        #[test]
        fn a_contract_cannot_end_before_it_starts() {
            let mut draft = rakoto();
            draft.start = date("2026-02-01");
            draft.end = Some(date("2026-01-01"));
            assert!(draft.validate(context()).expect_err("backwards").has("end"));
        }

        #[test]
        fn terms_cannot_take_effect_before_the_hire_date() {
            let mut draft = rakoto();
            draft.effective_from = date("2026-01-01");
            let errors = draft
                .validate(ContractContext::first(date("2026-02-01")))
                .expect_err("before being hired");
            assert!(errors.has("effectiveFrom"));
        }

        #[test]
        fn a_new_version_must_take_effect_after_the_one_it_replaces() {
            let current = rakoto()
                .validate(context())
                .expect("valid")
                .into_contract(ContractId::new(), EmployeeId::from("e1"), Utc::now());
            let amending = ContractContext::amending(date("2026-02-01"), &current);

            let mut same_day = rakoto();
            same_day.effective_from = current.valid_from;
            assert!(same_day
                .validate(amending)
                .expect_err("same day as the version it replaces")
                .has("effectiveFrom"));

            let mut earlier = rakoto();
            earlier.effective_from = date("2026-01-15");
            assert!(earlier.validate(amending).expect_err("before it").has("effectiveFrom"));

            let mut later = rakoto();
            later.effective_from = date("2026-06-01");
            assert!(later.validate(amending).is_ok());
        }

        #[test]
        fn every_problem_is_reported_at_once() {
            let mut draft = ContractDraft::monthly("0", date("2020-01-01"));
            draft.weekly_minutes = Some(-1);
            draft.probation_months = 99;
            draft.annual_grant_halves = 1000;
            draft.start = date("2026-02-01");
            draft.end = Some(date("2025-01-01"));

            let errors = draft
                .validate(ContractContext::first(date("2026-02-01")))
                .expect_err("six things are wrong");
            for field in
                ["rate", "weeklyHours", "probationMonths", "annualGrant", "end", "effectiveFrom"]
            {
                assert!(errors.has(field), "expected an error on {field}");
            }
            assert_eq!(errors.len(), 6);
        }
    }

    mod versions {
        use super::*;

        fn version(from: &str, to: Option<&str>) -> Contract {
            Contract {
                id: ContractId::new(),
                employee_id: EmployeeId::from("e1"),
                valid_from: date(from),
                valid_to: to.map(date),
                terms: terms(rakoto()),
                created_at: Utc::now(),
            }
        }

        #[test]
        fn a_closed_version_covers_its_window_inclusively() {
            let v = version("2026-02-01", Some("2026-05-31"));
            assert!(!v.covers(date("2026-01-31")));
            assert!(v.covers(date("2026-02-01")));
            assert!(v.covers(date("2026-05-31")));
            assert!(!v.covers(date("2026-06-01")));
            assert!(!v.is_current());
        }

        #[test]
        fn an_open_version_covers_everything_from_its_start() {
            let v = version("2026-06-01", None);
            assert!(!v.covers(date("2026-05-31")));
            assert!(v.covers(date("2026-06-01")));
            assert!(v.covers(date("2099-01-01")));
            assert!(v.is_current());
        }
    }

    mod units {
        use super::*;

        #[test]
        fn weekly_hours_print_the_way_the_employee_file_shows_them() {
            assert_eq!(WeeklyHours::FORTY.to_string(), "40 h");
            assert_eq!(
                WeeklyHours::from_minutes(37 * 60 + 30).expect("valid").to_string(),
                "37 h 30"
            );
        }

        #[test]
        fn leave_days_print_as_decimals_and_convert_exactly() {
            assert_eq!(LeaveDays::from_days(30).to_string(), "30");
            assert_eq!(LeaveDays::from_halves(5).expect("valid").to_string(), "2.5");
            assert_eq!(LeaveDays::from_halves(5).expect("valid").as_decimal(), dec("2.5"));
            assert!(LeaveDays::ZERO.is_zero());
        }

        #[test]
        fn pay_types_round_trip_through_their_stored_spelling() {
            for pay_type in PayType::ALL {
                assert_eq!(pay_type.as_str().parse::<PayType>(), Ok(pay_type));
            }
            assert!("weekly".parse::<PayType>().is_err());
        }
    }
}
