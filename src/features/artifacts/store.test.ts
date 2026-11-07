import { describe, expect, it } from "vitest";
import { PANEL_MIN, clampPanelWidth, useArtifactPanel } from "./store";

const s = () => useArtifactPanel.getState();

describe("the artifact panel", () => {
  it("follows a live call into the saved artifact", () => {
    s().openLive("c1", "call1");
    expect(s().target).toEqual({
      conversationId: "c1",
      artifactId: null,
      version: null,
      liveCallId: "call1",
    });
    expect(s().view).toBe("code");
    s().settle("c1", "call1", "a1", 1);
    expect(s().target).toEqual({
      conversationId: "c1",
      artifactId: "a1",
      version: 1,
      liveCallId: null,
    });
    expect(s().view).toBe("preview");
  });

  it("doesn't reopen a call the user closed, unless asked", () => {
    s().openLive("c1", "call1");
    s().close();
    s().openLive("c1", "call1");
    expect(s().target).toBeNull();
    s().settle("c1", "call1", "a1", 1);
    expect(s().target).toBeNull();
    s().openLive("c1", "call1", true);
    expect(s().target?.liveCallId).toBe("call1");
  });

  it("opens new artifacts when closed, and keeps another artifact on screen", () => {
    s().settle("c1", "call2", "a2", 1);
    expect(s().target?.artifactId).toBe("a2");
    s().settle("c1", "call3", "a3", 1);
    expect(s().target?.artifactId).toBe("a2");
    // An update of the artifact on screen shows its new version.
    s().setVersion(1);
    s().settle("c1", "call4", "a2", 2);
    expect(s().target?.version).toBe(2);
  });

  it("keeps a sensible width", () => {
    expect(clampPanelWidth(10)).toBe(PANEL_MIN);
    expect(clampPanelWidth(900, 1000)).toBe(620);
    expect(clampPanelWidth(Number.NaN)).toBe(560);
    s().setWidth(12);
    expect(s().width).toBe(PANEL_MIN);
  });
});
