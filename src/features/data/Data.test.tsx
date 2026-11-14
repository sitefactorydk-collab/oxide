import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeEvents, fakeState } from "@/test/fakeBackend";
import {
  fakeDataState,
  setFakeExportCancelled,
  setFakeUpdate,
  setFakeUpdateStatus,
} from "@/test/fakeData";
import { renderApp } from "@/test/renderApp";

describe("Settings → Data", () => {
  it("shows where data lives and exports everything into a zip", async () => {
    renderApp("/settings/data");
    expect(
      await screen.findByText("C:\\Users\\me\\AppData\\Roaming\\dev.oxide.app"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("data-size")).toHaveTextContent("50");
    fireEvent.click(screen.getByRole("button", { name: "Show folder" }));
    await waitFor(() => expect(fakeDataState().opens).toBe(1));

    fireEvent.click(screen.getByRole("button", { name: "Export all…" }));
    const done = await screen.findByText(/24 tables \(1312 rows\) and 37 files/);
    expect(done).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show in folder" }));
    await waitFor(() => expect(fakeDataState().reveals).toBe(1));
  });

  it("does nothing when the save dialog is cancelled", async () => {
    renderApp("/settings/data");
    setFakeExportCancelled(true);
    fireEvent.click(await screen.findByRole("button", { name: "Export all…" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Export all…" })).toBeEnabled());
    expect(screen.queryByText(/tables \(/)).not.toBeInTheDocument();
    expect(fakeDataState().exports).toBe(0);
  });

  it("deletes everything only after the word is typed, then clears the window's storage", async () => {
    renderApp("/settings/data");
    localStorage.setItem("leftover", "x");
    fireEvent.click(await screen.findByRole("button", { name: "Delete all data…" }));
    const confirm = await screen.findByRole("button", { name: "Delete everything" });
    expect(confirm).toBeDisabled();
    const input = screen.getByRole("textbox", { name: "Type delete to confirm" });
    fireEvent.change(input, { target: { value: "dele" } });
    expect(confirm).toBeDisabled();
    fireEvent.change(input, { target: { value: "Delete" } });
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(await screen.findByText(/Oxide is restarting/)).toBeInTheDocument();
    expect(fakeDataState().deletes).toEqual(["Delete"]);
    expect(localStorage.getItem("leftover")).toBeNull();
  });
});

describe("Settings → About → Updates", () => {
  it("checks, finds a version and installs it", async () => {
    renderApp("/settings/about");
    expect(await screen.findByText("Version 0.1.0. Not checked yet.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Version 0.1.0 is the newest.")).toBeInTheDocument();

    setFakeUpdate({ version: "0.2.0", notes: "Faster replies." });
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(
      await screen.findByText("Oxide 0.2.0 is available (you have 0.1.0)."),
    ).toBeInTheDocument();
    expect(screen.getByText("Faster replies.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Install 0.2.0 and restart" }));
    expect(await screen.findByText(/Installing\. Oxide restarts/)).toBeInTheDocument();
    expect(fakeDataState().installs).toBe(1);
  });

  it("shows a failed check and the automatic switch saves", async () => {
    renderApp("/settings/about");
    setFakeUpdate(null, "Couldn't reach the update server (offline).");
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't reach the update server");
    const auto = screen.getByRole("switch", { name: "Check for updates automatically" });
    expect(auto).toHaveAttribute("aria-checked", "true");
    fireEvent.click(auto);
    await waitFor(() => expect(fakeState().settings.updates.checkAutomatically).toBe(false));
  });

  it("says when a build doesn't look for updates", async () => {
    renderApp("/settings/about");
    await screen.findByText("Version 0.1.0. Not checked yet.");
    setFakeUpdateStatus({ configured: false });
    await act(async () => {
      await fakeEvents.updateChanged.emit({
        currentVersion: "0.1.0",
        configured: false,
        canInstall: true,
        phase: "idle",
        available: null,
        downloaded: 0,
        total: null,
        error: null,
        checkedAt: null,
        note: "This build doesn't look for updates. Installers built by the release workflow do.",
      });
    });
    expect(await screen.findByText(/This build doesn't look for updates/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
  });

  it("tells the user once when the background check finds a version", async () => {
    renderApp("/new");
    await screen.findByRole("main");
    const status = {
      currentVersion: "0.1.0",
      configured: true,
      canInstall: true,
      phase: "available" as const,
      available: { version: "0.3.0", date: null, notes: null },
      downloaded: 0,
      total: null,
      error: null,
      checkedAt: 1,
      note: null,
    };
    await act(async () => {
      await fakeEvents.updateChanged.emit(status);
      await fakeEvents.updateChanged.emit(status);
    });
    expect(await screen.findAllByText(/Oxide 0\.3\.0 is available/)).toHaveLength(1);
  });
});
