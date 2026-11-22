// Auracast (Developer screen): broadcasts the test tone or the system audio
// to every receiver nearby that joins (stream.start_broadcast; the core
// refuses it while developer mode is off). The broadcast is a stream like
// any other: Stop and the status follow core.state.snap.stream, the choices
// outlive the view (one set per core connection).

import { createEffect, createRoot, createSignal, Show } from "solid-js";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Button, Field, Section, Segmented, Select } from "../components/controls";
import { t, tm } from "../i18n";
import type { Core } from "../ipc/store";
import type { AdapterInfo } from "../ipc/types";
import { adapterName } from "./Adapter";

type Source = "tone" | "loopback";
type Quality = "hq" | "sq";
type Channels = "stereo" | "mono" | "split";

/** The core's limits (le_audio::broadcast): UTF-8 bytes */
export const NAME_BYTES = [4, 32] as const;
export const CODE_BYTES = [4, 16] as const;

interface Choices {
  name: string;
  code: string;
  quality: Quality;
  channels: Channels;
  source: Source;
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
      source: "tone",
    });
    /** A broadcast asked for here, until the stream it started ends or another takes over */
    const [mine, setMine] = createSignal(false);
    /** Why the last broadcast asked for here ended with an error */
    const [failed, setFailed] = createSignal<string | null>(null);
    /** stream.start_broadcast not answered yet: the stream before it still shows */
    const [asking, setAsking] = createSignal(false);
    createEffect(() => {
      const st = core.state.snap?.stream;
      if (!st || !mine() || asking()) return;
      if (st.state === "error") {
        setFailed(tm(st.message));
        setMine(false);
      } else if (st.state === "idle" || (st.state === "streaming" && st.message.key !== "status.broadcasting")) {
        setMine(false);
      }
    });
    return { choices, setChoices, mine, setMine, failed, setFailed, asking, setAsking };
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

  /** The running stream is the broadcast */
  const live = () => {
    const st = stream();
    if (st.state === "streaming") return st.message.key === "status.broadcasting";
    return st.state === "connecting" && s.mine() && st.profile === null;
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
      return;
    }
    const c = choices();
    s.setAsking(true);
    s.setFailed(null);
    s.setMine(true);
    const ok = await app.run(() =>
      core.request("stream.start_broadcast", {
        name: c.name.trim(),
        code: c.code === "" ? null : c.code,
        quality: c.quality,
        mono: c.channels === "mono",
        split: c.channels === "split",
        source: c.source,
      }),
    );
    if (!ok) s.setMine(false);
    s.setAsking(false);
  };

  const locked = () => live() || s.asking();

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
            disabled={!live() && (nameError() !== null || codeError() !== null)}
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
          disabled={locked()}
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
          ]}
          onChange={(channels) => set({ channels })}
        />
        <Choice<Source>
          label={t("receiver.source")}
          value={choices().source}
          disabled={locked()}
          options={[
            ["tone", t("receiver.source_tone")],
            ["loopback", t("receiver.source_system")],
          ]}
          onChange={(source) => set({ source })}
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
