import { commonSpeciesName } from "./speciesNames.ts";

export type LabelableOrganism = {
  organism_id: string;
  role: "person" | "fauna";
  species: { scientific_name: string };
  introduced_sequence?: string | number;
  introduced_tick: string | number;
};

/**
 * Stable observer-only numerical IDs ordered by first committed appearance.
 * These are finding-aid identifiers, never names known by the inhabitants.
 */
export function createPublicLifeLabels(organisms: LabelableOrganism[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const role of ["person", "fauna"] as const) {
    organisms
      .filter((organism) => organism.role === role)
      .sort((left, right) =>
        sequenceOf(left) - sequenceOf(right) || left.organism_id.localeCompare(right.organism_id),
      )
      .forEach((organism, index) => {
        const kind = role === "person" ? "Human" : commonSpeciesName(organism.species.scientific_name);
        labels.set(organism.organism_id, `${kind} ${index + 1}`);
      });
  }
  return labels;
}

function sequenceOf(organism: LabelableOrganism) {
  const candidate = organism.introduced_sequence ?? organism.introduced_tick;
  const parsed = Number(candidate);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}
