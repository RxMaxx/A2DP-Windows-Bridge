// The playback EQ on Audio Devices: on and off, points added on the graph
// (double-click, the button), moved by keyboard and by dragging (tried out
// on the stream while dragged, saved when let go), their type changed from
// the right-click menu, edited in the fields, and reset

import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore } from "../ipc/mock";
import type { MediaEq, Profile } from "../ipc/types";
import { createCore } from "../ipc/store";
import { endSettle } from "./Codecs";

let mock: MockCore | null = null;

function start() {
  location.hash = "#/devices";
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ scenario: "normal", speed: 0, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  const saved = () =>
    calls
      .filter((c) => c.method === "profile.save")
      .map((c) => (c.params as { profile: Profile }).profile.media_eq as MediaEq);
  const lastEq = () => saved()[saved().length - 1];
  return { calls, core, saved, lastEq };
}

afterEach(async () => {
  cleanup();
  endSettle();
  mock?.dispose();
  mock = null;
  await setLanguage("en");
});

const deviceEntry = (name: RegExp) =>
  waitFor(() => {
    const entry = [...document.querySelectorAll<HTMLElement>("main .device-entry")].find((e) =>
      name.test(e.textContent ?? ""),
    );
    expect(entry).toBeTruthy();
    return entry!;
  });

/** The graph's box as a browser lays it out: 640 x 240 px at (0, 0) */
function layOut(svg: SVGSVGElement) {
  svg.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 640, height: 240, right: 640, bottom: 240, x: 0, y: 0, toJSON() {} }) as DOMRect;
  svg.setPointerCapture = () => {};
  svg.releasePointerCapture = () => {};
  svg.hasPointerCapture = () => true;
}

async function eqOn() {
  const env = start();
  const wh = await deviceEntry(/WH-1000XM5/);
  const toggle = within(wh).getByRole("switch", { name: t("eq.title") });
  expect(toggle).not.toBeChecked();
  expect(wh.querySelector(".eq-graph")).toBeNull();
  fireEvent.click(toggle);
  await waitFor(() => expect(env.lastEq()).toEqual({ enabled: true, bands: [] }));
  const svg = await waitFor(() => {
    const s = wh.querySelector<SVGSVGElement>(".eq-graph svg");
    expect(s).toBeTruthy();
    return s!;
  });
  layOut(svg);
  return { ...env, wh, svg };
}

describe("Playback EQ", () => {
  it("is off until turned on, and adds a point where the graph is double-clicked", async () => {
    const { wh, svg, lastEq } = await eqOn();
    // 1 kHz is 2/3 of the way along the log axis; +6 dB a quarter from the top
    fireEvent.dblClick(svg, { clientX: 38 + (592 * Math.log10(50)) / 3, clientY: 10 + 208 * 0.375 });
    await waitFor(() => expect(lastEq().bands).toHaveLength(1));
    const p = lastEq().bands[0];
    expect(p.band_type).toBe("Peaking");
    expect(p.frequency).toBe(1000);
    expect(p.gain).toBeCloseTo(6, 1);
    const point = await within(wh).findByRole("button", { name: /^Point 1: Peak, 1.00 kHz, \+6.0 dB, Q 1.00$/ });
    expect(point).toHaveAttribute("aria-pressed", "true");
    // The selected point's fields
    expect(within(wh).getByRole("spinbutton", { name: t("eq.frequency") })).toHaveValue(1000);
  });

  it("moves a focused point with the keyboard and deletes it", async () => {
    const { wh, lastEq } = await eqOn();
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.add_point") }));
    const point = await within(wh).findByRole("button", { name: /^Point 1:/ });
    point.focus();
    fireEvent.keyDown(point, { key: "ArrowUp" });
    await waitFor(() => expect(lastEq().bands[0].gain).toBe(0.5));
    fireEvent.keyDown(within(wh).getByRole("button", { name: /^Point 1:/ }), { key: "ArrowRight", shiftKey: true });
    await waitFor(() => expect(lastEq().bands[0].frequency).toBe(2000));
    fireEvent.keyDown(within(wh).getByRole("button", { name: /^Point 1:/ }), { key: "PageUp" });
    await waitFor(() => expect(lastEq().bands[0].q).toBe(1.25));
    fireEvent.keyDown(within(wh).getByRole("button", { name: /^Point 1:/ }), { key: "Delete" });
    await waitFor(() => expect(lastEq().bands).toEqual([]));
  });

  it("changes a point's type from its menu, and resets", async () => {
    const { wh, lastEq } = await eqOn();
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.add_point") }));
    const point = await within(wh).findByRole("button", { name: /^Point 1:/ });
    fireEvent.keyDown(point, { key: "ArrowUp", shiftKey: true });
    await waitFor(() => expect(lastEq().bands[0].gain).toBe(3));
    fireEvent.contextMenu(within(wh).getByRole("button", { name: /^Point 1:/ }));
    const menu = await within(wh).findByRole("menu");
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: t("eq.type.high_pass") }));
    await waitFor(() => expect(lastEq().bands[0]).toMatchObject({ band_type: "HighPass", gain: 0 }));
    expect(within(wh).queryByRole("menu")).toBeNull();
    // A pass filter has no gain to type
    expect(within(wh).getByRole("spinbutton", { name: t("eq.gain") })).toBeDisabled();
    expect(within(wh).getByRole("combobox", { name: t("eq.type") })).toHaveValue("HighPass");
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.reset") }));
    await waitFor(() => expect(lastEq()).toEqual({ enabled: true, bands: [] }));
  });

  it("takes typed values held to their range", async () => {
    const { wh, lastEq } = await eqOn();
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.add_point") }));
    await within(wh).findByRole("button", { name: /^Point 1:/ });
    const gain = within(wh).getByRole("spinbutton", { name: t("eq.gain") });
    fireEvent.change(gain, { target: { value: "40" } });
    await waitFor(() => expect(lastEq().bands[0].gain).toBe(24));
    fireEvent.change(within(wh).getByRole("spinbutton", { name: "Q" }), { target: { value: "2.5" } });
    await waitFor(() => expect(lastEq().bands[0].q).toBe(2.5));
  });

  it("saves a dragged point once it is let go", async () => {
    const { wh, svg, saved, lastEq } = await eqOn();
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.add_point") }));
    const point = await within(wh).findByRole("button", { name: /^Point 1:/ });
    const before = saved().length;
    fireEvent.pointerDown(point, { button: 0, pointerId: 1, clientX: 432, clientY: 114 });
    // The far left, the top: 20 Hz, +24 dB
    fireEvent.pointerMove(svg, { pointerId: 1, clientX: 10, clientY: 0 });
    fireEvent.pointerMove(svg, { pointerId: 1, clientX: 38, clientY: 10 });
    // Shown while dragged, not saved yet
    await within(wh).findByRole("button", { name: /^Point 1: Peak, 20 Hz, \+24.0 dB/ });
    expect(saved().length).toBe(before);
    fireEvent.pointerUp(svg, { pointerId: 1 });
    await waitFor(() => expect(lastEq().bands[0]).toMatchObject({ frequency: 20, gain: 24 }));
    expect(saved().length).toBe(before + 1);
  });

  it("turns off and keeps its points", async () => {
    const { wh, lastEq } = await eqOn();
    fireEvent.click(within(wh).getByRole("button", { name: t("eq.add_point") }));
    await waitFor(() => expect(lastEq().bands).toHaveLength(1));
    fireEvent.click(within(wh).getByRole("switch", { name: t("eq.title") }));
    await waitFor(() => expect(lastEq().enabled).toBe(false));
    expect(lastEq().bands).toHaveLength(1);
    await waitFor(() => expect(wh.querySelector(".eq-graph")).toBeNull());
  });
});
