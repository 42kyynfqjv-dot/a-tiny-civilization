# Project Contract

This document records the product and simulation constraints that implementation
must preserve. A change that violates one of these rules requires an explicit
architecture decision and project-owner approval.

## 1. World ontology and knowledge

World entities are actual real-world materials, chemical substances, organisms,
species, and ecological relationships—not fictional substitutes or merely inspired
analogs. Their engine properties derive from documented measurements or explicitly
labeled scientific approximations. Sources, units, uncertainty, provenance, and the
ruleset version that transformed source data are retained alongside the model.

The engine may know the scientific identities and properties of those world entities.
Simulated people do not receive those identities.

For example, the engine may model a piece of flint using grounded fracture and
hardness parameters. A person can perceive its color, weight, resistance, shape,
and observed effects. They are not told that it is `flint`, that it is useful for
tools, or what a modern human would do with it.

Three layers remain distinct:

1. **World truth**: authoritative physical state and events.
2. **Situated knowledge**: perceptions, memories, associations, beliefs, and
   culturally transmitted claims available to particular beings.
3. **Observer interpretation**: retrospective wiki pages, classifications,
   translations, and causal reconstructions presented to site visitors.

Observer data is a one-way projection of simulation evidence. It is never a source
of perceptions, memories, prompts, or actions inside a world.

## 2. Causal openness

The project maximizes causal openness rather than raw randomness. Stable rules and
path-dependent histories should produce divergence without a scripted destination.

The engine must not contain:

- a technology tree or invention catalog used to guide behavior;
- encoded historical eras or progress scores;
- required discoveries, protected individuals, or rescue interventions;
- prompts asking a model to advance civilization or make history interesting;
- automatic civilization-wide knowledge propagation;
- stalled-progress bonuses or increased cognition budgets;
- outcome rules such as `clay -> pottery` or `flint -> tool`.

Physical affordances are allowed and necessary. Clay may have moisture-dependent
plasticity and heat-dependent material changes. Whether anyone notices, explains,
uses, teaches, loses, or mythologizes those effects is historical contingency.

Real-world fidelity does not mean molecular or cellular simulation at every scale.
It means abstractions preserve the relevant measured causal behavior and never replace
real materials or organisms with convenient fantasy equivalents. Any unsupported
parameter is visibly marked as an assumption until an authoritative source replaces it.

## 3. Cognition

Every person has a lightweight deterministic brain containing needs, traits,
relationships, learned policies, skills, memories, beliefs, goals, and attention.
External models are scheduled only for bounded candidate cognition.

Model responses:

- use small, versioned JSON schemas;
- can reference only entities and operations included in the request;
- must cite their experiential basis;
- are size-, cost-, time-, and frequency-bounded;
- are validated before becoming intentions or beliefs;
- never directly change objective state;
- are recorded so replay does not call a model again.

Unsupported conceptual leaps may become low-confidence fantasies. They do not become
discoveries or facts.

Paid cognition has a civilization-wide monthly treasury independent of population:

- target: USD 7.50;
- hard stop: USD 9.50;
- reserve below the hard stop for rare, high-value cognition.

Provider unavailability, malformed output, timeouts, missing credentials, or an
exhausted budget must degrade to deterministic behavior without stopping time.

## 4. Memory

PostgreSQL stores objective events and authoritative simulation state. Hindsight
stores and retrieves subjective durable memory behind a project-owned adapter.
Project-owned schemas remain the compatibility boundary so an unavailable or changed
Hindsight deployment cannot corrupt world truth or stop the simulation.

Memory is not a transcript. Records include provenance and retrieval metadata such
as agent, simulated time, place, participants, concepts, confidence, surprise,
curiosity, emotional salience, source, reinforcement, and decay state.

What happened, what was perceived, what is remembered, what is believed, and what a
culture teaches must be independently representable and mutually contradictory.

## 5. Artifacts and representation

People begin with no privileged concept of writing, papers, research, maps, counting,
libraries, citations, or institutions. The engine exposes only physical actions and
persistent effects. Marks, arrangements, copies, symbols, notation, writing, or
unknown representational systems may emerge—or never emerge.

The observer site may classify an object as an artifact and present competing
interpretations. That classification is not automatically known inside the world.
Every artifact page separates:

- physical reality;
- contemporary interpretations within the civilization;
- later in-world interpretations;
- observer reconstruction and confidence.

## 6. Worlds, extinction, and replay

Every durable record is scoped to a `world_id`. When no viable people remain:

1. the world transitions exactly once to an immutable archived state;
2. its timeline, wiki, genealogy, artifacts, and final state remain browseable;
3. a successor world is created with a new explicit seed;
4. no records are overwritten or silently reinterpreted under new rules.

Worlds pin a ruleset version. Random choices derive from explicit, independent streams
so unrelated implementation changes do not consume a shared random sequence.

## 7. Supporter participation

The public site is free to observe. A supporter may purchase an observer-label
reservation for the next naturally occurring eligible human or animal birth, choosing
an observer name and an available sex category (and species for animals).

Payment and naming:

- never cause a birth or choose its biological outcome;
- never alter behavior, cognition, status, health, reproduction, or survival;
- remain separate from names and identities developed inside the civilization;
- are moderated for abuse, harassment, personal data, impersonation, advertising,
  and unsafe content—not only profanity;
- disclose uncertain waiting time, death risk, extinction handling, and refund or
  transfer policy;
- remain attached to archived profiles after death.

Stripe is the intended processor. Apple Pay and Google Pay are checkout methods
through Stripe. Browser redirects never grant an entitlement; a verified,
idempotently processed webhook does.

## 8. Public observatory

The public site eventually provides:

- a live map and significant-event stream;
- a replayable historical timeline;
- people, animal, lineage, culture, place, material, event, and belief pages;
- an evidence-backed observer wiki;
- a dedicated archive for civilization-created artifacts;
- archived extinct worlds;
- a supporter area and naming queue.

Wiki claims carry provenance labels such as world fact, observed evidence,
contemporary claim, later interpretation, observer inference, or disputed.

## 9. Deployment and secrets

The initial production target is one Ubuntu server. Public HTTP traffic reaches the
web/API origin through Cloudflare Tunnel. PostgreSQL and internal services are never
published through the tunnel. Administrative surfaces may use Cloudflare Access.

Credentials are supplied at runtime, excluded from version control, scoped to the
least privilege possible, and documented through redacted examples.
