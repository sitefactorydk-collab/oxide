import { useEffect, useState } from "react";
import type { ProviderKind } from "@/bindings";
import { Toggle } from "@/features/models/ModelsSection";
import { useProviders } from "@/features/providers/queries";
import {
  DEFAULT_SETTINGS,
  useSetDictationSettings,
  useSettings,
} from "@/features/settings/queries";

/** Models that usually fit each kind of endpoint (typed models work too). */
const SUGGESTED: Record<ProviderKind, string[]> = {
  openai: ["whisper-1", "gpt-4o-mini-transcribe", "gpt-4o-transcribe", "whisper-large-v3-turbo"],
  lmstudio: ["whisper-1"],
  strata: ["whisper-1"],
  anthropic: [],
};

const input =
  "h-9 w-80 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent";

/** Settings → Voice: dictation through an OpenAI-compatible speech-to-text endpoint. */
export function VoiceSection() {
  const { data } = useSettings();
  const d = (data ?? DEFAULT_SETTINGS).dictation;
  const set = useSetDictationSettings();
  const { data: providers } = useProviders();
  // Anthropic's API has no speech-to-text.
  const usable = providers.filter((p) => p.kind !== "anthropic");
  const chosen = usable.find((p) => p.id === d.providerId);
  const [model, setModel] = useState(d.model);
  const [language, setLanguage] = useState(d.language);
  useEffect(() => setModel(d.model), [d.model]);
  useEffect(() => setLanguage(d.language), [d.language]);

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Toggle
        label="Show the microphone in the composer"
        hint="Click it to talk, click again to stop; what you said is added to your message. The recording goes to the endpoint below and isn’t kept."
        checked={d.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Speech-to-text endpoint</span>
        <select
          aria-label="Speech-to-text endpoint"
          value={chosen?.id ?? ""}
          onChange={(e) => set({ providerId: e.target.value || null })}
          className={input}
        >
          <option value="">Not set up</option>
          {usable.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <span className="text-sm text-fg-subtle">
          Any OpenAI-compatible endpoint with <code>/audio/transcriptions</code>: OpenAI, Groq, or a
          local Whisper server added in Settings → Providers. Its key is used.
        </span>
      </label>
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Model</span>
        <input
          aria-label="Transcription model"
          list="dictation-models"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onBlur={() => model.trim() !== d.model && set({ model: model.trim() })}
          className={input}
        />
        <datalist id="dictation-models">
          {(chosen ? SUGGESTED[chosen.kind] : SUGGESTED.openai).map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </label>
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Language of your speech</span>
        <input
          aria-label="Speech language"
          placeholder="Detect it"
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          onBlur={() => language.trim() !== d.language && set({ language: language.trim() })}
          className={`${input} w-40`}
        />
        <span className="text-sm text-fg-subtle">
          A two-letter code such as <code>da</code> or <code>en</code> helps the model; leave it
          empty to let it detect the language.
        </span>
      </label>
    </div>
  );
}
