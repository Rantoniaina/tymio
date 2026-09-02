//! Projects — the top level. Employees, contracts, leave and payroll all sit
//! inside one, so this is the first thing that has to be right.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::calendar::{DayLength, HolidaySet, WeekdayMask, WorkCalendar, YearMonth};
use super::{id_type, normalise_optional, ValidationErrors};

/// Longest a name, client or location may be. Generous for real Malagasy
/// project names, short enough that the UI can rely on it.
pub const MAX_TEXT_LEN: usize = 120;

id_type! {
    /// Identifies a project. Opaque — do not parse it, do not sort by it.
    ProjectId
}

id_type! {
    /// Identifies one holiday within a project's work calendar.
    HolidayId
}

/// Where a project is in its life. Drives the filter chips on the project list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Active,
    Paused,
    Closed,
}

impl ProjectStatus {
    pub const ALL: [ProjectStatus; 3] =
        [ProjectStatus::Active, ProjectStatus::Paused, ProjectStatus::Closed];

    /// How the status is stored, and the only spelling the DB CHECK accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Paused => "paused",
            ProjectStatus::Closed => "closed",
        }
    }
}

impl fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProjectStatus {
    type Err = UnknownStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(ProjectStatus::Active),
            "paused" => Ok(ProjectStatus::Paused),
            "closed" => Ok(ProjectStatus::Closed),
            other => Err(UnknownStatus(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a project status (expected active, paused or closed)")]
pub struct UnknownStatus(pub String);

/// A project as it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub client: Option<String>,
    pub location: Option<String>,
    pub status: ProjectStatus,
    pub start: NaiveDate,
    /// Absent for an open-ended project. Everything derived from a duration
    /// has to cope with not knowing when this ends.
    pub end: Option<NaiveDate>,
    pub calendar: WorkCalendar,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn duration(&self, as_of: NaiveDate) -> DurationProgress {
        DurationProgress::compute(self.start, self.end, as_of)
    }

    /// The searchable text of a project, already lowercased.
    fn haystack(&self) -> String {
        let mut text = self.name.to_lowercase();
        for extra in [self.client.as_deref(), self.location.as_deref()].into_iter().flatten() {
            text.push('\u{1}');
            text.push_str(&extra.to_lowercase());
        }
        text
    }
}

/// What the new/edit project form submits. Untrusted until validated.
///
/// Edits replace the whole draft rather than patching fields, because the UI
/// edits a project through one full form — there is no partial update to model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDraft {
    pub name: String,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub status: ProjectStatus,
    pub start: NaiveDate,
    #[serde(default)]
    pub end: Option<NaiveDate>,
    #[serde(default)]
    pub working_days: WeekdayMask,
    #[serde(default)]
    pub day_length: DayLength,
}

impl ProjectDraft {
    /// A draft with the defaults a new project starts from: active, Mon–Fri,
    /// eight-hour days, open-ended.
    pub fn new(name: impl Into<String>, start: NaiveDate) -> Self {
        ProjectDraft {
            name: name.into(),
            client: None,
            location: None,
            status: ProjectStatus::default(),
            start,
            end: None,
            working_days: WeekdayMask::default(),
            day_length: DayLength::default(),
        }
    }

    /// Checks every rule and reports all the failures at once.
    pub fn validate(self) -> Result<ValidProject, ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let name = self.name.trim().to_owned();
        if name.is_empty() {
            errors.push("name", "Project name is required");
        } else if name.chars().count() > MAX_TEXT_LEN {
            errors.push("name", format!("Project name cannot exceed {MAX_TEXT_LEN} characters"));
        }

        let client = normalise_optional(self.client);
        if client.as_deref().is_some_and(|c| c.chars().count() > MAX_TEXT_LEN) {
            errors.push("client", format!("Client cannot exceed {MAX_TEXT_LEN} characters"));
        }

        let location = normalise_optional(self.location);
        if location.as_deref().is_some_and(|l| l.chars().count() > MAX_TEXT_LEN) {
            errors.push("location", format!("Location cannot exceed {MAX_TEXT_LEN} characters"));
        }

        if self.end.is_some_and(|end| end < self.start) {
            errors.push("end", "End date cannot be before the start date");
        }

        errors.into_result(ValidProject {
            name,
            client,
            location,
            status: self.status,
            start: self.start,
            end: self.end,
            calendar: WorkCalendar::new(self.working_days, self.day_length),
        })
    }
}

/// A draft that has passed validation. The repository accepts nothing else,
/// so an invalid project cannot reach the database by another route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidProject {
    name: String,
    client: Option<String>,
    location: Option<String>,
    status: ProjectStatus,
    start: NaiveDate,
    end: Option<NaiveDate>,
    calendar: WorkCalendar,
}

impl ValidProject {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    pub fn status(&self) -> ProjectStatus {
        self.status
    }

    pub fn start(&self) -> NaiveDate {
        self.start
    }

    pub fn end(&self) -> Option<NaiveDate> {
        self.end
    }

    pub fn calendar(&self) -> WorkCalendar {
        self.calendar
    }

    /// Applies the draft to a stored project, keeping identity and creation time.
    pub fn onto(self, existing: &Project, now: DateTime<Utc>) -> Project {
        Project {
            id: existing.id.clone(),
            name: self.name,
            client: self.client,
            location: self.location,
            status: self.status,
            start: self.start,
            end: self.end,
            calendar: self.calendar,
            created_at: existing.created_at,
            updated_at: now,
        }
    }

    /// Turns the draft into a brand-new project with a fresh identity.
    pub fn into_project(self, id: ProjectId, now: DateTime<Utc>) -> Project {
        Project {
            id,
            name: self.name,
            client: self.client,
            location: self.location,
            status: self.status,
            start: self.start,
            end: self.end,
            calendar: self.calendar,
            created_at: now,
            updated_at: now,
        }
    }
}

/// One non-weekend day off in a project's work calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holiday {
    pub id: HolidayId,
    pub project_id: ProjectId,
    pub date: NaiveDate,
    pub name: String,
}

/// What the add-holiday form submits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolidayDraft {
    pub date: NaiveDate,
    pub name: String,
}

impl HolidayDraft {
    pub fn new(date: NaiveDate, name: impl Into<String>) -> Self {
        HolidayDraft { date, name: name.into() }
    }

    pub fn validate(self) -> Result<ValidHoliday, ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let name = self.name.trim().to_owned();
        if name.is_empty() {
            errors.push("name", "Holiday name is required");
        } else if name.chars().count() > MAX_TEXT_LEN {
            errors.push("name", format!("Holiday name cannot exceed {MAX_TEXT_LEN} characters"));
        }

        errors.into_result(ValidHoliday { date: self.date, name })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidHoliday {
    date: NaiveDate,
    name: String,
}

impl ValidHoliday {
    pub fn date(&self) -> NaiveDate {
        self.date
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// How the project list is narrowed: status chips plus the search box.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectFilter {
    /// `None` is the "All" chip.
    #[serde(default)]
    pub status: Option<ProjectStatus>,
    /// Matched case-insensitively against name, client and location.
    #[serde(default)]
    pub query: Option<String>,
}

impl ProjectFilter {
    pub fn with_status(status: ProjectStatus) -> Self {
        ProjectFilter { status: Some(status), query: None }
    }

    pub fn search(query: impl Into<String>) -> Self {
        ProjectFilter { status: None, query: Some(query.into()) }
    }

    /// The free-text half of the filter. Status is applied in SQL; this is
    /// applied in Rust so that case folding is Unicode-correct — SQLite's
    /// `NOCASE` only folds ASCII, and these are Malagasy names.
    pub fn matches_text(&self, project: &Project) -> bool {
        let Some(query) = self.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) else {
            return true;
        };
        project.haystack().contains(&query.to_lowercase())
    }
}

/// How far through its duration a project is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurationProgress {
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    /// Calendar days from start to end inclusive. `None` when open-ended.
    pub total_days: Option<u32>,
    /// Calendar days from the start up to and including `as_of`, never past
    /// the end. Zero before the project starts.
    pub elapsed_days: u32,
    pub remaining_days: Option<u32>,
    /// 0–100. `None` when open-ended, because there is nothing to be a
    /// percentage of.
    pub percent_elapsed: Option<u8>,
}

impl DurationProgress {
    pub fn compute(start: NaiveDate, end: Option<NaiveDate>, as_of: NaiveDate) -> Self {
        // Inclusive day counting: on its start date, a project is one day in.
        let days_since_start = if as_of < start {
            0
        } else {
            ((as_of - start).num_days() + 1).clamp(0, u32::MAX.into()) as u32
        };

        let Some(end) = end.filter(|end| *end >= start) else {
            return DurationProgress {
                start,
                end: None,
                total_days: None,
                elapsed_days: days_since_start,
                remaining_days: None,
                percent_elapsed: None,
            };
        };

        let total_days = ((end - start).num_days() + 1).clamp(1, u32::MAX.into()) as u32;
        let elapsed_days = days_since_start.min(total_days);
        // Integer arithmetic, rounding half up — no float ever touches this.
        let percent = ((u64::from(elapsed_days) * 100 + u64::from(total_days) / 2)
            / u64::from(total_days))
        .min(100) as u8;

        DurationProgress {
            start,
            end: Some(end),
            total_days: Some(total_days),
            elapsed_days,
            remaining_days: Some(total_days - elapsed_days),
            percent_elapsed: Some(percent),
        }
    }
}

/// The numbers on one project's overview, as far as projects alone can tell.
///
/// Headcount, monthly cost and pending-leave counts belong here too, but they
/// read tables that do not exist yet; they join when employees, leave and
/// payroll land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStats {
    pub project_id: ProjectId,
    pub status: ProjectStatus,
    /// The date the stats were taken as of — passed in, never `today()` deep
    /// inside, so the numbers are reproducible in a test.
    pub as_of: NaiveDate,
    pub month: YearMonth,
    pub duration: DurationProgress,
    pub holiday_count: u32,
    pub working_days_this_month: u32,
    pub working_minutes_this_month: u64,
}

impl ProjectStats {
    pub fn compute(project: &Project, holidays: &HolidaySet, as_of: NaiveDate) -> Self {
        let month = YearMonth::of(as_of);
        ProjectStats {
            project_id: project.id.clone(),
            status: project.status,
            as_of,
            month,
            duration: project.duration(as_of),
            holiday_count: holidays.len() as u32,
            working_days_this_month: project.calendar.working_days_in_month(month, holidays),
            working_minutes_this_month: project.calendar.working_minutes_in_month(month, holidays),
        }
    }
}

/// The KPI row above the project list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioStats {
    pub total: u32,
    pub active: u32,
    pub paused: u32,
    pub closed: u32,
}

impl PortfolioStats {
    pub fn count(&self, status: ProjectStatus) -> u32 {
        match status {
            ProjectStatus::Active => self.active,
            ProjectStatus::Paused => self.paused,
            ProjectStatus::Closed => self.closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    fn stored(draft: ProjectDraft) -> Project {
        draft
            .validate()
            .expect("draft is valid")
            .into_project(ProjectId::new(), Utc::now())
    }

    mod status {
        use super::*;

        #[test]
        fn round_trips_through_its_stored_spelling() {
            for status in ProjectStatus::ALL {
                assert_eq!(status.as_str().parse::<ProjectStatus>(), Ok(status));
            }
        }

        #[test]
        fn parses_the_capitalised_labels_the_mockup_uses() {
            assert_eq!("Active".parse::<ProjectStatus>(), Ok(ProjectStatus::Active));
            assert_eq!(" Paused ".parse::<ProjectStatus>(), Ok(ProjectStatus::Paused));
            assert_eq!("CLOSED".parse::<ProjectStatus>(), Ok(ProjectStatus::Closed));
        }

        #[test]
        fn rejects_anything_else() {
            assert_eq!(
                "archived".parse::<ProjectStatus>(),
                Err(UnknownStatus("archived".into()))
            );
        }

        #[test]
        fn a_new_project_is_active() {
            assert_eq!(ProjectStatus::default(), ProjectStatus::Active);
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn a_minimal_draft_is_valid_and_takes_the_defaults() {
            let valid = ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01"))
                .validate()
                .expect("name and start are all a project needs");

            assert_eq!(valid.name(), "Ambatolampy Solar Farm");
            assert_eq!(valid.client(), None);
            assert_eq!(valid.status(), ProjectStatus::Active);
            assert_eq!(valid.end(), None);
            assert_eq!(valid.calendar(), WorkCalendar::default());
        }

        #[test]
        fn name_is_required() {
            let errors = ProjectDraft::new("", date("2026-02-01"))
                .validate()
                .expect_err("a project must be named");
            assert!(errors.has("name"));
        }

        #[test]
        fn a_whitespace_only_name_is_no_name() {
            let errors = ProjectDraft::new("   \t ", date("2026-02-01"))
                .validate()
                .expect_err("whitespace is not a name");
            assert!(errors.has("name"));
        }

        #[test]
        fn name_is_trimmed_rather_than_rejected() {
            let valid = ProjectDraft::new("  Toamasina Port Logistics  ", date("2025-09-15"))
                .validate()
                .expect("padding is not an error");
            assert_eq!(valid.name(), "Toamasina Port Logistics");
        }

        #[test]
        fn name_has_a_length_limit_counted_in_characters() {
            // Accented characters count once, not once per byte.
            let long = "é".repeat(MAX_TEXT_LEN);
            assert!(ProjectDraft::new(long, date("2026-02-01")).validate().is_ok());

            let too_long = "é".repeat(MAX_TEXT_LEN + 1);
            let errors = ProjectDraft::new(too_long, date("2026-02-01"))
                .validate()
                .expect_err("over the limit");
            assert!(errors.has("name"));
        }

        #[test]
        fn blank_client_and_location_become_absent() {
            let mut draft = ProjectDraft::new("Nosy Be Resort Staffing", date("2025-01-10"));
            draft.client = Some("   ".into());
            draft.location = Some("  Nosy Be  ".into());

            let valid = draft.validate().expect("blank optionals are not errors");
            assert_eq!(valid.client(), None);
            assert_eq!(valid.location(), Some("Nosy Be"));
        }

        #[test]
        fn end_cannot_precede_start() {
            let mut draft = ProjectDraft::new("Antananarivo HQ Fit-out", date("2026-05-01"));
            draft.end = Some(date("2026-04-30"));

            let errors = draft.validate().expect_err("a project cannot end before it starts");
            assert!(errors.has("end"));
        }

        #[test]
        fn a_single_day_project_is_allowed() {
            let mut draft = ProjectDraft::new("One-day audit", date("2026-05-01"));
            draft.end = Some(date("2026-05-01"));
            assert!(draft.validate().is_ok());
        }

        #[test]
        fn every_problem_is_reported_at_once() {
            let mut draft = ProjectDraft::new("", date("2026-05-01"));
            draft.end = Some(date("2026-04-01"));
            draft.client = Some("c".repeat(MAX_TEXT_LEN + 1));

            let errors = draft.validate().expect_err("three things are wrong");
            assert_eq!(errors.len(), 3);
            assert!(errors.has("name"));
            assert!(errors.has("end"));
            assert!(errors.has("client"));
        }
    }

    mod editing {
        use super::*;

        #[test]
        fn an_edit_keeps_identity_and_creation_time() {
            let original = stored(ProjectDraft::new("Old name", date("2026-02-01")));
            let later = original.created_at + chrono::Duration::days(30);

            let mut draft = ProjectDraft::new("New name", date("2026-03-01"));
            draft.status = ProjectStatus::Paused;
            let edited = draft.validate().expect("valid").onto(&original, later);

            assert_eq!(edited.id, original.id);
            assert_eq!(edited.created_at, original.created_at);
            assert_eq!(edited.updated_at, later);
            assert_eq!(edited.name, "New name");
            assert_eq!(edited.status, ProjectStatus::Paused);
        }
    }

    mod filtering {
        use super::*;

        fn project(name: &str, client: Option<&str>, location: Option<&str>) -> Project {
            let mut draft = ProjectDraft::new(name, date("2026-01-01"));
            draft.client = client.map(str::to_owned);
            draft.location = location.map(str::to_owned);
            stored(draft)
        }

        #[test]
        fn an_empty_filter_matches_everything() {
            let p = project("Ambatolampy Solar Farm", None, None);
            assert!(ProjectFilter::default().matches_text(&p));
            assert!(ProjectFilter::search("   ").matches_text(&p));
        }

        #[test]
        fn search_is_case_insensitive_across_name_client_and_location() {
            let p = project("Ambatolampy Solar Farm", Some("JIRAMA"), Some("Vakinankaratra"));
            assert!(ProjectFilter::search("solar").matches_text(&p));
            assert!(ProjectFilter::search("jirama").matches_text(&p));
            assert!(ProjectFilter::search("VAKIN").matches_text(&p));
            assert!(!ProjectFilter::search("Toamasina").matches_text(&p));
        }

        #[test]
        fn search_folds_accented_characters_too() {
            let p = project("Nosy Be Resort Staffing", Some("Baobab Hôtels"), None);
            assert!(ProjectFilter::search("hôtels").matches_text(&p));
            assert!(ProjectFilter::search("HÔTELS").matches_text(&p));
        }

        #[test]
        fn search_does_not_match_across_two_different_fields() {
            let p = project("Solar", Some("Farm"), None);
            assert!(!ProjectFilter::search("solarfarm").matches_text(&p));
        }

        #[test]
        fn a_missing_client_is_not_searchable_text() {
            let p = project("Ambatolampy Solar Farm", None, None);
            assert!(!ProjectFilter::search("jirama").matches_text(&p));
        }
    }

    mod duration {
        use super::*;

        #[test]
        fn a_project_is_one_day_in_on_its_start_date() {
            let d = DurationProgress::compute(date("2026-02-01"), Some(date("2026-02-10")), date("2026-02-01"));
            assert_eq!(d.total_days, Some(10));
            assert_eq!(d.elapsed_days, 1);
            assert_eq!(d.remaining_days, Some(9));
            assert_eq!(d.percent_elapsed, Some(10));
        }

        #[test]
        fn before_the_start_nothing_has_elapsed() {
            let d = DurationProgress::compute(date("2026-02-01"), Some(date("2026-02-10")), date("2026-01-15"));
            assert_eq!(d.elapsed_days, 0);
            assert_eq!(d.remaining_days, Some(10));
            assert_eq!(d.percent_elapsed, Some(0));
        }

        #[test]
        fn past_the_end_it_stops_at_a_hundred_percent() {
            let d = DurationProgress::compute(date("2025-01-10"), Some(date("2026-03-31")), date("2026-09-01"));
            assert_eq!(d.elapsed_days, d.total_days.expect("closed project"));
            assert_eq!(d.remaining_days, Some(0));
            assert_eq!(d.percent_elapsed, Some(100));
        }

        #[test]
        fn a_single_day_project_is_whole_on_the_day() {
            let day = date("2026-05-01");
            let d = DurationProgress::compute(day, Some(day), day);
            assert_eq!(d.total_days, Some(1));
            assert_eq!(d.elapsed_days, 1);
            assert_eq!(d.percent_elapsed, Some(100));
        }

        #[test]
        fn halfway_through_reads_fifty_percent() {
            // 2026-02-01 to 2026-02-10 is 10 days; day 5 is 50%.
            let d = DurationProgress::compute(date("2026-02-01"), Some(date("2026-02-10")), date("2026-02-05"));
            assert_eq!(d.percent_elapsed, Some(50));
        }

        #[test]
        fn percentage_rounds_half_up_and_never_overflows() {
            // 3 days elapsed of 8 is 37.5%, which rounds to 38.
            let d = DurationProgress::compute(date("2026-02-01"), Some(date("2026-02-08")), date("2026-02-03"));
            assert_eq!(d.elapsed_days, 3);
            assert_eq!(d.total_days, Some(8));
            assert_eq!(d.percent_elapsed, Some(38));
        }

        #[test]
        fn an_open_ended_project_has_no_percentage() {
            let d = DurationProgress::compute(date("2026-02-01"), None, date("2026-02-11"));
            assert_eq!(d.total_days, None);
            assert_eq!(d.remaining_days, None);
            assert_eq!(d.percent_elapsed, None);
            assert_eq!(d.elapsed_days, 11);
        }

        #[test]
        fn the_mockup_projects_land_where_the_cards_show_them() {
            // Ambatolampy Solar Farm, as of the mockup's "today".
            let d = DurationProgress::compute(date("2026-02-01"), Some(date("2027-06-30")), date("2026-09-01"));
            assert_eq!(d.percent_elapsed, Some(41));

            // Nosy Be Resort Staffing ended in March; it is done.
            let closed = DurationProgress::compute(date("2025-01-10"), Some(date("2026-03-31")), date("2026-09-01"));
            assert_eq!(closed.percent_elapsed, Some(100));
        }
    }

    mod stats {
        use super::*;

        #[test]
        fn project_stats_use_the_projects_own_calendar() {
            let mut draft = ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01"));
            draft.end = Some(date("2027-06-30"));
            draft.working_days = WeekdayMask::MON_SAT;
            draft.day_length = DayLength::from_hours_and_minutes(7, 30).expect("7h30");
            let project = stored(draft);

            let holidays: HolidaySet = ["2026-09-07"].iter().map(|s| date(s)).collect();
            let stats = ProjectStats::compute(&project, &holidays, date("2026-09-15"));

            assert_eq!(stats.month, YearMonth::new(2026, 9).expect("september"));
            assert_eq!(stats.holiday_count, 1);
            // September 2026 has 26 Mon–Sat days; one is a holiday.
            assert_eq!(stats.working_days_this_month, 25);
            assert_eq!(stats.working_minutes_this_month, 25 * 450);
            assert_eq!(stats.duration.percent_elapsed, Some(44));
        }

        #[test]
        fn portfolio_counts_are_addressable_by_status() {
            let stats = PortfolioStats { total: 4, active: 2, paused: 1, closed: 1 };
            assert_eq!(stats.count(ProjectStatus::Active), 2);
            assert_eq!(stats.count(ProjectStatus::Paused), 1);
            assert_eq!(stats.count(ProjectStatus::Closed), 1);
            assert_eq!(stats.active + stats.paused + stats.closed, stats.total);
        }
    }
}
