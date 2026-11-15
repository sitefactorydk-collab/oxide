import { Loader2, Mic, Square } from "lucide-react";
import { IconButton } from "@/components/ui/IconButton";
import { useSettings } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useDictation } from "./useDictation";

/**
 * The composer's microphone (Phase 18): click to record, click again to stop; the speech is
 * turned into text and added to the message. Hidden when dictation is switched off in
 * Settings → Voice; opens Voice when no endpoint is set up yet.
 */
export function DictationButton({ onText }: { onText: (text: string) => void }) {
  const { data: settings } = useSettings();
  const openSettings = useOpenSettings();
  const t = useT();
  const dictation = useDictation(onText, (m) => toast(m, "error"));
  if (settings && !settings.dictation.enabled) return null;
  const ready = Boolean(settings?.dictation.providerId);

  if (dictation.state === "transcribing") {
    return (
      <IconButton label={t("composer.transcribing")} disabled>
        <Loader2 className="size-[18px] animate-spin" />
      </IconButton>
    );
  }
  const recording = dictation.state === "recording";
  return (
    <IconButton
      label={recording ? t("composer.stopDictation") : t("composer.dictate")}
      active={recording}
      onClick={() => {
        if (recording) {
          dictation.stop();
        } else if (!ready) {
          toast("Pick an endpoint for dictation first.");
          openSettings("voice");
        } else {
          void dictation.start();
        }
      }}
      className={cn(recording && "text-danger")}
    >
      {recording ? (
        <Square className="size-3.5 animate-pulse fill-current" />
      ) : (
        <Mic className="size-[18px]" />
      )}
    </IconButton>
  );
}
