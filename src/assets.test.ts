import { describe, expect, it } from "vitest";
import { fmtCompact, fmtPct, fmtThousands } from "./assets";

describe("asset formatters", () => {
  it("formats compact numbers", () => {
    expect(fmtCompact(563)).toBe("563");
    expect(fmtCompact(1659)).toBe("1.7k");
    expect(fmtCompact(1_250_000)).toBe("1.3m");
  });

  it("formats thousands", () => {
    expect(fmtThousands(1659)).toBe("1,659");
  });

  it("formats percentages", () => {
    expect(fmtPct(0.042)).toBe("4.2%");
  });
});
