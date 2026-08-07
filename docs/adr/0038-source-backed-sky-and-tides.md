# ADR 0038: The sky and tidal forcing are source-backed world physics

## Status

Accepted on 2026-08-07 for breadth-first implementation. SPK parsing, deterministic
numeric normalization, tidal-response coupling, and final scientific validation remain
required before canonical genesis.

## Context

The Sun and Moon are not scenery. Solar direction drives day, night, seasons,
temperature forcing, and photosynthesis; lunar phase affects nocturnal illumination;
the Moon and Sun both contribute to tides. Using a decorative calendar or scripted
phase cycle would contradict the real-world and unscripted-history contract.

## Decision

- Pin JPL DE441 planetary and lunar ephemeris kernels plus the IAU planetary constants
  kernel as source evidence. DE441 covers years -13,200 through +17,191 and includes the
  Sun, Earth, Moon, and planetary barycentres.
- Commit an astronomical epoch at genesis. Canonical celestial state is a pure function
  of that epoch, elapsed simulated seconds, the pinned kernels, and a versioned
  deterministic interpolation implementation. Wall clock, deployment pauses, and
  observer traffic never change the sky.
- Normalize kernel output at tick boundaries into fixed-scale integer position and
  distance values before it enters world physics. Replay never invokes a network API or
  a moving “latest ephemeris.”
- Derive local solar and lunar altitude, illumination, day length, season, and lunar
  phase from body vectors and pinned Earth orientation. Agents receive photons,
  temperature effects, shadows, and water movement—not labels such as “summer,” “full
  moon,” or “high tide.”
- Compute equilibrium Sun/Moon tidal potential first, then couple it to coastline,
  bathymetry, and water state through a separately versioned response model. The
  equilibrium potential is real astronomical forcing, not a claim of centimetre-accurate
  coastal tide prediction.
- Canonical execution may not silently extrapolate beyond DE441 coverage. A later
  source-backed kernel or versioned deterministic continuation model must be admitted
  before that boundary can be crossed.

## Consequences

`scripts/acquire-jpl-de441.py --download` can retain the two long-range SPK parts,
technical notes, checksum evidence, planetary constants, and the official NAIF rules
without an account. `--inventory-output` publishes one deterministic, no-replacement
aggregate manifest so the multi-file source can enter a provisional-world composition.
NAIF permits redistribution of unmodified kernels and commercial SPICE use under those
retained rules. The
simulation can eventually show the same sky to every observer while organisms discover
only its physical regularities. Scientific validation will compare selected epochs and
locations against an independent JPL/NAIF implementation before canonical admission.

References: [JPL DE441 generic kernels](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/),
[JPL astrodynamic parameters](https://ssd.jpl.nasa.gov/astro_par.html), and
[NAIF generic-kernel guidance](https://naif.jpl.nasa.gov/naif/data_generic.html).
