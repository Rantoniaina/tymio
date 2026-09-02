import { expect, firstMondayOfThisMonth, test } from "./fixtures";
import {
  addEmployee,
  attendanceBox,
  attendanceRow,
  enterProject,
  fillFromSchedule,
  goToAttendance,
  openProjectEditor,
  setAttendance,
} from "./helpers";
import type { Page } from "@playwright/test";

const PROJECT = { name: "Ambatolampy Solar Farm", client: "JIRAMA", start: "2020-01-01" };

const RAKOTO = {
  firstName: "Rakoto",
  lastName: "Randrianasolo",
  role: "Site supervisor",
  hireDate: "2020-01-06",
};

const FARA = {
  firstName: "Fara",
  lastName: "Rasoanaivo",
  role: "HSE officer",
  hireDate: "2020-02-03",
};

/** Reads one row's three numbers as the grid currently shows them. */
async function readRow(page: Page, name: string) {
  return {
    days: await attendanceBox(page, name, "Days worked").inputValue(),
    hours: await attendanceBox(page, name, "Hours worked").inputValue(),
    overtime: await attendanceBox(page, name, "Overtime").inputValue(),
  };
}

async function staffedProject(page: Page) {
  await enterProject(page, PROJECT);
  await addEmployee(page, RAKOTO);
  await addEmployee(page, FARA);
  await goToAttendance(page);
}

test("the grid gives everyone a line, blank until somebody records it", async ({ app }) => {
  await staffedProject(app);

  await expect(app.getByTestId("attendance-row")).toHaveCount(2);
  await expect(app.getByTestId("row-blank")).toHaveCount(2);
  await expect(app.getByTestId("total-days")).toHaveText("0");
  await expect(app.getByTestId("attendance-totals")).toContainText("0 of 2");
});

test("filling from the schedule seeds every row from the project calendar", async ({ app }) => {
  await staffedProject(app);

  await fillFromSchedule(app);
  await expect(app.getByTestId("toast")).toContainText("Filled 2 of 2");

  const rakoto = await readRow(app, "Rakoto Randrianasolo");
  const fara = await readRow(app, "Fara Rasoanaivo");

  // Both were hired years ago, so both get the whole month — whatever this
  // month happens to hold.
  expect(Number(rakoto.days)).toBeGreaterThan(0);
  expect(fara.days).toEqual(rakoto.days);
  // Eight-hour days, so hours follow days exactly.
  expect(Number(rakoto.hours)).toBe(Number(rakoto.days) * 8);
  expect(rakoto.overtime).toBe("0");

  await expect(app.getByTestId("row-source").first()).toHaveText("Schedule");
  await expect(app.getByTestId("total-days")).toHaveText(String(Number(rakoto.days) * 2));
  await expect(app.getByTestId("attendance-totals")).toContainText("2 of 2");
});

test("a holiday on the project takes a day off everybody's seeded month", async ({ app }) => {
  await staffedProject(app);
  await fillFromSchedule(app);
  const before = Number((await readRow(app, "Rakoto Randrianasolo")).days);

  // Add a holiday on a weekday of this month, then refill.
  await app.getByRole("button", { name: "Switch project" }).click();
  const modal = await openProjectEditor(app, PROJECT.name);
  await modal.getByLabel("Holiday date").fill(firstMondayOfThisMonth());
  await modal.getByLabel("Holiday name").fill("Site shutdown");
  await modal.getByRole("button", { name: "Add", exact: true }).click();
  await expect(modal.getByTestId("holiday-list")).toContainText("Site shutdown");
  await modal.getByRole("button", { name: "Cancel" }).click();

  await enterExisting(app);
  await fillFromSchedule(app);

  const name = "Rakoto Randrianasolo";
  await expect(attendanceBox(app, name, "Days worked")).toHaveValue(String(before - 1));
  await expect(attendanceBox(app, name, "Hours worked")).toHaveValue(String((before - 1) * 8));
});

/** Re-opens the one project on the picker and goes back to the grid. */
async function enterExisting(page: Page) {
  await page
    .locator(`[data-testid="project-card"][data-project-name="${PROJECT.name}"]`)
    .getByRole("button", { name: "Open project" })
    .click();
  await goToAttendance(page);
}

test("someone hired mid-month is only seeded from the day they started", async ({ app }) => {
  await enterProject(app, PROJECT);
  await addEmployee(app, RAKOTO);

  const midMonth = firstMondayOfThisMonth().replace(/-\d\d$/, "-15");
  await addEmployee(app, {
    firstName: "Nivo",
    lastName: "Rajaonarison",
    role: "Carpenter",
    hireDate: midMonth,
  });

  await goToAttendance(app);
  await fillFromSchedule(app);
  await expect(app.getByTestId("row-blank")).toHaveCount(0);

  const veteran = Number((await readRow(app, "Rakoto Randrianasolo")).days);
  const newcomer = Number((await readRow(app, "Nivo Rajaonarison")).days);

  expect(newcomer).toBeGreaterThan(0);
  expect(newcomer).toBeLessThan(veteran);
});

test("a typed value is saved and flips the row to manual", async ({ app }) => {
  await staffedProject(app);
  await fillFromSchedule(app);

  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "18");

  const row = attendanceRow(app, "Rakoto Randrianasolo");
  await expect(row.getByTestId("row-source")).toHaveText("Manual");
  // Fara's row is untouched, and still says so.
  await expect(attendanceRow(app, "Fara Rasoanaivo").getByTestId("row-source")).toHaveText(
    "Schedule",
  );

  // It came back from SQLite, not from the input box: leave and return.
  await app.getByRole("button", { name: /^Employees/ }).click();
  await goToAttendance(app);
  expect((await readRow(app, "Rakoto Randrianasolo")).days).toBe("18");
});

test("half days survive the round trip", async ({ app }) => {
  await staffedProject(app);
  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "21.5");
  await setAttendance(app, "Rakoto Randrianasolo", "Hours worked", "172");

  await app.getByRole("button", { name: /^Employees/ }).click();
  await goToAttendance(app);

  const row = await readRow(app, "Rakoto Randrianasolo");
  expect(row.days).toBe("21.5");
  expect(row.hours).toBe("172");
  await expect(app.getByTestId("total-days")).toHaveText("21.5");
});

test("the steppers move days by half and hours by one", async ({ app }) => {
  await staffedProject(app);
  const name = "Rakoto Randrianasolo";

  await attendanceRow(app, name).getByLabel(`Increase Days worked for ${name}`).click();
  await expect(attendanceBox(app, name, "Days worked")).toHaveValue("0.5");

  await attendanceRow(app, name).getByLabel(`Increase Overtime for ${name}`).click();
  await expect(attendanceBox(app, name, "Overtime")).toHaveValue("1");

  // Nothing goes below zero.
  await attendanceRow(app, name).getByLabel(`Decrease Days worked for ${name}`).click();
  await attendanceRow(app, name).getByLabel(`Decrease Days worked for ${name}`).click();
  await expect(attendanceBox(app, name, "Days worked")).toHaveValue("0");
});

test("refilling keeps the overtime somebody typed in", async ({ app }) => {
  await staffedProject(app);
  await setAttendance(app, "Rakoto Randrianasolo", "Overtime", "9");

  await fillFromSchedule(app);
  await expect(attendanceBox(app, "Rakoto Randrianasolo", "Days worked")).not.toHaveValue("0");

  const row = await readRow(app, "Rakoto Randrianasolo");
  expect(row.overtime).toBe("9");
  expect(Number(row.days)).toBeGreaterThan(0);
  await expect(app.getByTestId("total-overtime")).toHaveText("9 h");
});

test("a month that cannot hold that many days is rejected on the row", async ({ app }) => {
  await staffedProject(app);

  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "40");

  const error = app.getByTestId("row-error");
  await expect(error).toBeVisible();
  await expect(error).toContainText("days");
  // The rejected value stays on screen to be corrected, and nothing was stored.
  await expect(attendanceBox(app, "Rakoto Randrianasolo", "Days worked")).toHaveValue("40");
  await expect(app.getByTestId("total-days")).toHaveText("0");

  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "20");
  await expect(app.getByTestId("row-error")).toHaveCount(0);
  await expect(app.getByTestId("total-days")).toHaveText("20");
});

test("a month before somebody was hired is refused, naming the hire month", async ({ app }) => {
  await enterProject(app, PROJECT);
  // Hired this month, so any earlier period is impossible for them.
  await addEmployee(app, {
    firstName: "Vola",
    lastName: "Rasoamanana",
    role: "Customs clerk",
    hireDate: firstMondayOfThisMonth(),
  });
  await goToAttendance(app);

  const period = app.getByLabel("Period");
  const sixMonthsBack = await period.locator("option").nth(6).getAttribute("value");
  await period.selectOption(sixMonthsBack!);

  await setAttendance(app, "Vola Rasoamanana", "Days worked", "10");

  await expect(app.getByTestId("row-error")).toContainText("before this employee was hired");
});

test("clearing a month leaves it blank, which is not the same as zero", async ({ app }) => {
  await staffedProject(app);
  await fillFromSchedule(app);
  await expect(app.getByTestId("attendance-totals")).toContainText("2 of 2");

  await attendanceRow(app, "Rakoto Randrianasolo").getByRole("button", { name: "Clear" }).click();

  await expect(attendanceRow(app, "Rakoto Randrianasolo").getByTestId("row-blank")).toBeVisible();
  await expect(app.getByTestId("attendance-totals")).toContainText("1 of 2");

  // Recording zero is a different state: the row exists and counts as recorded.
  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "0");
  await expect(app.getByTestId("attendance-totals")).toContainText("2 of 2");
  await expect(app.getByTestId("total-days")).toHaveText(/^\d+$/);
});

test("each month keeps its own numbers", async ({ app }) => {
  await staffedProject(app);
  const period = app.getByLabel("Period");

  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "20");
  await expect(app.getByTestId("total-days")).toHaveText("20");

  const previous = await period.locator("option").nth(1).getAttribute("value");
  await period.selectOption(previous!);
  await expect(app.getByTestId("total-days")).toHaveText("0");
  await expect(attendanceBox(app, "Rakoto Randrianasolo", "Days worked")).toHaveValue("");

  await setAttendance(app, "Rakoto Randrianasolo", "Days worked", "15");
  await expect(app.getByTestId("total-days")).toHaveText("15");

  const current = await period.locator("option").nth(0).getAttribute("value");
  await period.selectOption(current!);
  await expect(attendanceBox(app, "Rakoto Randrianasolo", "Days worked")).toHaveValue("20");
});
