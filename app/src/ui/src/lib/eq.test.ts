import { describe, expect, it } from "vitest";
import type { EqPoint } from "../ipc/types";
import { eqResponseDb, formatFreq, formatGain, heldPoint, responseCurve, roundFreq, withType } from "./eq";

describe("EQ response", () => {
  // The same points and values as the core's test
  // (pipeline/src/eq.rs, response_table_shared_with_the_ui)
  const bands: EqPoint[] = [
    { frequency: 60, gain: 4.5, q: 0.7, band_type: "LowShelf" },
    { frequency: 250, gain: -3, q: 1.4, band_type: "Peaking" },
    { frequency: 2500, gain: 6, q: 0.8, band_type: "Peaking" },
    { frequency: 9000, gain: -5, q: 1, band_type: "HighShelf" },
    { frequency: 30, gain: 0, q: 0.71, band_type: "HighPass" },
    { frequency: 18000, gain: 0, q: 0.9, band_type: "LowPass" },
  ];
  const table: [number, number, number][] = [
    [44100, 20, -3.366949397],
    [44100, 2500, 5.976168683],
    [44100, 19000, -9.579220886],
    [48000, 45, 2.593260591],
    [48000, 250, -2.88454724],
    [48000, 1000, 1.484084636],
    [48000, 12000, -3.400823786],
    [96000, 100, 0.204033936],
    [96000, 6000, 1.171083516],
  ];

  it("matches the core's filters", () => {
    // Points as the core keeps them in f32
    const f32 = bands.map((b) => ({ ...b, q: Math.fround(b.q), gain: Math.fround(b.gain) }));
    for (const [rate, f, want] of table) {
      expect(eqResponseDb(f32, f, rate)).toBeCloseTo(want, 5);
    }
  });

  it("is flat without points, and a peak reaches its gain at its centre", () => {
    expect(eqResponseDb([], 1000)).toBe(0);
    const peak: EqPoint = { frequency: 1000, gain: 6, q: 1, band_type: "Peaking" };
    expect(eqResponseDb([peak], 1000)).toBeCloseTo(6, 3);
    // Under 0.05 dB: left out, as the core does
    expect(eqResponseDb([{ ...peak, gain: 0.01 }], 1000)).toBe(0);
    const curve = responseCurve([peak], 100);
    expect(curve).toHaveLength(101);
    expect(curve[0].f).toBeCloseTo(20);
    expect(curve[100].f).toBeCloseTo(20000);
  });
});

describe("EQ points", () => {
  it("round and hold values as the graph sets them", () => {
    expect(roundFreq(5)).toBe(20);
    expect(roundFreq(87.4)).toBe(87);
    expect(roundFreq(432)).toBe(430);
    expect(roundFreq(1234)).toBe(1250);
    expect(roundFreq(99999)).toBe(20000);
    expect(heldPoint({ frequency: 1, gain: 40, q: 0, band_type: "Peaking" })).toEqual({
      frequency: 20,
      gain: 24,
      q: 0.1,
      band_type: "Peaking",
    });
    expect(heldPoint({ frequency: 80, gain: 5, q: 0.7, band_type: "HighPass" }).gain).toBe(0);
  });

  it("drop the gain for a pass filter", () => {
    const p = withType({ frequency: 80, gain: 5, q: 0.2, band_type: "Peaking" }, "HighPass");
    expect(p).toEqual({ frequency: 80, gain: 0, q: 0.71, band_type: "HighPass" });
  });

  it("format", () => {
    expect(formatFreq(80)).toBe("80 Hz");
    expect(formatFreq(1250)).toBe("1.25 kHz");
    expect(formatFreq(12000)).toBe("12.0 kHz");
    expect(formatGain(3)).toBe("+3.0 dB");
    expect(formatGain(-2.5)).toBe("−2.5 dB");
    expect(formatGain(0)).toBe("0.0 dB");
  });
});
