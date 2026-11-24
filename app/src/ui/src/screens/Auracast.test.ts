import { describe, expect, it } from "vitest";
import type { AudioDevice } from "../ipc/types";
import { followable, layoutOf, pairBigs, sourceOfInfo, sourceOptions, sourceParams } from "./Auracast";

describe("Auracast layout", () => {
  it("names the layout as the core takes it", () => {
    const c = { front: "stereo", rear: "separate" } as const;
    expect(layoutOf({ ...c, channels: "stereo" })).toBe("stereo");
    expect(layoutOf({ ...c, channels: "mono" })).toBeNull();
    expect(layoutOf({ ...c, channels: "split" })).toBe("split");
    expect(layoutOf({ ...c, channels: "matrix" })).toBe("matrix_stereo_separate");
  });

  it("counts the BIGs of each pair mode", () => {
    expect([pairBigs("stereo"), pairBigs("separate"), pairBigs("subgroups"), pairBigs("off")]).toEqual([1, 2, 1, 0]);
  });
});

describe("Auracast sources", () => {
  const dev = (id: string, name: string): AudioDevice => ({
    id,
    name,
    short_name: null,
    adapter_name: null,
    is_default: false,
    is_virtual: false,
    is_display: false,
    mix_rate: null,
    mix_channels: null,
    mix_bits: null,
    capture_name: null,
    capture_rate: null,
  });
  const outputs = [dev("{spk}", "Speakers")];
  const inputs = [dev("{mic}", "Microphone")];

  it("names a device with its id and name", () => {
    expect(sourceParams("tone", outputs, inputs)).toEqual({ source: "tone", source_id: null, source_name: null });
    expect(sourceParams("output:{spk}", outputs, inputs)).toEqual({
      source: "output",
      source_id: "{spk}",
      source_name: "Speakers",
    });
    expect(sourceParams("input:{mic}", outputs, inputs)).toEqual({
      source: "input",
      source_id: "{mic}",
      source_name: "Microphone",
    });
    // Gone: the core opens it by its id
    expect(sourceParams("input:{old}", outputs, inputs).source_name).toBeNull();
  });

  it("finds the choice of what plays", () => {
    const info = (kind: string, name = "") => ({ kind, name, waiting: false });
    expect(sourceOfInfo(info("loopback"), outputs, inputs)).toBe("loopback");
    expect(sourceOfInfo(info("output", "Speakers"), outputs, inputs)).toBe("output:{spk}");
    expect(sourceOfInfo(info("input", "Microphone"), outputs, inputs)).toBe("input:{mic}");
    expect(sourceOfInfo(info("input", "Headset"), outputs, inputs)).toBeNull();
    expect(sourceOfInfo(null, outputs, inputs)).toBeNull();
  });

  it("lists the devices under their headings, and one chosen that is gone", () => {
    const options = sourceOptions("tone", outputs, inputs);
    expect(options.map((o) => o.value)).toEqual(["tone", "loopback", "output:{spk}", "input:{mic}"]);
    expect(options[2].group).not.toEqual(options[3].group);
    const gone = sourceOptions("input:{old}", outputs, inputs);
    expect(gone.at(-1)).toMatchObject({ value: "input:{old}", group: options[3].group });
  });

  it("follows the Windows volume of output devices only", () => {
    expect(["tone", "loopback", "output:{spk}", "input:{mic}"].map((s) => followable(s as never))).toEqual([
      false,
      true,
      true,
      false,
    ]);
  });
});
