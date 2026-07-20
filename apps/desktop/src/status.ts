export type EnforcementLevel = "enforced" | "advisory" | "unavailable" | "warning" | "not_reliably_observed";

export const stateLabel = (level: EnforcementLevel): string => ({
  enforced: "Enforced", advisory: "Advisory", unavailable: "Unavailable", warning: "Warning", not_reliably_observed: "Not reliably observed"
}[level]);

export const isDangerousPolicyEdit = (editing: boolean): string => editing ? "Editing enabled — save validates before it writes." : "Read-only by default.";
