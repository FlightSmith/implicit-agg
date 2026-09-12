import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("design-tree")).toContainText("main-wing", {
    timeout: 15_000,
  });
});

async function selectStation(page: import("@playwright/test").Page, id: string) {
  await page.locator(".tree-station").filter({ hasText: id }).first().click();
  await expect(page.getByRole("heading", { level: 3 })).toContainText(id);
}

test("renders the demo wing with report metrics", async ({ page }) => {
  const report = page.getByTestId("report");
  await expect(report).toContainText("reference area");
  await expect(report).toContainText("16.8370");
  await expect(report).toContainText("14.6000");
  await expect(report).toContainText("12.6602");
  await expect(page.getByTestId("viewport")).toContainText("triangles");
});

test("converting a bound chord to a literal edits live, with undo", async ({ page }) => {
  await selectStation(page, "kink");

  // Kink chord arrives bound to wing.kinkChord; switch to a literal.
  const chordField = page.locator(".field").filter({ hasText: "chord" }).first();
  await chordField.getByRole("button", { name: "123", exact: true }).click();
  await chordField.getByRole("button", { name: /convert to literal/ }).click();

  const chordInput = page.getByTestId("input-chord");
  await expect(chordInput).toHaveValue(/1\.15/);
  await chordInput.fill("1.6");
  await chordInput.blur();
  await expect(page.getByTestId("report")).toContainText("20.1220", {
    timeout: 5_000,
  });

  // First undo reverts the 1.6 edit; second undo reverts the conversion
  // itself, restoring the wing.kinkChord binding.
  await page.getByTestId("undo").click();
  await expect(page.getByTestId("input-chord")).toHaveValue(/1\.15/);
  await expect(page.getByTestId("report")).toContainText("16.8370");
  await page.getByTestId("undo").click();
  await expect(page.locator(".field").filter({ hasText: "chord" }).first()).toContainText(
    "wing.kinkChord",
  );
  await expect(page.getByTestId("report")).toContainText("16.8370");
});

test("a dimension error is reported and the last valid state is kept", async ({ page }) => {
  await selectStation(page, "kink");

  // Bind chord to an angle-valued expression: a dimension mismatch.
  const chordField = page.locator(".field").filter({ hasText: "chord" }).first();
  await chordField.getByRole("button", { name: "fx", exact: true }).click();
  const expression = page.getByTestId("expression-chord");
  await expression.fill("= @param.wing.rootTwist + 1");
  await expression.blur();

  await expect(page.getByTestId("diagnostics")).toContainText("dimension-mismatch", {
    timeout: 5_000,
  });
  // The last valid geometry is kept: the report still shows the baseline.
  await page.getByTestId("tab-report").click();
  await expect(page.getByTestId("report")).toContainText("16.8370");
});

test("half and full models differ in triangle count", async ({ page }) => {
  const viewport = page.getByTestId("viewport");
  await expect(viewport).toContainText("838 triangles");
  await page.getByTestId("view-half").click();
  await expect(viewport).toContainText("464 triangles", { timeout: 5_000 });
  await expect(viewport).toContainText("half model");
});

test("clicking the mesh traces the picked panel to its stations", async ({ page }) => {
  const canvas = page.locator("canvas.viewport-canvas");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas has no bounding box");
  // The camera targets the model center, so the wing spans the canvas
  // center; sweep neighbors in case of aspect-ratio drift.
  const candidates = [
    [0.5, 0.5],
    [0.55, 0.6],
    [0.45, 0.65],
    [0.6, 0.68],
  ];
  let traced = false;
  for (const [fx, fy] of candidates) {
    await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
    try {
      await expect(page.getByTestId("trace")).toContainText(
        "loft panel between stations",
        { timeout: 1_500 },
      );
      traced = true;
      break;
    } catch {
      // Missed the mesh; try the next candidate.
    }
  }
  expect(traced, "a click should have hit the wing and produced a trace").toBe(true);
});

test("export STL produces a download", async ({ page }) => {
  const downloadPromise = page.waitForEvent("download");
  await page.getByTestId("export-stl").click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("wing.stl");
});

test("typing in the number input updates the geometry without blur", async ({ page }) => {
  await selectStation(page, "kink");
  const chordField = page.locator(".field").filter({ hasText: "chord" }).first();
  await chordField.getByRole("button", { name: "123", exact: true }).click();
  await chordField.getByRole("button", { name: /convert to literal/ }).click();
  const input = page.getByTestId("input-chord");
  await expect(input).toHaveValue(/1\.15/);
  // fill() fires only a change event — no blur — and the geometry must follow.
  await input.fill("1.6");
  await expect(page.getByTestId("report")).toContainText("20.1220", { timeout: 5_000 });
});

test("slider edits round to four fraction digits", async ({ page }) => {
  await selectStation(page, "kink");
  const chordField = page.locator(".field").filter({ hasText: "chord" }).first();
  await chordField.getByRole("button", { name: "123", exact: true }).click();
  await chordField.getByRole("button", { name: /convert to literal/ }).click();
  const slider = page.getByTestId("slider-chord");
  await slider.focus();
  // Arrow keys move by the slider step; native range inputs accumulate
  // binary floating noise, which the four-digit rounding must strip.
  await slider.press("ArrowRight");
  await page.waitForTimeout(300);
  const applied = await page.getByTestId("input-chord").inputValue();
  expect(applied).toMatch(/^\d+\.\d{1,4}$/);
  expect(Number(applied)).toBeGreaterThan(1.15);
});

test("typed values clamp to the catalog safety bounds", async ({ page }) => {
  await selectStation(page, "kink");
  const chordField = page.locator(".field").filter({ hasText: "chord" }).first();
  await chordField.getByRole("button", { name: "123", exact: true }).click();
  await chordField.getByRole("button", { name: /convert to literal/ }).click();
  const input = page.getByTestId("input-chord");
  await input.fill("9999");
  // The catalog caps wing-section chords at 100 document units.
  await expect(input).toHaveValue(/^100$/, { timeout: 5_000 });
});
