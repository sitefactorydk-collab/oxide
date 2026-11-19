import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ContentBlock } from "@/bindings";
import { emitStream, fakeState, finishReplyWith, heldReplyId } from "@/test/fakeBackend";
import { fakeGeneration, setFakeMedia } from "@/test/fakeGeneration";
import { renderApp } from "@/test/renderApp";
import { callHeadline, toolKind, usageRows } from "@/features/tools/toolView";

describe("Settings → Generation", () => {
  it("sets an endpoint, a model and a key per kind, and tests it", async () => {
    renderApp("/settings/generation");
    // Four groups; Image is open, and each header shows its model or "Not set up".
    const image = await screen.findByRole("region", { name: "Image generation" });
    expect(screen.getByRole("region", { name: "Text generation" })).toHaveTextContent("Not set up");
    expect(screen.getByRole("region", { name: "Video generation" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Audio generation" })).toBeInTheDocument();
    expect(within(image).getByRole("button", { expanded: true })).toHaveTextContent("Not set up");

    // Anthropic can't generate media, so it isn't offered.
    const select = within(image).getByRole("combobox", { name: "image endpoint" });
    // Wait for the providers to load before picking one.
    await within(select).findByRole("option", { name: "OpenAI" });
    expect(within(select).queryByRole("option", { name: "Anthropic" })).not.toBeInTheDocument();
    fireEvent.change(select, { target: { value: "openai" } });
    await waitFor(() => expect(fakeState().settings.generation.image.providerId).toBe("openai"));
    expect(within(image).getByRole("button", { expanded: true })).toHaveTextContent("gpt-image-2");

    const model = within(image).getByLabelText("image model");
    fireEvent.change(model, { target: { value: " dall-e-3 " } });
    fireEvent.blur(model);
    await waitFor(() => expect(fakeState().settings.generation.image.model).toBe("dall-e-3"));

    fireEvent.click(within(image).getByRole("button", { name: "List models" }));
    expect(await within(image).findByText(/lists 3 models/)).toBeInTheDocument();

    const key = within(image).getByLabelText("image API key");
    fireEvent.change(key, { target: { value: "sk-image" } });
    fireEvent.click(within(image).getByRole("button", { name: "Save key" }));
    await waitFor(() => expect(fakeGeneration().keys.get("image")).toBe("sk-image"));
    expect(await within(image).findByRole("button", { name: "Remove" })).toBeInTheDocument();
    expect(within(image).getByLabelText("image API key")).toHaveAttribute(
      "placeholder",
      "•••• saved in the keychain",
    );

    fireEvent.click(within(image).getByRole("button", { name: "Test" }));
    expect(await within(image).findByRole("status")).toHaveTextContent(
      "Connected, but dall-e-3 isn't among the models.",
    );
    expect(fakeGeneration().tests.at(-1)?.endpoint.model).toBe("dall-e-3");

    // The audio group has a voice and a format; a custom URL counts as an endpoint.
    fireEvent.click(screen.getByRole("button", { name: /Audio generation/ }));
    const audio = screen.getByRole("region", { name: "Audio generation" });
    const url = within(audio).getByRole("textbox", { name: "audio custom URL" });
    fireEvent.change(url, { target: { value: "http://127.0.0.1:8880/v1" } });
    fireEvent.blur(url);
    await waitFor(() =>
      expect(fakeState().settings.generation.audio.baseUrl).toBe("http://127.0.0.1:8880/v1"),
    );
    fireEvent.change(within(audio).getByRole("combobox", { name: "Audio format" }), {
      target: { value: "wav" },
    });
    await waitFor(() => expect(fakeState().settings.generation.audioFormat).toBe("wav"));
    const voice = within(audio).getByLabelText("Voice");
    fireEvent.change(voice, { target: { value: "nova" } });
    fireEvent.blur(voice);
    await waitFor(() => expect(fakeState().settings.generation.voice).toBe("nova"));
  });
});

describe("generation tools in a reply", () => {
  // jsdom has no object URLs.
  beforeEach(() => {
    URL.createObjectURL = vi.fn(() => "blob:fake");
    URL.revokeObjectURL = vi.fn();
  });

  it("shows a video's progress, then a player and a download button", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    // After renderApp, which resets the fake backend.
    setFakeMedia("/data/attachments/v1.mp4", "AAAAGGZ0eXBtcDQy");
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Make a 5-second video of waves" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    heldReplyId("c2");
    const input = JSON.stringify({ prompt: "Waves at sunset", seconds: 5 });
    act(() => {
      emitStream("c2", { type: "toolUse", id: "v1", name: "generate_video" });
      emitStream("c2", {
        type: "toolCall",
        id: "v1",
        name: "generate_video",
        server: null,
        tool: "generate_video",
        title: null,
        inputJson: input,
      });
      emitStream("c2", { type: "toolRunning", id: "v1" });
      emitStream("c2", {
        type: "toolProgress",
        id: "v1",
        progress: { percent: 42, note: "Making the video" },
      });
    });
    expect(await within(live).findByText("Making the video 42%")).toBeInTheDocument();
    expect(within(live).getByRole("button", { name: /Making a video/ })).toHaveTextContent(
      "Waves at sunset",
    );

    const content: ContentBlock[] = [
      { type: "text", text: "Made a 5-second video with sora-2 (12 B)." },
      {
        type: "document",
        mediaType: "video/mp4",
        name: "generated-video.mp4",
        source: { kind: "path", path: "/data/attachments/v1.mp4" },
      },
    ];
    act(() => {
      emitStream("c2", {
        type: "toolResult",
        id: "v1",
        content,
        isError: false,
        durationMs: 61000,
      });
      finishReplyWith("c2", [
        {
          type: "toolUse",
          id: "v1",
          name: "generate_video",
          inputJson: input,
          server: null,
          tool: "generate_video",
        },
        { type: "toolResult", toolUseId: "v1", content, isError: false },
        { type: "text", text: "Here is your video." },
      ]);
    });
    const reply = (await screen.findByText("Here is your video.")).closest(
      "article",
    ) as HTMLElement;
    expect(within(reply).getByRole("button", { name: /Made a video/ })).toBeInTheDocument();
    const player = await within(reply).findByLabelText("generated-video.mp4");
    expect(player.tagName).toBe("VIDEO");
    expect(player).toHaveAttribute("src", "blob:fake");
    fireEvent.click(within(reply).getByRole("button", { name: "Download generated-video.mp4" }));
    await waitFor(() =>
      expect(fakeGeneration().saved).toEqual([
        { path: "/data/attachments/v1.mp4", name: "generated-video.mp4" },
      ]),
    );
    expect(await within(reply).findByText("Saved")).toBeInTheDocument();
  });

  it("names the calls and groups them in Used in this chat", () => {
    const view = {
      id: "g1",
      name: "generate_image",
      server: null,
      tool: "generate_image",
      title: null,
      inputJson: JSON.stringify({ prompt: "A red fox\nin the snow" }),
      status: "done" as const,
      approval: null,
      result: null,
      isError: false,
      durationMs: null,
      provider: false,
    };
    expect(toolKind(view)).toBe("generation");
    expect(toolKind({ ...view, server: "x" })).toBe("connector");
    expect(callHeadline(view)).toEqual({ action: "Made an image", detail: "A red fox" });
    expect(
      callHeadline({
        ...view,
        tool: "generate_audio",
        inputJson: '{"text":"Hello"}',
        status: "running",
      }),
    ).toEqual({ action: "Reading aloud", detail: "Hello" });
    const rows = usageRows([
      {
        toolUseId: "g1",
        kind: "generation",
        server: null,
        tool: "generate_image",
        label: "A red fox",
        isError: false,
      },
      {
        toolUseId: "g2",
        kind: "generation",
        server: null,
        tool: "generate_audio",
        label: "Hello",
        isError: false,
      },
    ] as Parameters<typeof usageRows>[0]);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.title).toBe("Generated media");
    expect(rows[0]?.details).toEqual(["A red fox", "Hello"]);
  });
});
