import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  fakeBundle,
  fakeExtensionsState,
  fakeSkill,
  sampleExtensionInfo,
  setPickedPaths,
} from "@/test/fakeExtensions";
import { renderApp } from "@/test/renderApp";
import { missingRequired } from "./fields";
import { isExtensionFile, useExtensionDrop } from "./store";

async function openInstall(path = "C:/Downloads/weather.mcpb") {
  setPickedPaths("extension", [path]);
  fireEvent.click(await screen.findByRole("button", { name: "Install extension…" }));
  return screen.findByRole("dialog", { name: /Install Weather|Update Weather/ });
}

describe("Settings → Extensions", () => {
  it("installs a bundle with its settings, keeping the key out of the values", async () => {
    renderApp("/settings/extensions");
    expect(await screen.findByText("No extensions yet.")).toBeInTheDocument();
    const dialog = await openInstall();
    expect(fakeExtensionsState().picks).toEqual([{ kind: "extension", multiple: false }]);
    expect(within(dialog).getByText("Forecasts for any city")).toBeInTheDocument();
    expect(within(dialog).getByText(/Version 1\.0\.0 · by Ann Example/)).toBeInTheDocument();
    expect(within(dialog).getByText("forecast")).toBeInTheDocument();
    // Defaults fill the form; the required key blocks Install until it's typed.
    expect(within(dialog).getByLabelText("Units")).toHaveValue("metric");
    expect(within(dialog).getByLabelText("Days")).toHaveValue(3);
    const install = within(dialog).getByRole("button", { name: "Install" });
    expect(install).toBeDisabled();
    expect(within(dialog).getByText("Fill in: API key")).toBeInTheDocument();
    const key = within(dialog).getByLabelText(/API key/);
    expect(key).toHaveAttribute("type", "password");
    fireEvent.change(key, { target: { value: "sk-abc" } });

    // An out-of-range number comes back as an error and the dialog stays.
    fireEvent.change(within(dialog).getByLabelText("Days"), { target: { value: "9" } });
    fireEvent.click(install);
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("at most 7");
    fireEvent.change(within(dialog).getByLabelText("Days"), { target: { value: "5" } });

    // A folder list: typed paths are added and removable.
    const typed = within(dialog).getByLabelText("Type a path for Folders");
    fireEvent.change(typed, { target: { value: "C:/Docs" } });
    fireEvent.keyDown(typed, { key: "Enter" });
    expect(within(dialog).getByText("C:/Docs")).toBeInTheDocument();

    fireEvent.click(install);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Install/ })).toBeNull());
    expect(await screen.findByText("Weather is installed.")).toBeInTheDocument();
    const ext = fakeExtensionsState().installed[0];
    expect(ext?.values).toEqual({ units: "metric", days: 5, folders: ["C:/Docs"] });
    expect(ext?.secretsSet).toEqual(["api_key"]);
    expect(fakeExtensionsState().secrets.get("weather/api_key")).toBe("sk-abc");

    const list = await screen.findByRole("list", { name: "Installed extensions" });
    expect(within(list).getByText("Weather")).toBeInTheDocument();
    expect(within(list).getByText("1.0.0 · Ann Example")).toBeInTheDocument();
  });

  it("cancelling the install dialog discards the opened bundle", async () => {
    renderApp("/settings/extensions");
    const dialog = await openInstall();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(fakeExtensionsState().discarded).toEqual(["stage-1"]));
    expect(fakeExtensionsState().installed).toHaveLength(0);
  });

  it("shows warnings, then configures, switches off and uninstalls", async () => {
    renderApp("/settings/extensions");
    fakeBundle("C:/mac.mcpb", sampleExtensionInfo({ platforms: ["darwin"], fields: [] }));
    const dialog = await openInstall("C:/mac.mcpb");
    expect(within(dialog).getByRole("alert")).toHaveTextContent("made for macOS");
    expect(within(dialog).queryByRole("region", { name: "Extension settings" })).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(fakeExtensionsState().installed).toHaveLength(1));

    // Reinstalling an installed name is an update.
    const update = await openInstall("C:/mac.mcpb");
    expect(within(update).getByRole("heading", { name: "Update Weather" })).toBeInTheDocument();
    expect(within(update).getByText(/Version 1\.0\.0 → 1\.0\.0/)).toBeInTheDocument();
    fireEvent.click(within(update).getByRole("button", { name: "Cancel" }));

    const toggle = await screen.findByRole("switch", { name: "Turn Weather off" });
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeExtensionsState().installed[0]?.enabled).toBe(false));

    fireEvent.click(screen.getByRole("button", { name: "Folder" }));
    expect(fakeExtensionsState().opened).toEqual(["weather"]);

    fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    const confirm = await screen.findByRole("dialog", { name: "Uninstall Weather?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Uninstall" }));
    expect(await screen.findByText("No extensions yet.")).toBeInTheDocument();
  });

  it("edits settings later, keeping a stored key unless it's retyped", async () => {
    renderApp("/settings/extensions");
    const dialog = await openInstall();
    fireEvent.change(within(dialog).getByLabelText(/API key/), { target: { value: "first" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    const settings = await screen.findByRole("dialog", { name: "Weather settings" });
    const key = within(settings).getByLabelText(/API key/);
    expect(key).toHaveValue("");
    expect(key).toHaveAttribute("placeholder", "Saved in the keychain. Type to replace it.");
    // Saving works without retyping the key.
    fireEvent.change(within(settings).getByLabelText("Units"), { target: { value: "imperial" } });
    fireEvent.click(within(settings).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved.")).toBeInTheDocument();
    expect(fakeExtensionsState().installed[0]?.values.units).toBe("imperial");
    expect(fakeExtensionsState().secrets.get("weather/api_key")).toBe("first");
  });

  it("opens a bundle handed over by a drop", async () => {
    act(() => useExtensionDrop.getState().request("D:/drop/weather.mcpb"));
    renderApp("/settings/extensions");
    expect(await screen.findByRole("dialog", { name: "Install Weather" })).toBeInTheDocument();
    expect(useExtensionDrop.getState().pending).toBeNull();
  });

  it("knows bundle files and missing required settings", () => {
    expect(isExtensionFile("C:/x/A.MCPB")).toBe(true);
    expect(isExtensionFile("old.dxt")).toBe(true);
    expect(isExtensionFile("notes.md")).toBe(false);
    const fields = sampleExtensionInfo().fields;
    expect(missingRequired(fields, {}, [])).toEqual(["API key"]);
    expect(missingRequired(fields, {}, ["api_key"])).toEqual([]);
    expect(missingRequired(fields, { api_key: "x" }, [])).toEqual([]);
  });
});

describe("Settings → Skills", () => {
  it("writes a skill, edits it, switches it off and removes it", async () => {
    renderApp("/settings/skills");
    expect(await screen.findByText("No skills yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New skill" }));
    const dialog = await screen.findByRole("dialog", { name: "New skill" });
    const create = within(dialog).getByRole("button", { name: "Create skill" });
    expect(create).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Haiku Writer" } });
    fireEvent.change(within(dialog).getByLabelText("Description"), {
      target: { value: "Use when the user asks for a haiku" },
    });
    fireEvent.change(within(dialog).getByLabelText("Instructions (Markdown)"), {
      target: { value: "Three lines: 5, 7, 5." },
    });
    fireEvent.click(create);
    expect(await screen.findByText("Created the skill “haiku-writer”.")).toBeInTheDocument();
    const list = await screen.findByRole("list", { name: "Skills" });
    expect(within(list).getByText("Use when the user asks for a haiku")).toBeInTheDocument();

    fireEvent.click(within(list).getByRole("button", { name: "View and edit" }));
    const edit = await screen.findByRole("dialog", { name: "haiku-writer" });
    expect(within(edit).getByLabelText("Instructions (Markdown)")).toHaveValue(
      "Three lines: 5, 7, 5.",
    );
    fireEvent.change(within(edit).getByLabelText("Instructions (Markdown)"), {
      target: { value: "Three lines. Add a season word." },
    });
    fireEvent.click(within(edit).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Skill saved.")).toBeInTheDocument();
    expect(fakeExtensionsState().skills[0]?.body).toBe("Three lines. Add a season word.");

    fireEvent.click(within(list).getByRole("switch", { name: "Turn haiku-writer off" }));
    await waitFor(() => expect(fakeExtensionsState().skills[0]?.skill.enabled).toBe(false));

    fireEvent.click(within(list).getByRole("button", { name: "Remove" }));
    const confirm = await screen.findByRole("dialog", { name: "Remove haiku-writer?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Remove" }));
    expect(await screen.findByText("No skills yet.")).toBeInTheDocument();
  });

  it("adds skills from a folder or zip and shows their files", async () => {
    renderApp("/settings/skills");
    fakeSkill("brand", "Our colours", "Teal.", ["colors.md"]);
    setPickedPaths("skillArchive", ["C:/Downloads/pdf-tools.skill"]);
    fireEvent.click(await screen.findByRole("button", { name: "Add .zip or .skill…" }));
    expect(await screen.findByText("Added the skill “pdf-tools”.")).toBeInTheDocument();
    const list = await screen.findByRole("list", { name: "Skills" });
    expect(within(list).getByText("pdf-tools")).toBeInTheDocument();
    expect(within(list).getByText("brand")).toBeInTheDocument();

    // The same one again is refused with the backend's reason.
    setPickedPaths("folder", ["C:/Downloads/pdf-tools"]);
    fireEvent.click(screen.getByRole("button", { name: "Add folder…" }));
    expect(await screen.findByText(/already a skill called "pdf-tools"/)).toBeInTheDocument();

    const brand = within(list).getByText("brand").closest("li") as HTMLElement;
    expect(within(brand).getByText("+ 1 file")).toBeInTheDocument();
    fireEvent.click(within(brand).getByRole("button", { name: "View and edit" }));
    const dialog = await screen.findByRole("dialog", { name: "brand" });
    expect(within(dialog).getByText("colors.md")).toBeInTheDocument();
  });
});
