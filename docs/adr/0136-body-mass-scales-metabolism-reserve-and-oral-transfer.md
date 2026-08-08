# ADR 0136: Body mass scales metabolism, reserve, and oral transfer

Date: 2026-08-08

Status: Accepted

## Context

Ruleset 30 commits the same 100 W metabolic power, 10 MJ usable-energy reserve, 100 g glucose
transfer, and 250 g water transfer for every organism. That makes a 3.5 g hummingbird physically
equivalent to a human at the most causal survival boundary. Retaining real adult mass without using
it left the simulation less credible than the available evidence permits.

The openly licensed FmrBT paper and dataset publish separate endotherm and ectotherm Arrhenius
field-metabolic-rate fits. At the model's 293 K reference temperature, the temperature term is
exactly neutral and the fitted mass terms are:

- endotherm: `ln(a) = 1.94`, `b = 0.674`;
- ectotherm: `ln(a) = -2.11`, `b = 0.844`;
- output: kJ per individual per day for body mass in grams.

## Decision

Ruleset 31 derives a species-bound power commitment from the committed adult mass. Mammals,
humans, and every `aves_*` range shard use the endotherm fit; all other current fauna packages use
the ectotherm fit. The compiler uses the pinned pure-Rust `libm` implementation, converts the
published output to watts, and rounds once to the nearest microwatt. Replay never evaluates a
transcendental function: it consumes only the resulting signed integer and decimal scale already
recorded at genesis.

The estimate is a literature approximation only when adult mass is source-informed. If mass is an
engineering fallback, the metabolic commitment is also an engineering assumption. A direct exact
standardized metabolic observation, when one exists, still takes precedence.

Usable energy reserve becomes exactly seven simulation days of committed metabolic power, rounded
up to a joule. This seven-day duration remains an explicit engineering guardrail. Glucose and water
oral transfers become 1% of committed adult mass, rounded up to a milligram. Glucose retains the
existing explicit 16 J/mg engineering response and water retains the existing 21,600-second
hydration response. Reservoir existence and replenishment remain engineering assumptions.

Event, state-hash, and snapshot schemas advance to 31 even though payload shapes do not change.
Ruleset 30 replay remains unchanged.

## Consequences

Organism size now materially affects energy drain, reserve quantity, and transfer quantity without
exposing mass, metabolism, food, or hydration labels to agents. The change removes the universal
100 W defect but is not scientific admission: the reference-temperature choice, seven-day reserve,
pure-glucose response, water response, and uncovered masses remain disclosed assumptions. Ambient
temperature-dependent metabolism and real biomass/diet coupling remain later causal revisions.
