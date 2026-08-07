export type WorldInputMetadata = {
  input_status?: "provisional-not-scientifically-admitted";
  composition_id?: string;
  composition_version?: string;
  composition_hash?: string;
};

export function WorldInputStatus({ world }: { world: WorldInputMetadata }) {
  if (world.input_status !== "provisional-not-scientifically-admitted") return null;

  const composition = [world.composition_id, world.composition_version]
    .filter(Boolean)
    .join(" · ");

  return (
    <span
      className="provisional-world-status"
      title={world.composition_hash ? `Composition ${world.composition_hash}` : undefined}
    >
      <strong>Provisional — not scientifically admitted</strong>
      {composition && <small>{composition}</small>}
    </span>
  );
}
