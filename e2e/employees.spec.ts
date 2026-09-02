import { expect, test } from "./fixtures";
import {
  addEmployee,
  detail,
  employeeRow,
  enterProject,
  fillEmployee,
  projectCard,
} from "./helpers";

const SOLAR_FARM = {
  name: "Ambatolampy Solar Farm",
  client: "JIRAMA",
  location: "Vakinankaratra",
  start: "2026-02-01",
  end: "2027-06-30",
};

const RAKOTO = {
  firstName: "Rakoto",
  lastName: "Randrianasolo",
  role: "Site supervisor",
  phone: "+261 34 12 887 01",
  email: "rakoto.randrianasolo@tymio.mg",
  address: "Lot II B 44 Ambatolampy",
  cin: "201021045",
  birthDate: "1988-04-12",
  hireDate: "2020-01-06",
};

test("opening a project steps into its workspace, with nobody on it yet", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);

  await expect(app.getByRole("navigation", { name: "Project sections" })).toContainText(
    "Ambatolampy Solar Farm",
  );
  await expect(app.getByRole("heading", { name: "Employees" })).toBeVisible();
  await expect(app.getByTestId("employees-empty")).toContainText("Nobody on this project yet");
});

test("a hired employee comes back from SQLite into the table", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  const row = employeeRow(app, "Rakoto Randrianasolo");
  await expect(row).toBeVisible();
  await expect(row).toContainText("Site supervisor");
  await expect(row).toContainText("06/01/2020");
  await expect(row).toContainText("201021045");
  await expect(row).toContainText("+261 34 12 887 01");

  await expect(app.getByTestId("toast")).toHaveText("Added Rakoto Randrianasolo");
  // The nav badge is the project's headcount, counted in SQL.
  await expect(app.getByRole("button", { name: /^Employees/ })).toContainText("1");
});

test("the domain rules rejecting a hire land on the fields that broke them", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);

  await app.getByRole("button", { name: "Add employee" }).click();
  const modal = app.getByTestId("employee-modal");
  await fillEmployee(modal, { lastName: "Randrianasolo", birthDate: "2027-01-01" });
  await modal.getByRole("button", { name: "Add employee" }).click();

  await expect(modal).toBeVisible();
  await expect(modal.getByTestId("error-firstName")).toHaveText("First name is required");
  // The mockup silently defaults a missing job title to "Operative".
  await expect(modal.getByTestId("error-role")).toHaveText("Role is required");
  await expect(modal.getByTestId("error-birthDate")).toHaveText(
    "Date of birth must be before the hire date",
  );

  await fillEmployee(modal, {
    firstName: "Rakoto",
    role: "Site supervisor",
    birthDate: "1988-04-12",
  });
  await modal.getByRole("button", { name: "Add employee" }).click();

  await expect(modal).toBeHidden();
  await expect(app.getByTestId("employee-row")).toHaveCount(1);
});

test("a national ID cannot be recorded against two people", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  await app.getByRole("button", { name: "Add employee" }).click();
  const modal = app.getByTestId("employee-modal");
  await fillEmployee(modal, {
    firstName: "Fara",
    lastName: "Rasoanaivo",
    role: "HSE officer",
    // The same number written with spaces is the same number.
    cin: "201 021 045",
    hireDate: "2026-02-15",
  });
  await modal.getByRole("button", { name: "Add employee" }).click();

  await expect(modal.getByTestId("modal-error")).toContainText("201021045");
  await expect(modal).toBeVisible();

  await modal.getByRole("button", { name: "Cancel" }).click();
  await expect(app.getByTestId("employee-row")).toHaveCount(1);
});

test("the search box is a query, not a client-side hide", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);
  await addEmployee(app, {
    firstName: "Fara",
    lastName: "Rasoanaivo",
    role: "HSE officer",
    hireDate: "2026-02-15",
  });

  await expect(app.getByTestId("employee-row")).toHaveCount(2);

  const search = app.getByLabel("Search employees");
  await search.fill("hse");
  await expect(app.getByTestId("employee-row")).toHaveCount(1);
  await expect(employeeRow(app, "Fara Rasoanaivo")).toBeVisible();

  // Matched against the CIN too, and case-folded in Rust.
  await search.fill("201021045");
  await expect(employeeRow(app, "Rakoto Randrianasolo")).toBeVisible();

  await search.fill("nobody");
  await expect(app.getByTestId("employees-empty")).toContainText("No employees match");

  await search.fill("");
  await expect(app.getByTestId("employee-row")).toHaveCount(2);
});

test("the employee file shows the record and the numbers Rust derives from it", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, { ...RAKOTO, bankAccount: "BNI 20102····", emergencyContact: "+261 34 20 441 09" });

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Open" }).click();
  await expect(app.getByTestId("employee-file")).toBeVisible();

  await expect(detail(app, "Date of birth")).toHaveText("12/04/1988");
  await expect(detail(app, "Phone")).toHaveText("+261 34 12 887 01");
  await expect(detail(app, "Email")).toHaveText("rakoto.randrianasolo@tymio.mg");
  await expect(detail(app, "Address")).toHaveText("Lot II B 44 Ambatolampy");
  await expect(detail(app, "Emergency contact")).toHaveText("+261 34 20 441 09");
  await expect(detail(app, "National ID (CIN)")).toHaveText("201021045");
  await expect(detail(app, "Project")).toHaveText("Ambatolampy Solar Farm");
  await expect(detail(app, "Hired on")).toHaveText("06/01/2020");
  await expect(detail(app, "Bank account")).toHaveText("BNI 20102····");

  // Age and service are computed in Rust, not in the browser.
  await expect(app.getByTestId("tag-age")).toContainText(/^\d+ years old$/);
  await expect(app.getByTestId("tag-service")).toContainText(/years?/);

  await app.getByRole("button", { name: "← All employees" }).click();
  await expect(app.getByTestId("employee-table")).toBeVisible();
});

test("the period selector drives the accrual month on the employee file", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  // Hired in an earlier year, so months-worked-this-year is simply the month
  // number — the mockup's accrual rule, reachable through the UI.
  await addEmployee(app, RAKOTO);
  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Open" }).click();

  const period = app.getByLabel("Period");
  for (const index of [0, 1, 5]) {
    const value = await period.locator("option").nth(index).getAttribute("value");
    const [year, month] = value!.split("-");
    await period.selectOption(value!);
    await expect(detail(app, `Months worked in ${year}`)).toHaveText(String(Number(month)));
  }
});

test("an edit is persisted and shows up in both the table and the file", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Edit" }).click();
  const modal = app.getByTestId("employee-modal");
  await fillEmployee(modal, { role: "Project manager" });
  await modal.getByRole("button", { name: "Save employee" }).click();
  await expect(modal).toBeHidden();

  await expect(employeeRow(app, "Rakoto Randrianasolo")).toContainText("Project manager");
  await expect(app.getByTestId("toast")).toHaveText("Employee saved");

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Open" }).click();
  await expect(detail(app, "Role")).toHaveText("Project manager");
});

test("removing an employee asks first, and the headcount follows", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Remove" }).click();
  const confirm = app.getByTestId("confirm-dialog");
  await expect(confirm).toContainText("Remove Rakoto Randrianasolo?");

  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(employeeRow(app, "Rakoto Randrianasolo")).toBeVisible();

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Remove" }).click();
  await app.getByTestId("confirm-dialog").getByRole("button", { name: "Remove employee" }).click();

  await expect(app.getByTestId("employees-empty")).toBeVisible();
  await expect(app.getByTestId("toast")).toHaveText("Removed Rakoto Randrianasolo");
});

test("the headcount reaches the project picker and the audit log", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  await app.getByRole("button", { name: "Switch project" }).click();

  await expect(app.getByTestId("kpi-People on payroll")).toHaveText("1");
  await expect(app.getByTestId("activity")).toContainText("Created employee — Rakoto");

  // Deleting the project takes its people with it, and says so.
  await projectCard(app, "Ambatolampy Solar Farm").getByRole("button", { name: "Delete" }).click();
  await app.getByTestId("confirm-dialog").getByRole("button", { name: "Delete project" }).click();

  await expect(app.getByTestId("kpi-People on payroll")).toHaveText("0");
  await expect(app.getByTestId("activity")).toContainText("Removed employee — Rakoto");
});

test("the unbuilt tabs and sections say what they are waiting for", async ({ app }) => {
  await enterProject(app, SOLAR_FARM);
  await addEmployee(app, RAKOTO);

  await employeeRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Open" }).click();
  await app.getByRole("tab", { name: "Contract" }).click();
  await expect(app.getByTestId("coming-soon")).toContainText("contracts slice");

  await app.getByRole("button", { name: /^Payroll/ }).click();
  await expect(app.getByTestId("coming-soon")).toContainText("payroll slice");

  // Overview is real: it reads the project's own work calendar.
  await app.getByRole("button", { name: /^Overview/ }).click();
  await expect(app.getByTestId("kpi-Headcount")).toHaveText("1");
  await expect(app.getByTestId("kpi-Working days")).toHaveText(/^\d+$/);
});
