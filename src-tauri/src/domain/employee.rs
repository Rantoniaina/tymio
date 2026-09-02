//! Employees — the people inside a project.
//!
//! What is *not* here is as deliberate as what is. Pay type, rate, weekly
//! hours, probation and leave policy all belong to a contract, which is an
//! effective-dated version in its own table; the mockup hangs a single mutable
//! contract object off the employee, and following it would make an old
//! payslip change when someone gets a raise.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::calendar::YearMonth;
use super::project::ProjectId;
use super::{id_type, normalise_optional, ValidationErrors};

pub const MAX_NAME_LEN: usize = 120;
pub const MAX_ADDRESS_LEN: usize = 240;
pub const MAX_CONTACT_LEN: usize = 60;
/// A CIN with the spaces taken out. Malagasy numbers are twelve digits; the
/// range is wide because the design mockup uses nine and historical cards
/// vary.
pub const CIN_DIGITS: std::ops::RangeInclusive<usize> = 6..=20;

id_type! {
    /// Identifies an employee. Opaque — do not parse it, do not sort by it.
    EmployeeId
}

/// An employee as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: EmployeeId,
    /// Set once, at creation. A transfer between projects is a different
    /// operation from an edit, and nothing in the design asks for one yet.
    pub project_id: ProjectId,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub cin: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub hire_date: NaiveDate,
    pub bank_account: Option<String>,
    pub emergency_contact: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Employee {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }

    /// The two-letter monogram the mockup puts in the avatar circle.
    pub fn initials(&self) -> String {
        let letter = |name: &str| {
            name.chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
        };
        format!("{}{}", letter(&self.first_name), letter(&self.last_name))
    }

    /// Age in whole years on `date`, or `None` when no birth date is recorded.
    pub fn age_on(&self, date: NaiveDate) -> Option<u32> {
        self.birth_date.map(|birth| whole_years_between(birth, date))
    }

    /// Whole months of service on `date`. Zero before the hire date.
    pub fn months_of_service_at(&self, date: NaiveDate) -> u32 {
        whole_months_between(self.hire_date, date)
    }

    /// Months worked within the calendar year of `period`, counting `period`
    /// itself.
    ///
    /// This is the mockup's accrual driver: someone hired in a previous year
    /// counts from January, someone hired during the year counts from their
    /// hire month. The leave slice multiplies this by the contract's
    /// days-per-month to get an accrued balance.
    pub fn months_worked_in(&self, period: YearMonth) -> u32 {
        if self.hire_date.year() > period.year {
            return 0;
        }
        let first = if self.hire_date.year() < period.year {
            1
        } else {
            self.hire_date.month()
        };
        if period.month < first {
            0
        } else {
            period.month - first + 1
        }
    }

    pub fn service_at(&self, as_of: NaiveDate) -> EmployeeStats {
        let months_of_service = self.months_of_service_at(as_of);
        EmployeeStats {
            employee_id: self.id.clone(),
            project_id: self.project_id.clone(),
            as_of,
            month: YearMonth::of(as_of),
            age: self.age_on(as_of),
            months_of_service,
            years_of_service: months_of_service / 12,
            months_worked_this_year: self.months_worked_in(YearMonth::of(as_of)),
        }
    }

    /// The searchable text of an employee, already lowercased.
    fn haystack(&self) -> String {
        let mut text = format!("{} {}", self.first_name, self.last_name).to_lowercase();
        let extras = [
            Some(self.role.as_str()),
            self.email.as_deref(),
            self.phone.as_deref(),
            self.cin.as_deref(),
        ];
        for extra in extras.into_iter().flatten() {
            text.push('\u{1}');
            text.push_str(&extra.to_lowercase());
        }
        text
    }
}

/// Whole years from `from` to `to`, floored at zero.
///
/// The birthday counts on the day itself, and a 29 February birthday falls due
/// on 1 March in a common year — the same rule civil registries use.
fn whole_years_between(from: NaiveDate, to: NaiveDate) -> u32 {
    if to <= from {
        return 0;
    }
    let mut years = to.year() - from.year();
    if (to.month(), to.day()) < (from.month(), from.day()) {
        years -= 1;
    }
    years.max(0) as u32
}

/// Whole months from `from` to `to`, floored at zero.
fn whole_months_between(from: NaiveDate, to: NaiveDate) -> u32 {
    if to <= from {
        return 0;
    }
    let mut months = (to.year() - from.year()) * 12 + (to.month() as i32 - from.month() as i32);
    if to.day() < from.day() {
        months -= 1;
    }
    months.max(0) as u32
}

/// What the add/edit employee form submits. Untrusted until validated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeDraft {
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub cin: Option<String>,
    #[serde(default)]
    pub birth_date: Option<NaiveDate>,
    pub hire_date: NaiveDate,
    #[serde(default)]
    pub bank_account: Option<String>,
    #[serde(default)]
    pub emergency_contact: Option<String>,
}

impl EmployeeDraft {
    /// The minimum an employee needs: a name, a job title and a hire date.
    pub fn new(
        first_name: impl Into<String>,
        last_name: impl Into<String>,
        role: impl Into<String>,
        hire_date: NaiveDate,
    ) -> Self {
        EmployeeDraft {
            first_name: first_name.into(),
            last_name: last_name.into(),
            role: role.into(),
            email: None,
            phone: None,
            address: None,
            cin: None,
            birth_date: None,
            hire_date,
            bank_account: None,
            emergency_contact: None,
        }
    }

    /// Checks every rule and reports all the failures at once.
    pub fn validate(self) -> Result<ValidEmployee, ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let first_name = required_text(&mut errors, self.first_name, "firstName", "First name");
        let last_name = required_text(&mut errors, self.last_name, "lastName", "Last name");
        // The mockup quietly defaults a missing job title to "Operative".
        // Inventing one is worse than asking for it.
        let role = required_text(&mut errors, self.role, "role", "Role");

        let email = normalise_optional(self.email);
        if let Some(address) = email.as_deref() {
            if !looks_like_email(address) {
                errors.push("email", "That does not look like an email address");
            }
        }

        let phone = bounded_optional(&mut errors, self.phone, "phone", "Phone", MAX_CONTACT_LEN);
        let address =
            bounded_optional(&mut errors, self.address, "address", "Address", MAX_ADDRESS_LEN);
        let bank_account = bounded_optional(
            &mut errors,
            self.bank_account,
            "bankAccount",
            "Bank account",
            MAX_CONTACT_LEN,
        );
        let emergency_contact = bounded_optional(
            &mut errors,
            self.emergency_contact,
            "emergencyContact",
            "Emergency contact",
            MAX_CONTACT_LEN,
        );

        let cin = match normalise_optional(self.cin) {
            None => None,
            Some(raw) => {
                let digits: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                if !digits.chars().all(|c| c.is_ascii_digit()) {
                    errors.push("cin", "A CIN is digits only");
                    None
                } else if !CIN_DIGITS.contains(&digits.len()) {
                    errors.push(
                        "cin",
                        format!(
                            "A CIN is between {} and {} digits",
                            CIN_DIGITS.start(),
                            CIN_DIGITS.end()
                        ),
                    );
                    None
                } else {
                    Some(digits)
                }
            }
        };

        if self.birth_date.is_some_and(|birth| birth >= self.hire_date) {
            errors.push("birthDate", "Date of birth must be before the hire date");
        }

        errors.into_result(ValidEmployee {
            first_name,
            last_name,
            role,
            email,
            phone,
            address,
            cin,
            birth_date: self.birth_date,
            hire_date: self.hire_date,
            bank_account,
            emergency_contact,
        })
    }
}

fn required_text(
    errors: &mut ValidationErrors,
    value: String,
    field: &'static str,
    label: &str,
) -> String {
    let trimmed = value.trim().to_owned();
    if trimmed.is_empty() {
        errors.push(field, format!("{label} is required"));
    } else if trimmed.chars().count() > MAX_NAME_LEN {
        errors.push(field, format!("{label} cannot exceed {MAX_NAME_LEN} characters"));
    }
    trimmed
}

fn bounded_optional(
    errors: &mut ValidationErrors,
    value: Option<String>,
    field: &'static str,
    label: &str,
    limit: usize,
) -> Option<String> {
    let value = normalise_optional(value);
    if value.as_deref().is_some_and(|v| v.chars().count() > limit) {
        errors.push(field, format!("{label} cannot exceed {limit} characters"));
    }
    value
}

/// Deliberately loose. The job is to catch a typo, not to adjudicate RFC 5322.
fn looks_like_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !value.contains(char::is_whitespace)
        && value.matches('@').nth(1).is_none()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// A draft that has passed validation. The repository accepts nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidEmployee {
    first_name: String,
    last_name: String,
    role: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    cin: Option<String>,
    birth_date: Option<NaiveDate>,
    hire_date: NaiveDate,
    bank_account: Option<String>,
    emergency_contact: Option<String>,
}

impl ValidEmployee {
    pub fn first_name(&self) -> &str {
        &self.first_name
    }

    pub fn last_name(&self) -> &str {
        &self.last_name
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn phone(&self) -> Option<&str> {
        self.phone.as_deref()
    }

    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// Digits only — spaces are stripped during validation.
    pub fn cin(&self) -> Option<&str> {
        self.cin.as_deref()
    }

    pub fn birth_date(&self) -> Option<NaiveDate> {
        self.birth_date
    }

    pub fn hire_date(&self) -> NaiveDate {
        self.hire_date
    }

    pub fn bank_account(&self) -> Option<&str> {
        self.bank_account.as_deref()
    }

    pub fn emergency_contact(&self) -> Option<&str> {
        self.emergency_contact.as_deref()
    }

    pub fn into_employee(
        self,
        id: EmployeeId,
        project_id: ProjectId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Employee {
        Employee {
            id,
            project_id,
            first_name: self.first_name,
            last_name: self.last_name,
            role: self.role,
            email: self.email,
            phone: self.phone,
            address: self.address,
            cin: self.cin,
            birth_date: self.birth_date,
            hire_date: self.hire_date,
            bank_account: self.bank_account,
            emergency_contact: self.emergency_contact,
            created_at: now,
            updated_at: now,
        }
    }

    /// Applies the draft to a stored employee, keeping identity, project and
    /// creation time.
    pub fn onto(self, existing: &Employee, now: chrono::DateTime<chrono::Utc>) -> Employee {
        Employee {
            id: existing.id.clone(),
            project_id: existing.project_id.clone(),
            created_at: existing.created_at,
            ..self.into_employee(existing.id.clone(), existing.project_id.clone(), now)
        }
    }
}

/// How the employees list is narrowed: the project it is shown inside, plus
/// the search box.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeFilter {
    /// `None` lists across every project — what the "People on payroll" KPI
    /// counts.
    #[serde(default)]
    pub project: Option<ProjectId>,
    /// Matched case-insensitively against name, role, email, phone and CIN.
    #[serde(default)]
    pub query: Option<String>,
}

impl EmployeeFilter {
    pub fn in_project(project: &ProjectId) -> Self {
        EmployeeFilter { project: Some(project.clone()), query: None }
    }

    pub fn search(query: impl Into<String>) -> Self {
        EmployeeFilter { project: None, query: Some(query.into()) }
    }

    /// The free-text half of the filter, applied in Rust so case folding is
    /// Unicode-correct — SQLite's `NOCASE` only folds ASCII, and these are
    /// Malagasy names.
    pub fn matches_text(&self, employee: &Employee) -> bool {
        let Some(query) = self.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) else {
            return true;
        };
        employee.haystack().contains(&query.to_lowercase())
    }
}

/// What one employee's file can say from the employee record alone.
///
/// Leave balance, contract terms and pay all belong to slices that do not
/// exist yet; `months_worked_this_year` is here because it is derived purely
/// from the hire date, and the leave slice needs it to accrue against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeStats {
    pub employee_id: EmployeeId,
    pub project_id: ProjectId,
    pub as_of: NaiveDate,
    pub month: YearMonth,
    pub age: Option<u32>,
    pub months_of_service: u32,
    pub years_of_service: u32,
    pub months_worked_this_year: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    fn stored(draft: EmployeeDraft) -> Employee {
        draft
            .validate()
            .expect("test draft is valid")
            .into_employee(EmployeeId::new(), ProjectId::from("p1"), Utc::now())
    }

    /// The mockup's first employee.
    fn rakoto() -> EmployeeDraft {
        let mut draft = EmployeeDraft::new(
            "Rakoto",
            "Randrianasolo",
            "Site supervisor",
            date("2026-02-01"),
        );
        draft.email = Some("rakoto.randrianasolo@tymio.mg".into());
        draft.phone = Some("+261 34 12 887 01".into());
        draft.address = Some("Lot II B 44 Ambatolampy".into());
        draft.cin = Some("201021045".into());
        draft.birth_date = Some(date("1988-04-12"));
        draft
    }

    mod validation {
        use super::*;

        #[test]
        fn a_minimal_draft_needs_a_name_a_role_and_a_hire_date() {
            let valid = EmployeeDraft::new("Fara", "Rasoanaivo", "HSE officer", date("2026-02-15"))
                .validate()
                .expect("that is enough to hire someone");

            assert_eq!(valid.first_name(), "Fara");
            assert_eq!(valid.last_name(), "Rasoanaivo");
            assert_eq!(valid.role(), "HSE officer");
            assert_eq!(valid.hire_date(), date("2026-02-15"));
            assert_eq!(valid.cin(), None);
            assert_eq!(valid.birth_date(), None);
        }

        #[test]
        fn both_names_are_required() {
            let errors = EmployeeDraft::new("  ", "", "Electrician", date("2026-03-02"))
                .validate()
                .expect_err("a person needs a name");
            assert!(errors.has("firstName"));
            assert!(errors.has("lastName"));
        }

        #[test]
        fn a_job_title_is_required_rather_than_invented() {
            let errors = EmployeeDraft::new("Naivo", "Razafimahatratra", "  ", date("2026-03-02"))
                .validate()
                .expect_err("no silent default of 'Operative'");
            assert!(errors.has("role"));
        }

        #[test]
        fn names_are_trimmed_not_rejected() {
            let valid = EmployeeDraft::new("  Lalao ", " Ravelojaona ", " Panel installer ", date("2026-03-02"))
                .validate()
                .expect("padding is not an error");
            assert_eq!(valid.first_name(), "Lalao");
            assert_eq!(valid.last_name(), "Ravelojaona");
            assert_eq!(valid.role(), "Panel installer");
        }

        #[test]
        fn a_cin_is_stored_as_digits_with_the_spaces_taken_out() {
            let mut draft = rakoto();
            draft.cin = Some("201 021 045".into());
            let valid = draft.validate().expect("spacing is a display choice");
            assert_eq!(valid.cin(), Some("201021045"));
        }

        #[test]
        fn a_cin_that_is_not_a_number_is_rejected() {
            let mut draft = rakoto();
            draft.cin = Some("201-021-045".into());
            let errors = draft.validate().expect_err("hyphens are not digits");
            assert!(errors.has("cin"));
        }

        #[test]
        fn a_cin_of_the_wrong_length_is_rejected() {
            let mut short = rakoto();
            short.cin = Some("12345".into());
            assert!(short.validate().expect_err("too short").has("cin"));

            let mut long = rakoto();
            long.cin = Some("1".repeat(21));
            assert!(long.validate().expect_err("too long").has("cin"));
        }

        #[test]
        fn a_blank_cin_is_simply_absent() {
            let mut draft = rakoto();
            draft.cin = Some("   ".into());
            assert_eq!(draft.validate().expect("blank is not an error").cin(), None);
        }

        #[test]
        fn an_email_only_has_to_look_like_one() {
            let mut good = rakoto();
            good.email = Some("  Fara.Rasoanaivo@tymio.mg ".into());
            assert_eq!(
                good.validate().expect("valid").email(),
                Some("Fara.Rasoanaivo@tymio.mg")
            );

            for junk in ["tymio.mg", "fara@", "@tymio.mg", "fara @tymio.mg", "fara@@tymio.mg", "fara@tymio"] {
                let mut draft = rakoto();
                draft.email = Some(junk.into());
                assert!(
                    draft.validate().expect_err("junk address").has("email"),
                    "expected {junk:?} to be rejected"
                );
            }
        }

        #[test]
        fn a_birth_date_must_precede_the_hire_date() {
            let mut draft = rakoto();
            draft.birth_date = Some(date("2026-02-01"));
            assert!(draft.validate().expect_err("born on the hire date").has("birthDate"));

            let mut later = rakoto();
            later.birth_date = Some(date("2026-06-01"));
            assert!(later.validate().expect_err("born after being hired").has("birthDate"));
        }

        #[test]
        fn every_problem_is_reported_at_once() {
            let mut draft = EmployeeDraft::new("", "", "", date("2026-02-01"));
            draft.email = Some("not-an-address".into());
            draft.cin = Some("abc".into());
            draft.birth_date = Some(date("2027-01-01"));

            let errors = draft.validate().expect_err("six things are wrong");
            for field in ["firstName", "lastName", "role", "email", "cin", "birthDate"] {
                assert!(errors.has(field), "expected an error on {field}");
            }
            assert_eq!(errors.len(), 6);
        }

        #[test]
        fn free_text_fields_have_length_limits() {
            let mut draft = rakoto();
            draft.address = Some("a".repeat(MAX_ADDRESS_LEN + 1));
            draft.emergency_contact = Some("e".repeat(MAX_CONTACT_LEN + 1));
            let errors = draft.validate().expect_err("over the limits");
            assert!(errors.has("address"));
            assert!(errors.has("emergencyContact"));
        }
    }

    mod derived {
        use super::*;

        #[test]
        fn initials_are_the_monogram_the_avatar_shows() {
            assert_eq!(stored(rakoto()).initials(), "RR");
            assert_eq!(
                stored(EmployeeDraft::new("jean-luc", "ratsimba", "Front desk", date("2025-01-10")))
                    .initials(),
                "JR"
            );
        }

        #[test]
        fn full_name_is_first_then_last() {
            assert_eq!(stored(rakoto()).full_name(), "Rakoto Randrianasolo");
        }

        #[test]
        fn age_counts_the_birthday_on_the_day_itself() {
            let employee = stored(rakoto()); // born 1988-04-12
            assert_eq!(employee.age_on(date("2026-04-11")), Some(37));
            assert_eq!(employee.age_on(date("2026-04-12")), Some(38));
            assert_eq!(employee.age_on(date("2026-04-13")), Some(38));
        }

        #[test]
        fn a_leap_day_birthday_falls_due_on_the_first_of_march() {
            let mut draft = rakoto();
            draft.birth_date = Some(date("2000-02-29"));
            let employee = stored(draft);

            assert_eq!(employee.age_on(date("2025-02-28")), Some(24));
            assert_eq!(employee.age_on(date("2025-03-01")), Some(25));
            // In a leap year the birthday itself exists again.
            assert_eq!(employee.age_on(date("2028-02-29")), Some(28));
        }

        #[test]
        fn age_is_absent_when_no_birth_date_was_recorded() {
            let employee =
                stored(EmployeeDraft::new("Hery", "Rabemananjara", "Crane operator", date("2026-04-01")));
            assert_eq!(employee.age_on(date("2026-09-01")), None);
        }

        #[test]
        fn service_counts_whole_months_only() {
            let employee = stored(rakoto()); // hired 2026-02-01
            assert_eq!(employee.months_of_service_at(date("2026-02-01")), 0);
            assert_eq!(employee.months_of_service_at(date("2026-02-28")), 0);
            assert_eq!(employee.months_of_service_at(date("2026-03-01")), 1);
            assert_eq!(employee.months_of_service_at(date("2027-02-01")), 12);
            assert_eq!(employee.months_of_service_at(date("2027-01-31")), 11);
        }

        #[test]
        fn service_before_the_hire_date_is_zero_not_negative() {
            let employee = stored(rakoto());
            assert_eq!(employee.months_of_service_at(date("2020-01-01")), 0);
        }

        mod months_worked_in_year {
            use super::*;

            fn hired(on: &str) -> Employee {
                stored(EmployeeDraft::new("Tiana", "Andriamihaja", "Site accountant", date(on)))
            }

            fn month(year: i32, month: u32) -> YearMonth {
                YearMonth::new(year, month).expect("valid month")
            }

            #[test]
            fn someone_hired_this_year_counts_from_their_hire_month() {
                let employee = hired("2026-03-02");
                assert_eq!(employee.months_worked_in(month(2026, 3)), 1);
                assert_eq!(employee.months_worked_in(month(2026, 9)), 7);
            }

            #[test]
            fn someone_hired_earlier_counts_from_january() {
                let employee = hired("2025-10-01");
                assert_eq!(employee.months_worked_in(month(2026, 1)), 1);
                assert_eq!(employee.months_worked_in(month(2026, 9)), 9);
                assert_eq!(employee.months_worked_in(month(2026, 12)), 12);
            }

            #[test]
            fn months_before_the_hire_month_do_not_count() {
                let employee = hired("2026-05-10");
                assert_eq!(employee.months_worked_in(month(2026, 4)), 0);
                assert_eq!(employee.months_worked_in(month(2026, 5)), 1);
            }

            #[test]
            fn a_future_hire_has_worked_nothing() {
                let employee = hired("2027-01-05");
                assert_eq!(employee.months_worked_in(month(2026, 12)), 0);
            }

            #[test]
            fn the_count_resets_each_january() {
                let employee = hired("2026-03-02");
                assert_eq!(employee.months_worked_in(month(2026, 12)), 10);
                assert_eq!(employee.months_worked_in(month(2027, 1)), 1);
            }
        }

        #[test]
        fn the_service_summary_gathers_what_the_employee_file_shows() {
            let employee = stored(rakoto()); // born 1988-04-12, hired 2026-02-01
            let stats = employee.service_at(date("2026-09-15"));

            assert_eq!(stats.employee_id, employee.id);
            assert_eq!(stats.project_id, employee.project_id);
            assert_eq!(stats.month, YearMonth::new(2026, 9).expect("september"));
            assert_eq!(stats.age, Some(38));
            assert_eq!(stats.months_of_service, 7);
            assert_eq!(stats.years_of_service, 0);
            assert_eq!(stats.months_worked_this_year, 8);
        }
    }

    mod editing {
        use super::*;

        #[test]
        fn an_edit_keeps_identity_project_and_creation_time() {
            let original = stored(rakoto());
            let later = original.created_at + chrono::Duration::days(90);

            let mut draft = rakoto();
            draft.role = "Project manager".into();
            let edited = draft.validate().expect("valid").onto(&original, later);

            assert_eq!(edited.id, original.id);
            assert_eq!(edited.project_id, original.project_id);
            assert_eq!(edited.created_at, original.created_at);
            assert_eq!(edited.updated_at, later);
            assert_eq!(edited.role, "Project manager");
        }
    }

    mod filtering {
        use super::*;

        #[test]
        fn an_empty_filter_matches_everyone() {
            let employee = stored(rakoto());
            assert!(EmployeeFilter::default().matches_text(&employee));
            assert!(EmployeeFilter::search("   ").matches_text(&employee));
        }

        #[test]
        fn search_covers_name_role_email_phone_and_cin() {
            let employee = stored(rakoto());
            for query in ["rakoto", "RANDRIANASOLO", "site super", "tymio.mg", "887", "201021045"] {
                assert!(
                    EmployeeFilter::search(query).matches_text(&employee),
                    "expected {query:?} to match"
                );
            }
            assert!(!EmployeeFilter::search("electrician").matches_text(&employee));
        }

        #[test]
        fn search_folds_accented_characters() {
            let mut draft = rakoto();
            draft.last_name = "Ravololonirina".into();
            draft.role = "Chargé d'affaires".into();
            let employee = stored(draft);

            assert!(EmployeeFilter::search("chargé").matches_text(&employee));
            assert!(EmployeeFilter::search("CHARGÉ").matches_text(&employee));
        }

        #[test]
        fn search_does_not_run_across_two_different_fields() {
            let employee = stored(rakoto());
            assert!(!EmployeeFilter::search("randrianasolosite").matches_text(&employee));
        }
    }
}
