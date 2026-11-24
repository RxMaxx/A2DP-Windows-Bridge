// Auracast (Developer screen): broadcasts the test tone, the system audio,
// an output device's sound or a recording device to every receiver nearby
// that joins (stream.start_broadcast; the core refuses it while developer
// mode is off), at a volume of its own (the stream's, which can follow the
// Windows volume). The broadcast is a stream like
// any other: Stop and the status follow core.state.snap.stream, the choices
// outlive the view (one set per core connection).

import { createEffect, createRoot, createSignal, Show } from "solid-js";
import { onAudioDevicesChanged, useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Button, Field, type Option, Section, Segmented, Select, Toggle } from "../components/controls";
import { VolumeControl } from "../components/stream";
import { t, tm } from "../i18n";
import type { Core } from "../ipc/store";
import { AuracastMixer } from "./AuracastMixer";
import type { AdapterInfo, AudioDevice, SourceInfo } from "../ipc/types";
import { adapterName } from "./Adapter";

/** What is broadcast: the test tone, the system audio, the loopback of an
 *  output device ("output:<id>") or a recording device ("input:<id>") */
export type Source = "tone" | "loopback" | `output:${string}` | `input:${string}`;
type Quality = "hq" | "sq";
type Channels = "stereo" | "mono" | "split" | "matrix";
/** How a pair of matrix outputs is sent (le_audio::broadcast::PairMode) */
export type PairMode = "stereo" | "separate" | "subgroups" | "off";

/** The BIGs (broadcasts) a pair takes */
export const pairBigs = (m: PairMode) => (m === "separate" ? 2 : m === "off" ? 0 : 1);
/** How long after a change of source the stream info names the one that
 *  plays (the core switches at its next 200 ms tick, then opens the capture) */
const SOURCE_SETTLE_MS = 1500;
/** BIGs the ASUS BT600 runs at most (a third: LE Create BIG 0x0D) */
export const MAX_BIGS = 2;

/** The layout as stream.start_broadcast takes it */
export function layoutOf(c: { channels: Channels; front: PairMode; rear: PairMode }): string | null {
  if (c.channels === "mono") return null;
  if (c.channels === "matrix") return `matrix_${c.front}_${c.rear}`;
  return c.channels;
}

/** A device source's kind and endpoint id; null: the tone or system audio */
function deviceOf(source: Source): { kind: "output" | "input"; id: string } | null {
  const at = source.indexOf(":");
  if (at < 0) return null;
  return { kind: source.slice(0, at) as "output" | "input", id: source.slice(at + 1) };
}

interface SourceParams {
  source: string;
  source_id: string | null;
  source_name: string | null;
}

/** The source as stream.start_broadcast and auracast.set_source take it
 *  (`names`: the names of devices listed before, for one that is gone) */
export function sourceParams(
  source: Source,
  outputs: readonly AudioDevice[],
  inputs: readonly AudioDevice[],
  names?: ReadonlyMap<string, string>,
): SourceParams {
  const device = deviceOf(source);
  if (!device) return { source, source_id: null, source_name: null };
  const list = device.kind === "output" ? outputs : inputs;
  const name = list.find((d) => d.id === device.id)?.name ?? names?.get(device.id) ?? null;
  return { source: device.kind, source_id: device.id, source_name: name };
}

/** Whether the broadcast's stream info names the source sent as `params`
 *  (a device by the name sent, or by its id when none was) */
export function playsSource(info: SourceInfo | null | undefined, params: SourceParams): boolean {
  if (!info) return false;
  const kind = info.kind === "virtual" ? "output" : info.kind;
  if (kind !== params.source) return false;
  return params.source_id === null || info.name === (params.source_name ?? params.source_id);
}

/** The choice of what the broadcast plays (its stream info), a device by
 *  its name (the first of two with the same name); null: none of them */
export function sourceOfInfo(
  info: SourceInfo | null | undefined,
  outputs: readonly AudioDevice[],
  inputs: readonly AudioDevice[],
): Source | null {
  const named = (list: readonly AudioDevice[]) => list.find((d) => d.name === info?.name)?.id;
  switch (info?.kind) {
    case "tone":
    case "loopback":
      return info.kind;
    case "output":
    case "virtual": {
      const id = named(outputs);
      return id === undefined ? null : `output:${id}`;
    }
    case "input": {
      const id = named(inputs);
      return id === undefined ? null : `input:${id}`;
    }
    default:
      return null;
  }
}

/** The sources to choose from: the tone, the system audio, Windows' output
 *  and recording devices, and a chosen device that is not there now (by
 *  its name in `names`, the devices listed before) */
export function sourceOptions(
  chosen: Source,
  outputs: readonly AudioDevice[],
  inputs: readonly AudioDevice[],
  names?: ReadonlyMap<string, string>,
): Option<Source>[] {
  const device =
    (kind: "output" | "input", group: string) =>
    (d: AudioDevice): Option<Source> => ({ value: `${kind}:${d.id}`, label: d.name, group });
  const options: Option<Source>[] = [
    { value: "tone", label: t("receiver.source_tone") },
    { value: "loopback", label: t("receiver.source_system") },
    ...outputs.map(device("output", t("ui.dev.auracast_outputs"))),
    ...inputs.map(device("input", t("ui.dev.auracast_inputs"))),
  ];
  if (!options.some((o) => o.value === chosen)) {
    const d = deviceOf(chosen);
    const input = d?.kind === "input";
    const group = input ? t("ui.dev.auracast_inputs") : t("ui.dev.auracast_outputs");
    const name = (d && names?.get(d.id)) || t(input ? "capture.input" : "capture.output");
    options.push({ value: chosen, label: t("ui.dev.auracast_device_gone", name), group });
  }
  return options;
}

/** A source whose Windows volume the broadcast can follow (not the tone,
 *  nor a recording device: its recording level is in the audio already) */
export const followable = (s: Source) => s !== "tone" && deviceOf(s)?.kind !== "input";

/** The core's limits (le_audio::broadcast): UTF-8 bytes */
export const NAME_BYTES = [4, 32] as const;
export const CODE_BYTES = [4, 16] as const;

interface Choices {
  name: string;
  code: string;
  quality: Quality;
  channels: Channels;
  /** Matrix: the front (FL FR) and the rear (RL RR), each with its rate */
  front: PairMode;
  rear: PairMode;
  frontQuality: Quality;
  rearQuality: Quality;
  source: Source;
  /** The broadcast's volume follows the Windows volume */
  follow: boolean;
}

const bytes = (s: string) => new TextEncoder().encode(s).length;

/** The error of a name or code field, null when it is fine (an empty code: none) */
export function lengthError(text: string, [min, max]: readonly [number, number], optional: boolean): string | null {
  const n = bytes(optional ? text : text.trim());
  if (optional && n === 0) return null;
  return n < min || n > max ? t("ui.dev.auracast_err_length", min, max) : null;
}

const hex4 = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");

/** The adapter for broadcasts (Settings::auracast_adapter_id): the current
 *  one first, then each adapter on WinUSB by name and MAC address (two
 *  adapters can share a name, "Bluetooth Controller"), VID:PID while its
 *  address is not known yet, and a chosen one that is not there. `value`
 *  is the option the setting names, whatever its case. */
export function auracastAdapterChoices(
  list: readonly AdapterInfo[],
  chosen: string,
): { value: string; options: { value: string; label: string }[] } {
  const winusb = list.filter((a) => a.driver === "winusb");
  const id = chosen.trim();
  const options = [
    { value: "", label: t("ui.dev.auracast_adapter_current") },
    ...winusb.map((a) => ({
      value: a.instance_id,
      label: `${adapterName(a)} · ${a.address ?? `${hex4(a.vid)}:${hex4(a.pid)}`}`,
    })),
  ];
  const found = winusb.find((a) => id !== "" && a.instance_id.toLowerCase() === id.toLowerCase());
  if (id !== "" && !found) options.push({ value: id, label: t("ui.dev.auracast_adapter_missing") });
  return { value: found?.instance_id ?? id, options };
}

function createSession(core: Core) {
  return createRoot(() => {
    const [choices, setChoices] = createSignal<Choices>({
      name: "BlueCaster",
      code: "",
      quality: "hq",
      channels: "stereo",
      front: "stereo",
      rear: "stereo",
      frontQuality: "hq",
      rearQuality: "hq",
      source: "tone",
      follow: false,
    });
    /** A broadcast asked for here, until the stream it started ends or another takes over */
    const [mine, setMine] = createSignal(false);
    /** Why the last broadcast asked for here ended with an error */
    const [failed, setFailed] = createSignal<string | null>(null);
    /** stream.start_broadcast not answered yet: the stream before it still shows */
    const [asking, setAsking] = createSignal(false);
    /** The broadcast asked for here went on air. Until then an Idle is the
     *  session before it ending: the core answers stream.start_broadcast,
     *  then stops what ran before (Idle), then sets the broadcast up */
    const [onAir, setOnAir] = createSignal(false);
    /** The names of the devices listed so far, for a chosen one that is gone */
    const names = new Map<string, string>();
    createEffect(() => {
      const st = core.state.snap?.stream;
      if (!st || !mine() || asking()) return;
      if (st.state === "error") {
        setFailed(tm(st.message));
        setMine(false);
      } else if (st.state === "streaming" && st.message.key === "status.broadcasting") {
        setOnAir(true);
      } else if ((st.state === "idle" && onAir()) || st.state === "streaming") {
        // Stopped, or another stream took over
        setMine(false);
      }
    });
    return { choices, setChoices, mine, setMine, failed, setFailed, asking, setAsking, setOnAir, names };
  });
}

const sessions = new WeakMap<Core, ReturnType<typeof createSession>>();

export function Auracast() {
  const app = useApp();
  const { core } = app;
  let session = sessions.get(core);
  if (!session) {
    session = createSession(core);
    sessions.set(core, session);
  }
  const s = session;
  const choices = s.choices;
  const set = (patch: Partial<Choices>) => s.setChoices({ ...choices(), ...patch });
  const stream = () => core.state.snap!.stream;
  // Windows' devices to broadcast from, read again when they change
  const [outputs, setOutputs] = createSignal<AudioDevice[]>([]);
  const [inputs, setInputs] = createSignal<AudioDevice[]>([]);
  const listed = (list: AudioDevice[]) => {
    for (const d of list) s.names.set(d.id, d.name);
    return list;
  };
  const readDevices = () => {
    core.request("audio.devices").then(
      (list) => setOutputs(listed(list)),
      () => {},
    );
    core.request("audio.input_devices").then(
      (list) => setInputs(listed(list)),
      () => {},
    );
  };
  readDevices();
  onAudioDevicesChanged(readDevices);
  const paramsOf = (source: Source) => sourceParams(source, outputs(), inputs(), s.names);
  // The core keeps "follow" between broadcasts and windows: the switch shows it
  core.request("auracast.follow_volume").then(
    (follow) => set({ follow }),
    () => {},
  );

  /** The running stream is the broadcast */
  const live = () => {
    const st = stream();
    if (st.state === "streaming") return st.message.key === "status.broadcasting";
    // Setting up, or starting again after the adapter stopped working
    return (st.state === "connecting" || st.state === "reconnecting") && s.mine() && st.profile === null;
  };
  /** What to show under the form: the broadcast's state, or why it ended */
  const status = () => {
    const st = stream();
    if (live()) return { text: tm(st.message), bad: false };
    const failed = s.failed();
    return failed === null ? null : { text: failed, bad: true };
  };

  const nameError = () => lengthError(choices().name, NAME_BYTES, false);
  const codeError = () => lengthError(choices().code, CODE_BYTES, true);

  const startStop = async () => {
    if (live()) {
      await app.run(() => core.request("stream.stop"));
      s.setMine(false);
      return;
    }
    const c = choices();
    s.setAsking(true);
    s.setFailed(null);
    s.setOnAir(false);
    s.setMine(true);
    const ok = await app.run(() =>
      core.request("stream.start_broadcast", {
        name: c.name.trim(),
        code: c.code === "" ? null : c.code,
        quality: c.channels === "matrix" ? c.frontQuality : c.quality,
        rear_quality: c.channels === "matrix" ? c.rearQuality : null,
        mono: c.channels === "mono",
        layout: layoutOf(c),
        ...paramsOf(c.source),
        follow_volume: c.follow,
      }),
    );
    if (!ok) s.setMine(false);
    s.setAsking(false);
  };

  const locked = () => live() || s.asking();
  /** The source: the running broadcast switches to it in place */
  const changeSource = async (source: Source) => {
    const before = choices().source;
    set({ source });
    if (!live()) return;
    const sent = paramsOf(source);
    const sentBefore = paramsOf(before);
    let taken = false;
    await app.run(async () => {
      taken = await core.request("auracast.set_source", sent);
    });
    if (!taken) {
      // Not taken (a start or stop under way): the broadcast keeps its source
      if (choices().source === source) set({ source: before });
      return;
    }
    // A source that cannot be captured: the broadcast goes on with the one
    // before, which its stream info then still names
    setTimeout(() => {
      if (!live() || choices().source !== source) return;
      const info = stream().info?.source;
      // By what was sent, not by name alone: two devices can share a name
      if (playsSource(info, sent)) return;
      const playing = playsSource(info, sentBefore) ? before : sourceOfInfo(info, outputs(), inputs());
      if (playing !== null && playing !== source) set({ source: playing });
    }, SOURCE_SETTLE_MS);
  };
  /** Follow the Windows volume: kept for the next broadcast, taken by the running one */
  const changeFollow = (follow: boolean) => {
    set({ follow });
    void app.run(() => core.request("auracast.set_follow_volume", { follow }));
  };
  /** While its broadcasts come up the source cannot change yet */
  const sourceLocked = () => s.asking() || (live() && stream().state !== "streaming");
  const rateOptions = (): [Quality, string][] => [
    ["hq", t("ui.dev.auracast_48")],
    ["sq", t("ui.dev.auracast_24")],
  ];
  const matrix = () => choices().channels === "matrix";
  const bigs = () => (matrix() ? pairBigs(choices().front) + pairBigs(choices().rear) : 0);
  const pairOptions = (): [PairMode, string][] => [
    ["stereo", t("ui.dev.auracast_pair_stereo")],
    ["separate", t("ui.dev.auracast_pair_separate")],
    ["subgroups", t("ui.dev.auracast_pair_subgroups")],
    ["off", t("ui.dev.auracast_pair_off")],
  ];

  // The adapter the broadcast goes on: offered while several adapters are
  // on WinUSB, or while one is chosen (to go back to the current one)
  const auracastAdapter = () => core.state.snap!.settings.auracast_adapter_id ?? "";
  const adapterChoice = () => auracastAdapterChoices(core.state.snap!.adapter.adapters, auracastAdapter());
  const showAdapter = () =>
    core.state.snap!.adapter.adapters.filter((a) => a.driver === "winusb").length > 1 ||
    auracastAdapter().trim() !== "";

  return (
    <div>
      <p class="hint">{t("ui.dev.auracast_intro")}</p>
      <Section
        title={t("ui.dev.auracast")}
        actions={
          <Button
            kind={live() ? "danger" : "strong"}
            icon={live() ? "stop" : "play"}
            busy={s.asking()}
            disabled={!live() && (nameError() !== null || codeError() !== null || (matrix() && bigs() === 0))}
            onClick={startStop}
          >
            {live() ? t("receiver.stop") : t("receiver.start")}
          </Button>
        }
      >
        <Show when={showAdapter()}>
          <Field label={t("ui.dev.auracast_adapter")} hint={t("ui.dev.auracast_adapter_hint")}>
            {(id, describedBy) => (
              <Select
                id={id}
                describedBy={describedBy}
                value={adapterChoice().value}
                options={adapterChoice().options}
                disabled={locked()}
                onChange={(auracast_adapter_id) => void saveSettings(app, { auracast_adapter_id })}
              />
            )}
          </Field>
        </Show>
        <Field label={t("ui.dev.auracast_name")} error={nameError()}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="text"
              value={choices().name}
              disabled={locked()}
              aria-describedby={describedBy}
              aria-invalid={nameError() ? "true" : undefined}
              onInput={(e) => set({ name: e.currentTarget.value })}
            />
          )}
        </Field>
        <Field label={t("ui.dev.auracast_code")} error={codeError()}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="text"
              autocomplete="off"
              spellcheck={false}
              value={choices().code}
              placeholder={t("ui.dev.auracast_code_none")}
              disabled={locked()}
              aria-describedby={describedBy}
              aria-invalid={codeError() ? "true" : undefined}
              onInput={(e) => set({ code: e.currentTarget.value })}
            />
          )}
        </Field>
        <Choice<Quality>
          label={t("ui.dev.auracast_quality")}
          value={choices().quality}
          disabled={locked() || matrix()}
          options={[
            ["hq", t("ui.dev.auracast_hq")],
            ["sq", t("ui.dev.auracast_sq")],
          ]}
          onChange={(quality) => set({ quality })}
        />
        <Choice<Channels>
          label={t("ui.dev.auracast_channels")}
          value={choices().channels}
          disabled={locked()}
          options={[
            ["stereo", t("ui.dev.auracast_stereo")],
            ["mono", t("ui.dev.auracast_mono")],
            ["split", t("ui.dev.auracast_split")],
            ["matrix", t("ui.dev.auracast_matrix")],
          ]}
          onChange={(channels) => set({ channels })}
        />
        <Field label={t("receiver.source")}>
          {(id, describedBy) => (
            <Select<Source>
              id={id}
              describedBy={describedBy}
              value={choices().source}
              options={sourceOptions(choices().source, outputs(), inputs(), s.names)}
              disabled={sourceLocked()}
              onChange={changeSource}
            />
          )}
        </Field>
        <div class="row">
          <span class="row-text">
            <span class="row-label">{t("ui.dev.auracast_volume")}</span>
          </span>
          <VolumeControl off={!live()} />
        </div>
        <Toggle
          label={t("ui.dev.auracast_follow")}
          hint={t("ui.dev.auracast_follow_hint")}
          checked={choices().follow && followable(choices().source)}
          disabled={!followable(choices().source)}
          onChange={changeFollow}
        />
        <Show when={status()}>
          {(st) => (
            <div role="status" class="stack-tight" style={st().bad ? { color: "var(--bad-text)" } : undefined}>
              <p>{st().text}</p>
              <Show when={live() && stream().info?.le?.summary}>{(sum) => <p class="muted">{sum()}</p>}</Show>
            </div>
          )}
        </Show>
      </Section>
      <Show when={matrix()}>
        <AuracastMixer rear={choices().rear !== "off"} live={live() && !s.asking()}>
          <PairRow
            label={t("ui.dev.auracast_front")}
            mode={choices().front}
            rate={choices().frontQuality}
            disabled={locked()}
            modes={pairOptions()}
            rates={rateOptions()}
            onMode={(front) => set({ front })}
            onRate={(frontQuality) => set({ frontQuality })}
          />
          <PairRow
            label={t("ui.dev.auracast_rear")}
            mode={choices().rear}
            rate={choices().rearQuality}
            disabled={locked()}
            modes={pairOptions()}
            rates={rateOptions()}
            onMode={(rear) => set({ rear })}
            onRate={(rearQuality) => set({ rearQuality })}
          />
          <p class="hint">{t("ui.dev.auracast_speakers_hint")}</p>
          <Show when={bigs() === 0}>
            <p class="hint" style={{ color: "var(--bad-text)" }}>
              {t("ui.dev.auracast_pairs_off_hint")}
            </p>
          </Show>
          <Show when={bigs() > MAX_BIGS}>
            <p class="hint field-warn">{t("ui.dev.auracast_bigs_hint", bigs(), MAX_BIGS)}</p>
          </Show>
        </AuracastMixer>
      </Show>
    </div>
  );
}

/** A pair of matrix outputs: how it is sent, and its rate */
function PairRow(props: {
  label: string;
  mode: PairMode;
  rate: Quality;
  disabled: boolean;
  modes: [PairMode, string][];
  rates: [Quality, string][];
  onMode: (m: PairMode) => void;
  onRate: (q: Quality) => void;
}) {
  return (
    <div class="row pair-row">
      <span class="row-text">
        <span class="row-label">{props.label}</span>
      </span>
      <span class="pair-row-controls">
        <Segmented<PairMode>
          label={props.label}
          value={props.mode}
          options={props.modes.map(([value, label]) => ({ value, label, disabled: props.disabled }))}
          onChange={props.onMode}
        />
        <Segmented<Quality>
          label={`${props.label}: ${t("ui.dev.auracast_quality")}`}
          value={props.rate}
          options={props.rates.map(([value, label]) => ({
            value,
            label,
            disabled: props.disabled || props.mode === "off",
          }))}
          onChange={props.onRate}
        />
      </span>
    </div>
  );
}

function Choice<T extends string>(props: {
  label: string;
  value: T;
  disabled: boolean;
  options: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <div class="row">
      <span class="row-text">
        <span class="row-label">{props.label}</span>
      </span>
      <Segmented<T>
        label={props.label}
        value={props.value}
        options={props.options.map(([value, label]) => ({ value, label, disabled: props.disabled }))}
        onChange={props.onChange}
      />
    </div>
  );
}
