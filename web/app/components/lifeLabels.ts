export type LabelableOrganism = {
  organism_id: string;
  role: "person" | "fauna";
  introduced_sequence?: string | number;
  introduced_tick: string | number;
};

/**
 * Stable observer-only numerical IDs ordered by first committed appearance.
 * These are finding-aid identifiers, never names known by the inhabitants.
 */
export function createPublicLifeLabels(organisms: LabelableOrganism[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const [role, prefix] of [["person", "P"], ["fauna", "A"]] as const) {
    organisms
      .filter((organism) => organism.role === role)
      .sort((left, right) =>
        sequenceOf(left) - sequenceOf(right) || left.organism_id.localeCompare(right.organism_id),
      )
      .forEach((organism, index) => labels.set(organism.organism_id, `${prefix}-${String(index + 1).padStart(4, "0")}`));
  }
  return labels;
}

function sequenceOf(organism: LabelableOrganism) {
  const candidate = organism.introduced_sequence ?? organism.introduced_tick;
  const parsed = Number(candidate);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}
