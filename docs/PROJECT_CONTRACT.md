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

Organisms are embodied products of evolution, not blank-slate random walkers. The
engine may provide species-appropriate physiological regulation and reflexive
capabilities such as hunger, thirst, swallowing, pain avoidance, locomotion,
reproductive physiology, and infant attachment. It must not provide culturally
contingent conclusions: which objects are edible, how to obtain or prepare them,
courtship strategies, partner choice, childcare practice, kinship, inheritance, or
explanations of reproduction must be learned from situated experience if they arise.
Primitive bodily capability is not privileged conceptual knowledge.

Real-world fidelity does not mean molecular or cellular simulation at every scale.
It means abstractions preserve the relevant measured causal behavior and never replace
real materials or organisms with convenient fantasy equivalents. Any unsupported
parameter is visibly marked as an assumption until an authoritative source replaces it.

The canonical spatial domain is the full present-geography Earth, not a hand-selected
biome enclosure. Global ecology may remain at a conserved coarse resolution while
organisms and physical effects cause deterministic regional and local refinement.
Observer attention can never activate canonical detail. Direct modern infrastructure,
borders, written labels, and place names do not enter the agent environment; ecological
reconstruction and unresolved physical legacies are explicit assumptions rather than
claims of pristine or prehistoric Earth.

Every person remains a durable individual at every population size. The engine may use
documented cohort representations for appropriate plants, microbes, insects, fish, or
fauna tiers, but it may not aggregate people because they are numerous or unobserved.
Execution is deterministically event-scheduled and spatially partitioned. Resource
pressure may slow wall-clock progress or pause after a committed hash boundary; it may
not modify fertility, mortality, cognition, detail, event recording, or random outcomes.
The project promises a correct resumable history and published capacity measurements,
not unlimited or twenty-billion-person real-time throughput.

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

Paid cognition is allocated in versioned units per simulated time, independent of
population and wall-clock execution speed. A separate wall-clock cost circuit breaker
protects the operator:

- target operating spend: USD 7.50 per calendar month;
- hard stop: USD 9.50 per calendar month;
- deterministic scheduling and reserve policy expressed per simulated year;
- a circuit-breaker trip is recorded as an unavailable external input and invokes the
  deterministic fallback.

Requests are selected at a deterministic tick and have a deterministic deadline tick.
Late responses cannot enter history. Exact model and memory-retrieval results—or their
recorded absence—are replay inputs.

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

Every durable record is scoped to a `world_id`. When the versioned mechanical
extinction condition is satisfied:

1. the world transitions exactly once to an immutable archived state;
2. its timeline, wiki, genealogy, artifacts, and final state remain browseable;
3. no records are overwritten or silently reinterpreted under new rules;
4. an authorized operator may later create a successor with a new, explicit,
   unpreviewed seed.

Extinction never creates a successor automatically. Humans may authorize a new world
after archival, but may not terminate a live world early, reroll an unattractive seed,
or rescue a doomed population.

Worlds pin a ruleset version. Random choices derive from explicit, independent streams
so unrelated implementation changes do not consume a shared random sequence.

## 7. Supporter participation

The public site is free to observe. A supporter may purchase an observer-label
reservation for the next naturally occurring eligible human or animal birth after the
reservation becomes valid, choosing an observer name and an available birth category
(and species for animals).

Payment and naming:

- never cause a birth or choose its biological outcome;
- attach only after the canonical birth event has committed;
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

Observer summaries, firsts, records, streaks, charts, and digests are deterministic,
versioned projections with links to their source events. They are finding aids rather
than inputs to history. The core observer experience does not use an LLM narrator.

Public presentation is restrained and non-explicit. The observatory and wiki never
depict, animate, or narrate sexual acts or violence. Reproduction, injury, predation,
and mortality may exist as abstract canonical mechanics, while public projections show
only necessary, age-appropriate outcomes such as pregnancy state, birth, injury,
death, and population change. Raw mechanism codes are not public copy. Presentation
policy never changes what agents perceive or what causally occurs.

## 9. Deployment and secrets

The initial production target is one Ubuntu server for a small population. Public HTTP
traffic reaches the web/API origin through Cloudflare Tunnel. PostgreSQL and internal
services are never published through the tunnel. Administrative surfaces may use
Cloudflare Access.

Additional deterministic partition workers may be added when measured simulation load
requires them. Worker count and ownership are operational and cannot alter canonical
ordering or state hashes.

Credentials are supplied at runtime, excluded from version control, scoped to the
least privilege possible, and documented through redacted examples.
