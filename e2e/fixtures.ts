import { test as base, expect, type Page } from "@playwright/test";

export const DEVSERVER = "http://127.0.0.1:4599";

/**
 * Stands in for Tauri's IPC bridge.
 *
 * `@tauri-apps/api` calls `window.__TAURI_INTERNALS__.invoke(cmd, args)`, so
 * defining that before the app boots is enough to redirect every command at
 * the Rust dev server. Deliberately the only thing this fixture knows how to
 * do — there is no mock backend here, and no business rule is restated.
 */
async function bridgeToRust(page: Page) {
  await page.addInitScript(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {
        async invoke(command: string, args?: Record<string, unknown>) {
          const response = await fetch(`/ipc/${command}`, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(args ?? {}),
          });
          const payload: unknown = await response.json();
          if (!response.ok) throw payload; // the AppError shape, as Tauri delivers it
          return payload;
        },
        transformCallback: (callback: unknown) => callback,
      },
      writable: false,
    });
  });
}

export const test = base.extend<{ app: Page }>({
  app: async ({ page, request }, use) => {
    // Each test starts against an empty, freshly migrated database.
    await request.post(`${DEVSERVER}/ipc/__reset`, { data: {} });
    await bridgeToRust(page);
    await page.goto("/");
    await use(page);
  },
});

export { expect };

/** The first Monday of the current month — always a working day under Mon–Fri. */
export function firstMondayOfThisMonth(): string {
  const now = new Date();
  const day = new Date(now.getFullYear(), now.getMonth(), 1);
  while (day.getDay() !== 1) day.setDate(day.getDate() + 1);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
}

/** A date `months` months from the first of the current month. */
export function monthsFromNow(months: number): string {
  const now = new Date();
  const day = new Date(now.getFullYear(), now.getMonth() + months, 1);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
}
