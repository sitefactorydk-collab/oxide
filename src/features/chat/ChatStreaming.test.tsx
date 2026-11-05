import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { emitConversationUpdated, fakeState, finishReply, setReplyMode } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

const sidebar = () => screen.getByRole("complementary", { name: "Sidebar" });

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

describe("streaming replies", () => {
  it("shows the reply as it streams, with Stop, then the saved message", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Warmer, please");
    const live = await screen.findByLabelText("Reply in progress");
    expect(await within(live).findByText("Partial")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();

    act(() => finishReply("c2"));
    expect(await screen.findByText("Partial Reply to: Warmer, please")).toBeInTheDocument();
    expect(screen.queryByLabelText("Reply in progress")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
  });

  it("Stop keeps what was written and says so", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Long answer please");
    await screen.findByLabelText("Reply in progress");
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(await screen.findByText("Stopped.")).toBeInTheDocument();
    const saved = fakeState().messages.at(-1);
    expect(saved?.stopReason).toBe("stopped");
  });

  it("shows a failed reply with Retry, which answers the same message again", async () => {
    renderApp("/chat/c2", { backend: { replies: "fail" } });
    await screen.findByText(/warm neutral palette/);
    await send("Are you there?");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("LM Studio: couldn't reach");
    setReplyMode("auto");
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Reply to: Are you there?")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("Retry on a reply writes a new one in its place", async () => {
    renderApp("/chat/c2");
    const old = await screen.findByText(/iron-oxide accent/);
    const reply = old.closest(".group") as HTMLElement;
    fireEvent.click(within(reply).getByRole("button", { name: "Retry" }));
    expect(
      await screen.findByText("Reply to: Suggest a warm neutral palette with a rust accent."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/iron-oxide accent/)).not.toBeInTheDocument();
    // The old reply is kept as a sibling branch.
    expect(fakeState().messages.some((m) => m.id === "m6")).toBe(true);
  });

  it("sends the extended-thinking choice with the message", async () => {
    renderApp("/chat/c2", { ui: { thinking: true } });
    await screen.findByText(/warm neutral palette/);
    await send("Think hard");
    await screen.findByText("Reply to: Think hard");
    expect(fakeState().replyOptions.at(-1)).toEqual({ thinking: true });
  });

  it("applies automatic titles that arrive in the background", async () => {
    renderApp("/chat/c4");
    await within(sidebar()).findByRole("link", { name: "Explain Rust lifetimes simply" });
    const c4 = fakeState().conversations.find((c) => c.id === "c4");
    if (!c4) throw new Error("seed chat missing");
    act(() => emitConversationUpdated({ ...c4, title: "Rust lifetimes" }));
    expect(
      await within(sidebar()).findByRole("link", { name: "Rust lifetimes" }),
    ).toBeInTheDocument();
  });
});

describe("model picker", () => {
  const openPicker = async (name: string) => {
    const trigger = await screen.findByRole("button", { name: `Model: ${name}` });
    fireEvent.keyDown(trigger, { key: "Enter" });
    return screen.findByRole("menu");
  };

  it("lists models per endpoint, hides ones without tools, and shows endpoint errors", async () => {
    renderApp("/chat/c2");
    const menu = await openPicker("Claude Opus 5.5");
    const anthropic = within(menu).getByRole("group", { name: "Anthropic" });
    expect(within(anthropic).getAllByRole("menuitemradio")).toHaveLength(3);
    expect(within(menu).getByText(/No API key yet/)).toBeInTheDocument();
    expect(within(menu).queryByText("gemma-3")).not.toBeInTheDocument();
  });

  it("changes the chat's model", async () => {
    renderApp("/chat/c2");
    const menu = await openPicker("Claude Opus 5.5");
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: /qwen\/qwen3-14b/ }));
    await waitFor(() =>
      expect(fakeState().conversations.find((c) => c.id === "c2")?.model).toBe(
        "lmstudio/qwen/qwen3-14b",
      ),
    );
    expect(
      await screen.findByRole("button", { name: "Model: qwen/qwen3-14b" }),
    ).toBeInTheDocument();
  });

  it("shows models without tools greyed out when Settings says so", async () => {
    renderApp("/settings/models");
    const toggle = await screen.findByRole("switch", { name: "Show models without tool calling" });
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeState().settings.models.showUnsupported).toBe(true));
    const lm = await screen.findByRole("region", { name: "LM Studio models" });
    expect(within(lm).getByText("No tools")).toBeInTheDocument();
  });

  it("disables extended thinking for models that can't think", async () => {
    renderApp("/chat/c5", { ui: { thinking: true } });
    const toggle = await screen.findByRole("button", { name: "Extended thinking" });
    // Until the model list arrives, any model may think; then this one can't.
    await waitFor(() => expect(toggle).toBeDisabled());
    expect(toggle).toHaveAttribute("aria-pressed", "false");
  });
});
