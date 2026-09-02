import { expect, type Page } from "@playwright/test";

export interface NewProject {
  name: string;
  client?: string;
  location?: string;
  status?: "active" | "paused" | "closed";
  start?: string;
  end?: string;
}

export async function createProject(page: Page, project: NewProject) {
  await page.getByRole("button", { name: "New project" }).click();
  const modal = page.getByTestId("project-modal");
  await expect(modal).toBeVisible();

  await modal.getByLabel("Project name").fill(project.name);
  if (project.client) await modal.getByLabel("Client").fill(project.client);
  if (project.location) await modal.getByLabel("Location").fill(project.location);
  if (project.status) await modal.getByLabel("Status").selectOption(project.status);
  if (project.start) await modal.getByLabel("Start date").fill(project.start);
  if (project.end) await modal.getByLabel("End date").fill(project.end);

  await modal.getByRole("button", { name: "Create project" }).click();
  await expect(modal).toBeHidden();
}

export function projectCard(page: Page, name: string) {
  return page.locator(`[data-testid="project-card"][data-project-name="${name}"]`);
}

export async function openProjectEditor(page: Page, name: string) {
  await projectCard(page, name).getByRole("button", { name: "Edit" }).click();
  const modal = page.getByTestId("project-modal");
  await expect(modal).toBeVisible();
  return modal;
}

/** Creates a project and steps into its workspace. */
export async function enterProject(page: Page, project: NewProject) {
  await createProject(page, project);
  await projectCard(page, project.name).getByRole("button", { name: "Open project" }).click();
  await expect(page.getByRole("navigation", { name: "Project sections" })).toBeVisible();
}

export interface NewEmployee {
  firstName: string;
  lastName: string;
  role: string;
  phone?: string;
  email?: string;
  address?: string;
  cin?: string;
  birthDate?: string;
  hireDate?: string;
  bankAccount?: string;
  emergencyContact?: string;
}

export async function addEmployee(page: Page, employee: NewEmployee) {
  await page.getByRole("button", { name: "Add employee" }).click();
  const modal = page.getByTestId("employee-modal");
  await expect(modal).toBeVisible();
  await fillEmployee(modal, employee);
  await modal.getByRole("button", { name: "Add employee" }).click();
  await expect(modal).toBeHidden();
}

type Modal = ReturnType<Page["getByTestId"]>;

export async function fillEmployee(modal: Modal, employee: Partial<NewEmployee>) {
  const fields: Array<[keyof NewEmployee, string]> = [
    ["firstName", "First name"],
    ["lastName", "Last name"],
    ["role", "Role / job title"],
    ["phone", "Phone"],
    ["email", "Email"],
    ["address", "Address"],
    ["cin", "National ID (CIN)"],
    ["birthDate", "Date of birth"],
    ["hireDate", "Hire date"],
    ["bankAccount", "Bank account"],
    ["emergencyContact", "Emergency contact"],
  ];
  for (const [key, label] of fields) {
    const value = employee[key];
    if (value !== undefined) await modal.getByLabel(label, { exact: true }).fill(value);
  }
}

export function employeeRow(page: Page, name: string) {
  return page.locator(`[data-testid="employee-row"][data-employee-name="${name}"]`);
}

/** The value of one label/value row on the employee file. */
export function detail(page: Page, label: string) {
  return page.locator(`[data-detail="${label}"] .details__value`);
}

/** Steps into the Time & attendance grid of the project already open. */
export async function goToAttendance(page: Page) {
  await page.getByRole("button", { name: /^Time & attendance/ }).click();
  await expect(page.getByTestId("attendance-grid")).toBeVisible();
}

export function attendanceRow(page: Page, name: string) {
  return page.locator(`[data-testid="attendance-row"][data-employee-name="${name}"]`);
}

/** The three number boxes on one attendance row. */
export function attendanceBox(page: Page, name: string, field: string) {
  return attendanceRow(page, name).getByLabel(`${field} for ${name}`, { exact: true });
}

/** Types a value into one box and commits it by blurring. */
export async function setAttendance(page: Page, name: string, field: string, value: string) {
  const box = attendanceBox(page, name, field);
  await box.fill(value);
  await box.blur();
}

/**
 * Clicks "Fill from standard schedule" and waits for it to land.
 *
 * The button starts an async round trip; the toast is raised in the same
 * render as the new grid, so it is the signal that the boxes now hold the
 * seeded values rather than the previous ones.
 */
export async function fillFromSchedule(page: Page) {
  await page.getByRole("button", { name: "Fill from standard schedule" }).click();
  await expect(page.getByTestId("toast")).toContainText("Filled");
}

