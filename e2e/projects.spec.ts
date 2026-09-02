import { expect, firstMondayOfThisMonth, monthsFromNow, test } from "./fixtures";
import { createProject, openProjectEditor, projectCard as card } from "./helpers";

const openEditor = openProjectEditor;

test("an empty database offers a first project rather than an empty grid", async ({ app }) => {
  await expect(app.getByTestId("empty-state")).toBeVisible();
  await expect(app.getByText("No projects yet")).toBeVisible();
  await expect(app.getByTestId("kpi-Active projects")).toHaveText("0");
  await expect(app.getByTestId("activity")).toContainText("Nothing has happened yet");
});

test("a created project comes back from SQLite onto a card", async ({ app }) => {
  await createProject(app, {
    name: "Ambatolampy Solar Farm",
    client: "JIRAMA",
    location: "Vakinankaratra",
    start: "2026-02-01",
    end: "2027-06-30",
  });

  const project = card(app, "Ambatolampy Solar Farm");
  await expect(project).toBeVisible();
  await expect(project).toContainText("JIRAMA · Vakinankaratra");
  await expect(project.locator(".pill")).toHaveText("Active");
  // Malagasy date convention, formatted from the civil date Rust stored.
  await expect(project.locator(".project__dates")).toContainText("01/02/2026");
  await expect(project.locator(".project__dates")).toContainText("30/06/2027");
  await expect(project.locator(".project__progress-label")).toHaveText(
    /^\d{1,3}% of duration elapsed$/,
  );

  await expect(app.getByTestId("kpi-Active projects")).toHaveText("1");
  await expect(app.getByTestId("toast")).toHaveText("Created Ambatolampy Solar Farm");
  // The audit log is written in the same transaction as the insert.
  await expect(app.getByTestId("activity")).toContainText(
    "Created project — Ambatolampy Solar Farm",
  );
});

test("an open-ended project says so instead of showing a false percentage", async ({ app }) => {
  await createProject(app, { name: "Ongoing maintenance", start: "2026-01-01" });

  const project = card(app, "Ongoing maintenance");
  await expect(project.locator(".project__progress-label")).toHaveText(
    "Open-ended — no end date set",
  );
  await expect(project.locator(".project__dates")).toContainText("—");
});

test("the domain rules rejecting a form land on the fields that broke them", async ({ app }) => {
  await app.getByRole("button", { name: "New project" }).click();
  const modal = app.getByTestId("project-modal");

  // Nameless, and ending before it starts: Rust reports both at once.
  await modal.getByLabel("Start date").fill("2026-05-01");
  await modal.getByLabel("End date").fill("2026-04-30");
  await modal.getByRole("button", { name: "Create project" }).click();

  await expect(modal).toBeVisible();
  await expect(modal.getByTestId("error-name")).toHaveText("Project name is required");
  await expect(modal.getByTestId("error-end")).toHaveText(
    "End date cannot be before the start date",
  );

  // Fixing both saves, and nothing was stored in the meantime.
  await modal.getByLabel("Project name").fill("Antananarivo HQ Fit-out");
  await modal.getByLabel("End date").fill("2026-11-30");
  await modal.getByRole("button", { name: "Create project" }).click();

  await expect(modal).toBeHidden();
  await expect(app.getByTestId("project-card")).toHaveCount(1);
});

test("an edit is persisted and the status chips follow it", async ({ app }) => {
  await createProject(app, { name: "Antananarivo HQ Fit-out", client: "Tymio internal" });

  const modal = await openEditor(app, "Antananarivo HQ Fit-out");
  await modal.getByLabel("Status").selectOption("paused");
  await modal.getByRole("button", { name: "Save project" }).click();
  await expect(modal).toBeHidden();

  await expect(card(app, "Antananarivo HQ Fit-out").locator(".pill")).toHaveText("Paused");
  await expect(app.getByTestId("kpi-Active projects")).toHaveText("0");
  await expect(app.getByTestId("kpi-Paused")).toHaveText("1");

  // The filter runs in SQL, so this is the query, not a client-side hide.
  await app.getByRole("button", { name: "Active", exact: true }).click();
  await expect(app.getByTestId("empty-state")).toContainText("No active projects");

  await app.getByRole("button", { name: "Paused", exact: true }).click();
  await expect(card(app, "Antananarivo HQ Fit-out")).toBeVisible();

  await app.getByRole("button", { name: "All", exact: true }).click();
  await expect(app.getByTestId("project-card")).toHaveCount(1);

  await expect(app.getByTestId("activity")).toContainText(
    "Updated project — Antananarivo HQ Fit-out",
  );
});

test("the work calendar is editable and reaches the card", async ({ app }) => {
  await app.getByRole("button", { name: "New project" }).click();
  const modal = app.getByTestId("project-modal");

  await modal.getByLabel("Project name").fill("Toamasina Port Logistics");
  await modal.getByRole("button", { name: "Saturday" }).click();
  await modal.getByLabel("Hours per day").fill("7");
  await modal.getByLabel("Minutes per day").fill("30");
  await modal.getByRole("button", { name: "Create project" }).click();
  await expect(modal).toBeHidden();

  // Stored as 450 minutes, printed the way Rust's DayLength prints it.
  await expect(card(app, "Toamasina Port Logistics")).toContainText("7 h 30");

  const reopened = await openEditor(app, "Toamasina Port Logistics");
  await expect(reopened.getByRole("button", { name: "Saturday" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(reopened.getByLabel("Hours per day")).toHaveValue("7");
  await expect(reopened.getByLabel("Minutes per day")).toHaveValue("30");
});

test("a holiday takes a day off the month, and a date cannot be booked twice", async ({ app }) => {
  await createProject(app, {
    name: "Ambatolampy Solar Farm",
    start: monthsFromNow(-6),
    end: monthsFromNow(6),
  });

  const project = card(app, "Ambatolampy Solar Farm");
  const before = Number(await project.getByTestId("figure-working-days").innerText());
  expect(before).toBeGreaterThan(0);

  const modal = await openEditor(app, "Ambatolampy Solar Farm");
  const monday = firstMondayOfThisMonth();
  await modal.getByLabel("Holiday date").fill(monday);
  await modal.getByLabel("Holiday name").fill("Site shutdown");
  await modal.getByRole("button", { name: "Add", exact: true }).click();

  await expect(modal.getByTestId("holiday-list")).toContainText("Site shutdown");

  // The same date again is a unique-constraint conflict, surfaced as a message.
  await modal.getByLabel("Holiday date").fill(monday);
  await modal.getByLabel("Holiday name").fill("Duplicate");
  await modal.getByRole("button", { name: "Add", exact: true }).click();
  await expect(modal.getByTestId("error-holiday")).toContainText(monday);
  await expect(modal.getByTestId("holiday-list").locator("li")).toHaveCount(1);

  await modal.getByRole("button", { name: "Cancel" }).click();
  await expect(modal).toBeHidden();

  await expect(project.getByTestId("figure-holidays")).toHaveText("1");
  await expect(project.getByTestId("figure-working-days")).toHaveText(String(before - 1));
});

test("deleting a project asks first, then logs what it removed", async ({ app }) => {
  await createProject(app, { name: "Nosy Be Resort Staffing", client: "Baobab Hôtels" });

  await card(app, "Nosy Be Resort Staffing").getByRole("button", { name: "Delete" }).click();
  const confirm = app.getByTestId("confirm-dialog");
  await expect(confirm).toContainText("Delete Nosy Be Resort Staffing?");

  // Backing out changes nothing.
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toBeHidden();
  await expect(card(app, "Nosy Be Resort Staffing")).toBeVisible();

  await card(app, "Nosy Be Resort Staffing").getByRole("button", { name: "Delete" }).click();
  await app.getByTestId("confirm-dialog").getByRole("button", { name: "Delete project" }).click();

  await expect(app.getByTestId("empty-state")).toBeVisible();
  await expect(app.getByTestId("kpi-Active projects")).toHaveText("0");
  await expect(app.getByTestId("activity")).toContainText(
    "Deleted project — Nosy Be Resort Staffing",
  );
});
