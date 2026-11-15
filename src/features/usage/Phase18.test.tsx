import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { resolveLanguage, translate } from "@/i18n";
import { da } from "@/i18n/da";
import { en } from "@/i18n/en";
import { fakeState } from "@/test/fakeBackend";
import { fakeInsights, setFakeTranscript } from "@/test/fakeInsights";
import { renderApp } from "@/test/renderApp";
import { compactNumber } from "./format";

describe("interface language", () => {
  it("follows the system for Danish and English, and fills placeholders", () => {
    expect(resolveLanguage("system", "da-DK")).toBe("da");
    expect(resolveLanguage("system", "da")).toBe("da");
    expect(resolveLanguage("system", "en-US")).toBe("en");
    expect(resolveLanguage("system", "nb-NO")).toBe("en");
    expect(resolveLanguage("da", "en-US")).toBe("da");
    expect(resolveLanguage("en", "da-DK")).toBe("en");
    expect(translate("da", "home.startFailed", { error: "x" })).toBe(
      "Chatten kunne ikke startes: x",
    );
    expect(Object.keys(da).sort()).toEqual(Object.keys(en).sort());
  });

  it("shows the shell in Danish when picked in Settings → Appearance", async () => {
    renderApp("/settings/appearance");
    const group = await screen.findByRole("radiogroup", { name: "Language" });
    fireEvent.click(within(group).getByRole("radio", { name: "Dansk" }));
    const sidebar = await screen.findByRole("complementary", { name: "Sidepanel" });
    expect(within(sidebar).getByRole("link", { name: /Ny chat/ })).toBeInTheDocument();
    expect(within(sidebar).getByText("Projekter")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Afsnit i indstillinger" })).toHaveTextContent(
      "Udseende",
    );
    expect(document.documentElement.lang).toBe("da");
    await waitFor(() => expect(fakeState().settings.appearance.language).toBe("da"));
  });
});

describe("Settings → Usage", () => {
  it("shows totals, a bar per day and tables per provider and model", async () => {
    renderApp("/settings/usage");
    expect(await screen.findByText("175K")).toBeInTheDocument();
    expect(screen.getByText("in 9 chats")).toBeInTheDocument();
    expect(screen.getAllByRole("img", { name: /tokens$/ })).toHaveLength(7);
    const byModel = screen.getByRole("region", { name: "By model" });
    expect(within(byModel).getByText("qwen/qwen3-14b")).toBeInTheDocument();
    expect(screen.getByText(/3 replies came without counts/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "7 days" }));
    await waitFor(() => expect(fakeInsights().usageCalls.at(-1)?.days).toBe(7));
    expect(compactNumber(999)).toBe("999");
    expect(compactNumber(1234)).toBe("1.2K");
    expect(compactNumber(3_400_000)).toBe("3.4M");
  });
});

describe("About", () => {
  it("reports the start-up time and memory, and the window says it's ready", async () => {
    renderApp("/settings/about");
    expect(await screen.findByTestId("about-metrics")).toHaveTextContent("Started in 0.8 s");
    expect(screen.getByTestId("about-metrics")).toHaveTextContent("96 MB in use");
    expect(fakeInsights().ready).toBeGreaterThan(0);
  });
});

describe("dictation", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function stubMicrophone() {
    const track = { stop: vi.fn() };
    const stream = { getTracks: () => [track] };
    vi.stubGlobal("navigator", {
      ...navigator,
      mediaDevices: { getUserMedia: vi.fn(() => Promise.resolve(stream)) },
    });
    class FakeRecorder {
      static isTypeSupported = (m: string) => m === "audio/webm;codecs=opus";
      state = "inactive";
      mimeType = "audio/webm;codecs=opus";
      ondataavailable: ((e: { data: Blob }) => void) | null = null;
      onstop: (() => void) | null = null;
      start() {
        this.state = "recording";
      }
      stop() {
        this.state = "inactive";
        this.ondataavailable?.({ data: new Blob(["voice"], { type: this.mimeType }) });
        this.onstop?.();
      }
    }
    vi.stubGlobal("MediaRecorder", FakeRecorder);
    return track;
  }

  it("records, transcribes and adds the text to the message", async () => {
    const track = stubMicrophone();
    renderApp("/new", { backend: { settings: { dictation: { providerId: "openai" } } } });
    const box = await screen.findByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Note:" } });
    fireEvent.click(screen.getByRole("button", { name: "Dictate" }));
    const stop = await screen.findByRole("button", { name: "Stop dictation" });
    await act(async () => {
      fireEvent.click(stop);
    });
    await waitFor(() => expect(box).toHaveValue("Note: hello from the microphone"));
    expect(track.stop).toHaveBeenCalled();
    expect(fakeInsights().recordings[0]?.mime).toBe("audio/webm;codecs=opus");
    expect(atob(fakeInsights().recordings[0]?.audio ?? "")).toBe("voice");
  });

  it("explains failures", async () => {
    stubMicrophone();
    renderApp("/new", { backend: { settings: { dictation: { providerId: "openai" } } } });
    setFakeTranscript({ error: "Dictation failed: bad key" });
    fireEvent.click(await screen.findByRole("button", { name: "Dictate" }));
    const stop = await screen.findByRole("button", { name: "Stop dictation" });
    await act(async () => {
      fireEvent.click(stop);
    });
    expect(await screen.findByText("Dictation failed: bad key")).toBeInTheDocument();
  });

  it("points to Settings → Voice when no endpoint is set up", async () => {
    const { location } = renderApp("/new");
    fireEvent.click(await screen.findByRole("button", { name: "Dictate" }));
    expect(
      await screen.findByRole("combobox", { name: "Speech-to-text endpoint" }),
    ).toBeInTheDocument();
    expect(location.pathname).toBe("/settings/voice");
  });

  it("is hidden when switched off", async () => {
    renderApp("/new", { backend: { settings: { dictation: { enabled: false } } } });
    await screen.findByRole("textbox", { name: "Message" });
    await waitFor(() => expect(screen.queryByRole("button", { name: "Dictate" })).toBeNull());
  });
});
