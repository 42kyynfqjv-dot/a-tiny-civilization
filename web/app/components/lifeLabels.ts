export type LabelableOrganism = {
  organism_id: string;
  role: "person" | "fauna";
  introduced_sequence?: string | number;
  introduced_tick: string | number;
};

/**
 * Stable observer-only identifiers ordered by first committed appearance.
 * These are finding-aid labels, never names known by the inhabitants.
 */
export function createPublicLifeLabels(organisms: LabelableOrganism[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const [role, prefix] of [["person", "Person"], ["fauna", "Animal"]] as const) {
    organisms
      .filter((organism) => organism.role === role)
      .sort((left, right) =>
        sequenceOf(left) - sequenceOf(right) || left.organism_id.localeCompare(right.organism_id),
      )
      .forEach((organism, index) => labels.set(organism.organism_id, `${prefix} ${String(index + 1).padStart(2, "0")}`));
  }
  return labels;
}

function sequenceOf(organism: LabelableOrganism) {
  const candidate = organism.introduced_sequence ?? organism.introduced_tick;
  const parsed = Number(candidate);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}
