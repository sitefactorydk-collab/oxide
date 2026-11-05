import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@/bindings", () => ({
  commands: {
    ping: vi.fn().mockResolvedValue({
      message: "pong, Oxide",
      appName: "Oxide",
      appVersion: "0.1.0",
      coreVersion: "0.1.0",
    }),
  },
}));

describe("App", () => {
  it("shows the app name", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Oxide" })).toBeInTheDocument();
  });

  it("shows the ping result from the backend", async () => {
    render(<App />);
    expect(await screen.findByText(/pong, Oxide · v0\.1\.0/)).toBeInTheDocument();
  });
});
