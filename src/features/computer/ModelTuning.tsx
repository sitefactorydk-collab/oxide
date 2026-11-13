import { Loader2 } from "lucide-react";
import type { ComputerModelSettings, ModelInfo } from "@/bindings";
import { allModels } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { DEFAULT_SETTINGS, useSetComputerSettings, useSettings } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { describeCalibration } from "./calibration";
import { useCalibrate } from "./queries";

/**
 * Settings → Computer use → Models: per model that sees images, a labelled grid on its
 * screenshots, and Calibrate (two test images; the measured offset corrects its clicks).
 */
export function ModelTuning() {
  const { data: lists } = useModels();
  const settings = useSettings().data ?? DEFAULT_SETTINGS;
  const set = useSetComputerSettings();
  const calibrate = useCalibrate();
  const models = allModels(lists ?? []).filter(
    (m) => m.capabilities.images && m.capabilities.tools,
  );
  const tuning = settings.computer.models;
  const update = (model: string, change: (m: ComputerModelSettings) => ComputerModelSettings) => {
    const old = tuning.find((t) => t.model === model) ?? { model, grid: false, calibration: null };
    set({ models: [...tuning.filter((t) => t.model !== model), change(old)] });
  };
  return (
    <section aria-label="Models" className="flex flex-col gap-3 border-t border-line pt-5">
      <h3 className="text-md font-semibold text-fg">Models</h3>
      <p className="text-sm text-fg-subtle">
        Some models click a little beside what they mean. <strong>Calibrate</strong> shows a model
        two test pictures and asks where six dots are; Oxide then corrects its clicks by what it
        measured. A <strong>grid</strong> on screenshots (lines every 100 pixels, labelled) helps
        some models read positions. Calibrate again after switching the grid.
      </p>
      {models.length === 0 ? (
        <p className="text-sm text-fg-muted">
          No model that sees images yet. Mark one in Settings → Models.
        </p>
      ) : (
        <ul className="flex flex-col divide-y divide-line rounded-xl border border-line">
          {models.map((m) => (
            <ModelRow
              key={m.id}
              model={m}
              tuning={tuning.find((t) => t.model === m.id)}
              busy={calibrate.isPending && calibrate.variables === m.id}
              anyBusy={calibrate.isPending}
              error={
                calibrate.isError && calibrate.variables === m.id
                  ? errorMessage(calibrate.error)
                  : null
              }
              onCalibrate={() => calibrate.mutate(m.id)}
              onGrid={(grid) => update(m.id, (t) => ({ ...t, grid }))}
              onReset={() => update(m.id, (t) => ({ ...t, calibration: null }))}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function ModelRow({
  model,
  tuning,
  busy,
  anyBusy,
  error,
  onCalibrate,
  onGrid,
  onReset,
}: {
  model: ModelInfo;
  tuning: ComputerModelSettings | undefined;
  busy: boolean;
  anyBusy: boolean;
  error: string | null;
  onCalibrate: () => void;
  onGrid: (on: boolean) => void;
  onReset: () => void;
}) {
  const grid = tuning?.grid ?? false;
  const c = tuning?.calibration ?? null;
  return (
    <li className="flex flex-col gap-1.5 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-md text-fg">{model.label}</span>
        <button
          type="button"
          aria-pressed={grid}
          aria-label={`Grid on ${model.label}'s screenshots`}
          onClick={() => onGrid(!grid)}
          className={cn(
            "rounded-md px-1.5 py-0.5 text-xs hover:opacity-80",
            grid ? "bg-accent-soft text-accent" : "bg-sunken text-fg-subtle",
          )}
        >
          {grid ? "Grid" : "No grid"}
        </button>
        <button
          type="button"
          disabled={anyBusy}
          onClick={onCalibrate}
          className="inline-flex h-7 items-center gap-1.5 rounded-md border border-line-strong px-2 text-xs text-fg hover:bg-hover disabled:opacity-50"
        >
          {busy && <Loader2 aria-hidden className="size-3 animate-spin" />}
          {busy ? "Calibrating…" : c ? "Calibrate again" : "Calibrate"}
        </button>
        {c && (
          <button
            type="button"
            aria-label={`Reset ${model.label}'s calibration`}
            onClick={onReset}
            className="h-7 rounded-md px-1.5 text-xs text-fg-muted hover:bg-hover hover:text-fg"
          >
            Reset
          </button>
        )}
      </div>
      {busy && <p className="text-xs text-fg-subtle">Asking the model about two test pictures…</p>}
      {c && !busy && (
        <p className="text-xs text-fg-muted">
          {describeCalibration(c)}
          {c.grid !== grid &&
            " · measured with the grid " + (c.grid ? "on" : "off") + ": calibrate again"}
        </p>
      )}
      {error && (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      )}
    </li>
  );
}
