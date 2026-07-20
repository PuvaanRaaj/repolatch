import { describe, expect, it } from "vitest";
import { isDangerousPolicyEdit, stateLabel } from "./status";
describe("security state labels", () => {
  it("does not soften not reliably observed", () => expect(stateLabel("not_reliably_observed")).toBe("Not reliably observed"));
  it("keeps policy read-only by default", () => expect(isDangerousPolicyEdit(false)).toBe("Read-only by default."));
});
