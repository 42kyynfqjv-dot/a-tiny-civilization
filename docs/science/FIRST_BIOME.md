# First biome: Lower Buffalo–Ozark river valley

## Status

Selected as the first data-pipeline and embodied-simulation target. This is **not** an
authorization to create the canonical public world. Exact source snapshots,
normalized parameters, licenses, assumptions, and the unpreviewed seed must pass the
gates below before genesis.

## Why this reference domain

The lower Buffalo River area in northern Arkansas provides a bounded but varied
temperate ecology without requiring extreme-winter survival in the first ruleset.
The National Park Service documents long hot summers, short mild winters, a roughly
200-day growing season, and lower reaches that usually retain flow year-round. It also
documents limestone, cherty limestone, dolostone, sandstone, shale, springs, alluvial
bottoms, gravel and sand.

The same watershed contains upland oak-hickory forest, riparian/floodplain forest,
glades, barrens, and native river-cane thickets. These adjacent habitats create real
differences in water, shelter, food, movement, fire, and material affordances without
requiring a continent-scale model.

Primary orientation sources:

- [NPS climate and geology](https://www.nps.gov/buff/learn/nature/climate-and-geology.htm)
- [NPS forests and plant communities](https://www.nps.gov/buff/learn/nature/forests.htm)
- [NPS rivers and streams](https://www.nps.gov/buff/learn/nature/rivers.htm)
- [NPS mammal inventory](https://www.nps.gov/buff/learn/nature/mammals.htm)
- [NPS fish inventory](https://www.nps.gov/buff/planyourvisit/fishing.htm)

## Model extent

The proposed first normalized world is 8.192 by 8.192 kilometres represented by a
256 by 256 environmental raster with 32-metre source cells. Organism locations and
interactions are not restricted to the cell centre; later rules use fixed-point local
coordinates.

The source domain is the lower-river district toward the White River confluence. The
normalized world will retain measured elevation, slope, drainage, soil, climate, and
habitat distributions, but it will not expose modern roads, buildings, parcels,
place names, protected cave locations, or archaeological locations. If geometry is
rotated, warped, or procedurally assembled to remove those features, that
transformation and its seed are first-class assumptions in the bundle.

This is a present-climate ecological counterfactual, not a depiction of an empty
wilderness, a prehistoric date, or a reconstruction of any Indigenous people. The
simulation models generic embodied agents learning inside an ecology; it does not
appropriate a real community's language, practices, artifacts, or history.

## Initial actual-world roster

The first bundle stays deliberately small. Each named entry is an actual Earth taxon
or material. Stable taxonomic identifiers will be resolved and validated during
ingestion rather than copied into this decision document by hand.

Candidate plants:

- white oak (*Quercus alba*);
- shagbark hickory (*Carya ovata*);
- pawpaw (*Asimina triloba*);
- American persimmon (*Diospyros virginiana*);
- giant river cane (*Arundinaria gigantea*).

Candidate individually modeled animals:

- white-tailed deer (*Odocoileus virginianus*);
- eastern cottontail (*Sylvilagus floridanus*);
- North American beaver (*Castor canadensis*);
- raccoon (*Procyon lotor*);
- wild turkey (*Meleagris gallopavo*);
- American black bear (*Ursus americanus*) as a rare boundary visitor.

Fish begin as species-aware schools rather than biographies; the initial candidate is
smallmouth bass (*Micropterus dolomieu*). Insects, fungi, microbes, small fish, and
background understory biomass begin as versioned ecological cohorts. A species must
move to durable individual identity before it can become supporter-name eligible.

Initial materials include freshwater, limestone, dolostone, chert, sandstone, quartz
sand and gravel, clay/silty alluvium, the listed plants' woods and tissues, bone,
antler, hide, and sinew. Charcoal, ash, cordage, fired clay, and shaped stone are
possible physical states or transformations—not predefined inventions or recipes.

## Data sources and normalization

Every downloaded input is retained by content digest alongside its upstream version,
retrieval date, geographic coverage, units, uncertainty, transformation code, and
license record.

| Domain | Intended authoritative input | Normalized use |
| --- | --- | --- |
| Elevation and slope | [USGS 3DEP / The National Map](https://www.usgs.gov/3d-elevation-program) | Integer elevation, slope and drainage parameters |
| Surface water | [USGS 3D Hydrography Program](https://www.usgs.gov/3d-hydrography-program) and retained legacy NHD metadata where needed | Flow topology, channel class and perennial/intermittent evidence |
| Soils | [USDA NRCS SSURGO](https://www.nrcs.usda.gov/resources/data-and-reports/soil-survey-geographic-database-ssurgo) | Texture, depth, drainage, water capacity and parent material |
| Climate | [NOAA 1991–2020 U.S. Climate Normals](https://www.ncei.noaa.gov/products/land-based-station/us-climate-normals) | Statistical weather-generation parameters, never a claim of historical weather |
| Habitat and vegetation | NPS inventories above plus a pinned, license-compatible land-cover source | Habitat proportions and species-presence evidence |
| Taxonomy | [ITIS](https://www.itis.gov/) and license-compatible [GBIF](https://www.gbif.org/) records | Stable taxon identity and source provenance |

Raw observations are never silently turned into convenient game values. Each
normalized parameter is marked as one of:

1. directly sourced measurement;
2. documented transformation of sourced measurements;
3. literature-supported approximation;
4. explicit engineering assumption awaiting stronger evidence.

Agent-facing perception contains measurable properties and effects—mass, dimensions,
colour, temperature, odour, taste, hardness, fracture response, moisture, flexibility,
combustion behavior, energy effect, toxicity symptoms—not modern labels such as
`edible`, `tool`, `medicine`, `prey`, or `building material`.

## Canonical-genesis gates

Before the first public seed is chosen:

- every entity has a stable identity, citation, units, license, and assumption status;
- every input artifact is locally archived and content-hashed;
- the normalized bundle validates deterministically and rebuilds byte-for-byte;
- material and species parameters pass range and dimensional-consistency checks;
- the identity tier of every population is fixed and event-volume tested;
- perception tests prove that privileged scientific and use labels cannot cross into
  agent inputs;
- a configured genesis and at least one simulated year replay to identical hashes on
  two clean runs;
- the observer projection can explain every displayed claim through source data or
  canonical events;
- the canonical seed procedure is published before an unpreviewed seed is generated.
