// The headset microphone's processing, in a dialog opened under its Input
// Source: gain, noise suppression with its strength, and the noise gate's
// threshold (lowest: off). Kept in the device's profile; the running
// microphone takes a change in place, nothing reconnects. A slider saves
// when let go and shows what it holds until the profile has it.

import { createSignal, createUniqueId, Show } from "solid-js";
import { t } from "../i18n";
import type { MicProcessing } from "../ipc/types";
import { Button, Dialog, Toggle } from "./controls";

/** Anything set away from "off" */
export const micProcessingOn = (p: MicProcessing) =>
  p.gain_db !== 0 || p.noise_suppression || p.gate_db > MIC_GATE_RANGE[0];

/** A line saying whether the processing is on, with the dialog behind it */
export function MicProcessingRow(props: {
  value: MicProcessing;
  disabled?: boolean;
  onChange: (p: MicProcessing) => Promise<unknown>;
}) {
  const [open, setOpen] = createSignal(false);
  return (
    <div class="row dialog-row">
      <span class="row-label">{t("ui.mic_processing")}</span>
      <div class="inline">
        <span class="row-state">{t(micProcessingOn(props.value) ? "ui.state_on" : "ui.state_off")}</span>
        <Button onClick={() => setOpen(true)}>{t("ui.adjust")}</Button>
      </div>
      <Dialog open={open()} title={t("ui.mic_processing")} onCancel={() => setOpen(false)}>
        <MicProcessingControls value={props.value} disabled={props.disabled} onChange={props.onChange} />
      </Dialog>
    </div>
  );
}

export const MIC_GAIN_RANGE = [-12, 24] as const;
export const MIC_GATE_RANGE = [-80, -20] as const;

export const gainText = (db: number) => `${db > 0 ? "+" : ""}${db} dB`;
export const gateText = (db: number) => (db <= MIC_GATE_RANGE[0] ? t("ui.mic_gate_off") : `${db} dB`);

function Slider(props: {
  label: string;
  min: number;
  max: number;
  value: number;
  text: (v: number) => string;
  disabled?: boolean;
  onSave: (v: number) => Promise<unknown>;
}) {
  const id = createUniqueId();
  // Being dragged, then saved and not back from the core yet
  const [held, setHeld] = createSignal<number | null>(null);
  const [pending, setPending] = createSignal<number | null>(null);
  const shown = () => held() ?? pending() ?? props.value;
  const fill = () => ((shown() - props.min) / (props.max - props.min)) * 100;
  const letGo = () => {
    const v = held();
    setHeld(null);
    if (v === null || v === props.value) return;
    setPending(v);
    void props.onSave(v).finally(() => setPending(null));
  };
  return (
    <div class="row mic-slider">
      <label class="row-label" for={id}>
        {props.label}
      </label>
      <div class="volume">
        <input
          id={id}
          type="range"
          min={props.min}
          max={props.max}
          step="1"
          value={shown()}
          disabled={props.disabled}
          aria-valuetext={props.text(shown())}
          style={{ "--fill": `${fill()}%` }}
          onInput={(e) => setHeld(e.currentTarget.valueAsNumber)}
          onChange={letGo}
        />
        <output class="volume-value num" for={id}>
          {props.text(shown())}
        </output>
      </div>
    </div>
  );
}

export function MicProcessingControls(props: {
  value: MicProcessing;
  disabled?: boolean;
  onChange: (p: MicProcessing) => Promise<unknown>;
}) {
  const save = (patch: Partial<MicProcessing>) => props.onChange({ ...props.value, ...patch });
  return (
    <div class="mic-processing">
      <Slider
        label={t("ui.mic_gain")}
        min={MIC_GAIN_RANGE[0]}
        max={MIC_GAIN_RANGE[1]}
        value={props.value.gain_db}
        text={gainText}
        disabled={props.disabled}
        onSave={(v) => save({ gain_db: v })}
      />
      <Toggle
        label={t("ui.mic_noise_suppression")}
        checked={props.value.noise_suppression}
        disabled={props.disabled}
        onChange={(v) => void save({ noise_suppression: v })}
      />
      <Show when={props.value.noise_suppression}>
        <Slider
          label={t("ui.mic_suppression_strength")}
          min={0}
          max={100}
          value={props.value.suppression}
          text={(v) => `${v}%`}
          disabled={props.disabled}
          onSave={(v) => save({ suppression: v })}
        />
      </Show>
      <Slider
        label={t("ui.mic_gate")}
        min={MIC_GATE_RANGE[0]}
        max={MIC_GATE_RANGE[1]}
        value={Math.max(props.value.gate_db, MIC_GATE_RANGE[0])}
        text={gateText}
        disabled={props.disabled}
        onSave={(v) => save({ gate_db: v })}
      />
    </div>
  );
}
