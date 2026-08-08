use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use drand_verify::{G2PubkeyRfc, Pubkey, derive_randomness};
use netcdf_reader::{NcAttrValue, NcFile, NcSliceInfo, NcSliceInfoElem, NcType};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tiff::decoder::{
    ChunkType as TiffChunkType, Decoder as TiffDecoder, DecodingResult as TiffDecodingResult,
};
use tiff::tags::Tag as TiffTag;
use uuid::Uuid;
use weezl::{BitOrder as LzwBitOrder, decode::Decoder as LzwDecoder};
use world_data::{
    BooleanFieldCell, COPERNICUS_LCCS_CLASSES, DataLayerKind, FAUNA_POPULATION_PLAN_SCHEMA_VERSION,
    FaunaBirthCategoryCount, FaunaMetabolicRatePlan, FaunaMetabolicRateSelection,
    FaunaPhysiologyProfileCatalog, FaunaPhysiologyProfileSet, FaunaPopulationPlan,
    FaunaPopulationPlanEntry, FaunaRangeCandidateSet, FaunaSeededSelection,
    LOCAL_FAUNA_OCCURRENCE_EVIDENCE_SCHEMA_VERSION, LandCoverClassCount, LandCoverEvidenceCell,
    LandCoverSignedValueCount, LocalFaunaOccurrenceEvidenceSet, LocalFaunaOccurrenceRecord,
    PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE, PACKED_LAND_COVER_EVIDENCE_TILE_MEDIA_TYPE,
    PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE, PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE,
    PACKED_SEASONAL_FIELD_TILE_MEDIA_TYPE, PACKED_SOILGRIDS_TOPSOIL_TILE_MEDIA_TYPE,
    PROVISIONAL_MATERIAL_RESOURCE_PLAN_SCHEMA_VERSION, PROVISIONAL_MATERIAL_RESOURCE_PLAN_STATUS,
    PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION,
    PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS, PackedBooleanFieldTile,
    PackedLandCoverEvidenceTile, PackedScalarFieldTile, PackedScalarTerrainTile,
    PackedSeasonalScalarFieldTile, PackedSoilGridsTopsoilTile, ProvisionalLandOriginSelection,
    ProvisionalMaterialResourcePlan, ProvisionalMaterialResourceSource,
    ProvisionalOrganismBodyProfileEntry, ProvisionalOrganismBodyProfilePlan,
    ProvisionalOriginEnvironment, SOILGRIDS_NO_DATA_VALUE, ScalarFieldCell, ScalarTerrainCell,
    SeasonalScalarFieldCell, SeasonalSourceArtifact, SoilDepth, SoilGridsProperty,
    SoilGridsPropertySource, SoilGridsQuantileValues, SoilGridsTopsoilCell, SourceSnapshotArtifact,
    SourceSnapshotManifest, TileArtifactReference, TileTreeEntry, TileTreeEntryKind, TileTreeIndex,
    WorldDataBundle, soilgrids_source_set_digest,
};
use world_data_filesystem::{
    load_provisional_world_composition, verify_provisional_world_artifacts,
    verify_release_artifacts, verify_source_snapshot_artifact, verify_source_snapshot_artifacts,
};
use world_domain::{
    ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION, AdultBodyMassCommitment, BirthCategory,
    CartesianMillimetres, CelestialState, Digest, GeographicCoordinateE7,
    GeographicCoordinateHalfArcsecond, HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
    HeritableDispositionProfile, MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION, MAX_S2_LEVEL,
    METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION, MaterialIdentity, MaterialReservoirCommitment,
    MetabolicRateCommitment, ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION, OffspringCategoryWeight,
    OralTransferCommitment, OralTransferEvidenceBasis,
    PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION, PhysiologicalEvidenceBasis,
    PhysiologicalRegulationCommitment, REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
    ReproductiveCategoryMaturityCommitment, ReproductiveCategoryPair,
    ReproductivePhysiologyCommitment, S2CellId, S2FaceUv, SpeciesIdentity, TdbSecondsSinceJ2000,
    WorldConfiguration, WorldId, WorldSeed, decode_s2_face_ij, route_geographic_to_s2,
    route_half_arcsecond_to_s2, s2_face_ij_center_uv, s2_face_ij_vertex_uv, s2_face_uv_to_ray,
    s2_ray_to_geographic_e7,
};

#[derive(Debug, Parser)]
#[command(name = "civilization-data")]
#[command(about = "Validate canonical scientific world-data bundles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate release completeness, canonical bytes, and optionally a world config.
    Validate {
        bundle: PathBuf,
        #[arg(long)]
        configuration: Option<PathBuf>,
    },
    /// Validate a provisional full-world composition and every referenced local artifact.
    ValidateProvisional {
        composition: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Acquire or verify exact pre-normalization scientific source bytes.
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Inspect exact source bytes without treating them as normalized world data.
    Inspect {
        #[command(subcommand)]
        command: InspectCommand,
    },
    /// Derive deterministic intermediate artifacts from verified scientific sources.
    Derive {
        #[command(subcommand)]
        command: DeriveCommand,
    },
    /// Commit to and resolve one unpreviewed public world seed from future drand randomness.
    Seed {
        #[command(subcommand)]
        command: SeedCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SeedCommand {
    /// Publish a future quicknet round before its randomness can exist.
    Commit {
        #[arg(long)]
        round: u64,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify the committed future beacon and derive the one accepted world seed.
    Resolve {
        #[arg(long)]
        commitment: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify committed and resolved seed artifacts offline and print their bound identity.
    Verify {
        #[arg(long)]
        commitment: PathBuf,
        #[arg(long)]
        resolution: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum SourceCommand {
    /// Verify every retained artifact without contacting the network.
    Validate {
        manifest: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Fetch missing artifacts over HTTPS, refusing to replace any existing file.
    Fetch {
        manifest: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum InspectCommand {
    /// Resolve one exact S2 cell centre to its WGS84 E7 geographic coordinate.
    S2Geographic {
        #[arg(long)]
        s2_cell_id: S2CellId,
    },
    /// Validate an exact point-scoped iNaturalist modeled-range candidate set.
    FaunaRangeCandidateSet {
        #[arg(long)]
        input: PathBuf,
    },
    /// Validate a canonical seed-derived fauna selection against its candidate pool.
    FaunaSeededSelection {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        candidates: PathBuf,
    },
    /// Validate a canonical source-pinned fauna physiology profile set.
    FaunaPhysiologyProfileSet {
        #[arg(long)]
        input: PathBuf,
    },
    /// Validate a canonical catalog of independently pinned fauna profile sets.
    FaunaPhysiologyProfileCatalog {
        #[arg(long)]
        input: PathBuf,
    },
    /// Validate and summarize a canonical provisional full-Earth composition.
    ProvisionalWorldComposition {
        #[arg(long)]
        input: PathBuf,
    },
    /// Validate one explicit provisional fauna population plan against its source
    /// candidate pool and seed-derived selection.
    FaunaPopulationPlan {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        origin_environment: PathBuf,
    },
    /// Recompute a seed-derived provisional land origin against its exact Natural
    /// Earth land-reference tile tree.
    ProvisionalLandOriginSelection {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        land_reference_root_index: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Read the pinned observed land cover and annual temperature evidence at a
    /// seed-derived provisional origin. This reports source evidence only; it
    /// never chooses a habitat, taxon, or population.
    ProvisionalOriginEnvironment {
        #[arg(long)]
        origin_selection: PathBuf,
        #[arg(long)]
        composition: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Report exact EltonTraits terrestrial-foraging coverage for one local
    /// modeled-range candidate set. This reports evidence; it does not populate.
    FaunaTerrestrialEvidence {
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        elton_birds: PathBuf,
    },
    /// Compare retained fauna-source names against the frozen accepted GBIF catalog.
    /// This only reports exact-name matches; it never guesses synonym mappings.
    FaunaTraitTaxa {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        animaltraits: PathBuf,
        #[arg(long)]
        elton_birds: PathBuf,
        #[arg(long)]
        elton_mammals: PathBuf,
    },
    /// Parse the pinned Natural Earth polygon stream into an auditable summary.
    NaturalEarthLand {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Evaluate one exact coordinate against the pinned Natural Earth land polygons.
    NaturalEarthLandPoint {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        latitude_e7: i32,
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Inspect the pinned ETOPO NetCDF schema through the portable Rust reader.
    Etopo {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Inspect the pinned CHELSA January-temperature NetCDF schema through the portable Rust reader.
    ChelsaJanuaryTemperature {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Inspect and cross-check all twelve pinned CHELSA monthly temperature normals.
    ChelsaAnnualTemperature {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Read one exact raw sample from the pinned CHELSA January-temperature grid.
    ChelsaJanuaryCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Zero-based source row in the retained latitude axis order.
        #[arg(long)]
        row: u64,
        /// Zero-based source column in the retained longitude axis order.
        #[arg(long)]
        column: u64,
    },
    /// Resolve an exact WGS 84 coordinate to its nearest retained CHELSA source cell.
    ChelsaNearestCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Latitude in exact 10^-7 degrees.
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees.
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Read all twelve retained CHELSA normal-month samples at one exact coordinate.
    ChelsaAnnualCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Latitude in exact 10^-7 degrees.
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees.
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Inspect the two NetCDF members inside one verified annual ERA5 ZIP response.
    Era5AnnualArchive {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value_t = 1981)]
        year: u16,
    },
    /// Inspect the verified 2022 Copernicus land-cover ZIP and NetCDF schema.
    CopernicusLandCover {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Census every Copernicus land-cover class and quality value globally.
    CopernicusLandCoverCensus {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Optionally publish deterministic pretty JSON to a new path.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Inspect fixed target-support samples for one S2 land-cover cell.
    CopernicusLandCoverTargetSupport {
        #[arg(long)]
        s2_cell_id: S2CellId,
        #[arg(long, default_value_t = 32)]
        points_per_axis: u8,
    },
    /// Sample one L10 cell from the verified Copernicus source and retain quality.
    CopernicusLandCoverCellEvidence {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        s2_cell_id: S2CellId,
        #[arg(long, default_value_t = 32)]
        points_per_axis: u8,
    },
    /// Route one exact WGS 84 geographic coordinate through the shared S2 contract.
    GeographicRoute {
        /// Latitude in exact 10^-7 degrees, within [-900000000, 900000000].
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees, within [-1800000000, 1800000000).
        #[arg(long)]
        longitude_e7: i32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
    },
    /// Route an exact ETOPO 2022 60-arc-second area-cell centre through shared S2 routing.
    EtopoCellRoute {
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
    },
    /// Inspect a deterministic interior quadrature of one ETOPO source area cell.
    EtopoCellQuadrature {
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Measure the exact interior-quadrature routing workload without writing data.
    EtopoQuadratureThroughput {
        /// Zero-based first source row, south to north.
        #[arg(long, default_value_t = 0)]
        start_row: u32,
        /// Consecutive complete source rows to route. This never creates output.
        #[arg(long, default_value_t = 1)]
        source_rows: u32,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Verify every artifact in a standalone packed ETOPO terrain layer release.
    EtopoTerrainLayer {
        /// Directory emitted by `derive etopo-terrain-layer`.
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "bedrock-relief")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Verify every artifact in a standalone packed Natural Earth land-reference release.
    NaturalEarthLandReferenceLayer {
        /// Directory emitted by `derive natural-earth-land-reference-layer`.
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "land-reference")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Verify every artifact in a packed Copernicus observed-land-cover release.
    CopernicusLandCoverLayer {
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "observed-land-cover")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 32)]
        points_per_axis: u8,
    },
    /// Inspect every retained SoilGrids topsoil overview without decoding a whole raster.
    SoilgridsTopsoil {
        /// Directory containing the nine property subdirectories acquired by the
        /// SoilGrids breadth-first acquisition script.
        #[arg(long)]
        input_directory: PathBuf,
    },
    /// Verify every artifact in a provisional SoilGrids topsoil release.
    SoilgridsTopsoilLayer {
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "soilgrids-topsoil")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Inspect the frozen GBIF Backbone archive inventory before taxon normalization.
    GbifBackbone {
        #[arg(long)]
        archive: PathBuf,
    },
    /// Verify every record in a compact derived accepted-Animalia catalog.
    GbifAnimaliaCatalog {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect every official JRC global surface-water occurrence tile.
    JrcSurfaceWaterOccurrence {
        #[arg(long)]
        input_directory: PathBuf,
    },
    /// Verify every artifact in a provisional JRC occurrence source-code release.
    JrcSurfaceWaterOccurrenceLayer {
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "observed-water-occurrence-source-code")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Verify every artifact in a provisional CHELSA monthly-temperature release.
    ChelsaAnnualTemperatureLayer {
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "near-surface-air-temperature-normal")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Inspect both retained JPL DE441 DAF/SPK segment directories.
    JplDe441 {
        #[arg(long)]
        input_directory: PathBuf,
    },
    /// Evaluate actual DE441 Sun/Earth/Moon positions at one exact integral TDB second.
    JplDe441Epoch {
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value_t = 0)]
        tdb_seconds_from_j2000: i64,
    },
}

#[derive(Debug, Subcommand)]
enum DeriveCommand {
    /// Select one source-confirmed land patch from the public world seed.
    ///
    /// This makes no habitat, population, or survivability assertion.
    ProvisionalLandOriginSelection {
        #[arg(long)]
        land_reference_root_index: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        world_seed: u64,
        #[arg(long, default_value_t = 23)]
        embodied_patch_level: u8,
        #[arg(long)]
        output: PathBuf,
    },
    /// Produce a bounded provisional founder plan from a seed-selected local range
    /// pool. Every selected real taxon receives one female and one male founder;
    /// this is an explicit engineering population assumption, not an abundance claim.
    ProvisionalFaunaPopulationPlan {
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        origin_environment: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Normalize retained research-grade, wild, commercially licensed iNaturalist
    /// observations near one point. This corroborates presence, never abundance,
    /// native status, habitat suitability, or an organism-creation decision.
    LocalFaunaOccurrenceEvidence {
        #[arg(long)]
        source_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Intersect modeled-range candidates with retained local observations.
    /// The result remains presence evidence, never abundance or native status.
    CorroboratedFaunaCandidates {
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        occurrence_evidence: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Pin the first canonical valid exact metabolic observation for each planned
    /// fauna species. This never averages, estimates, or selects by presentation order.
    FaunaMetabolicRatePlan {
        #[arg(long)]
        population_plan: PathBuf,
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        origin_environment: PathBuf,
        #[arg(long)]
        metabolic_profiles: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Construct explicit provisional body profiles for Homo sapiens and every
    /// planned fauna species. Optional metabolic inputs provide exact retained fauna
    /// observations; otherwise metabolism, regulation, reproduction, and heredity
    /// remain explicit engineering assumptions.
    ProvisionalOrganismBodyProfilePlan {
        #[arg(long)]
        population_plan: PathBuf,
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        origin_environment: PathBuf,
        /// Optional only as a pair with `--metabolic-rate-plan`. When omitted,
        /// metabolic commitments are explicitly engineering assumptions.
        #[arg(long, requires = "metabolic_rate_plan")]
        metabolic_profiles: Option<PathBuf>,
        /// Optional only as a pair with `--metabolic-profiles`. When omitted,
        /// metabolic commitments are explicitly engineering assumptions.
        #[arg(long, requires = "metabolic_profiles")]
        metabolic_rate_plan: Option<PathBuf>,
        /// Canonical exact life-history profiles. Missing category/species values
        /// remain explicit engineering assumptions.
        #[arg(long)]
        life_history_profiles: Option<PathBuf>,
        /// Canonical source-compiled physiology profiles. Exact adult-body-mass
        /// aggregates are retained as literature approximations; missing species
        /// receive explicit non-evidentiary assumptions.
        #[arg(long)]
        body_mass_profiles: Option<PathBuf>,
        #[arg(long, default_value_t = 300)]
        tick_duration_seconds: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Construct bounded real-material reservoirs and a neutral transformable object
    /// with explicit provisional availability assumptions for the founder plan.
    ProvisionalMaterialResourcePlan {
        #[arg(long)]
        population_plan: PathBuf,
        #[arg(long)]
        candidates: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        origin_environment: PathBuf,
        #[arg(long)]
        organism_body_profile_plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Join the pinned land-cover and climate cells at a seed-derived origin into
    /// one canonical evidence artifact. This is not a habitat or population plan.
    ProvisionalOriginEnvironment {
        #[arg(long)]
        origin_selection: PathBuf,
        #[arg(long)]
        composition: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Select a bounded, seed-derived subset of one exact local modeled-range pool.
    ///
    /// This creates no organisms and makes no abundance claim. It only makes the
    /// later population planner's source candidates and selection procedure auditable.
    FaunaSeededSelection {
        /// Canonical `FaunaRangeCandidateSet` bytes from the range-query tool.
        #[arg(long)]
        candidates: PathBuf,
        /// Publicly committed world seed used for the deterministic ranking.
        #[arg(long)]
        world_seed: u64,
        /// Maximum number of species candidates to retain. Must be nonzero.
        #[arg(long)]
        species_limit: u32,
        /// Restrict durable individual founders to source-ranged tetrapods.
        #[arg(long, default_value_t = false)]
        individual_fauna_only: bool,
        /// New output path. Existing artifacts are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Preserve a regular, evenly sampled ETOPO elevation grid as portable canonical bytes.
    ///
    /// This is a provenance-bound intermediate artifact, not yet a canonical world-data
    /// layer or a claim that full-Earth genesis is available.
    EtopoGrid {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Sampling spacing in ETOPO's native one-arc-minute cells. Must divide 60.
        #[arg(long, default_value_t = 5)]
        sample_arc_minutes: u16,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Pair sampled ETOPO source values with their exact centre-routed S2 addresses.
    ///
    /// This is a provenance-bound intermediate index, not a canonical elevation layer.
    EtopoCentreIndex {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Sampling spacing in ETOPO's native one-arc-minute cells. Must divide 60.
        #[arg(long, default_value_t = 5)]
        sample_arc_minutes: u16,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Summarize a verified ETOPO centre-attribution index at a coarser S2 level.
    ///
    /// This is a source-centre quadrature summary, not a target-cell coverage or
    /// area-overlap terrain layer.
    CentreSummary {
        /// Existing ETOPO centre index to read and validate.
        #[arg(long)]
        input: PathBuf,
        /// S2 level to summarize into. It must not be finer than the input level.
        #[arg(long)]
        s2_level: u8,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Derive the exact weighted S2 contribution of one verified ETOPO source cell.
    ///
    /// This is a normalizer-kernel probe, not a complete global terrain layer.
    EtopoQuadratureContribution {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Normalize the complete pinned ETOPO source into packed L6→L10 terrain tiles.
    ///
    /// This is deliberately a long offline batch. It emits only an ETOPO terrain-layer
    /// root, not a complete world-data bundle or canonical world.
    EtopoTerrainLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Layer identifier, normally bedrock-relief.
        #[arg(long, default_value = "bedrock-relief")]
        layer_id: String,
        /// New empty release directory. Existing paths are never reused.
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Classify every L10 centre against pinned Natural Earth land polygons.
    ///
    /// The source is a generalized land reference, so this creates a clearly named
    /// reference layer, never a claim of a measurement-resolution coastline.
    NaturalEarthLandReferenceLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "land-reference")]
        layer_id: String,
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Normalize Copernicus 2022 into mixed-class, quality-preserving L6→L10 tiles.
    CopernicusLandCoverLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "observed-land-cover")]
        layer_id: String,
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 32)]
        points_per_axis: u8,
        /// Maximum decompressed 2,025 × 2,025 source chunks retained in memory.
        #[arg(long, default_value_t = 32)]
        source_chunk_cache: usize,
    },
    /// Centre-sample the retained SoilGrids topsoil overview set into global L10 tiles.
    SoilgridsTopsoilLayer {
        /// Stable hash inventory emitted by the SoilGrids acquisition helper.
        #[arg(long)]
        source_inventory: PathBuf,
        /// Root containing inventory artifact paths, normally `data/source-cache`.
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "soilgrids-topsoil")]
        layer_id: String,
        /// New release directory. An interrupted hidden staging tree is resumable.
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
    /// Stream the frozen GBIF core into a compact catalog of accepted Animalia species.
    GbifAnimaliaCatalog {
        #[arg(long)]
        archive: PathBuf,
        /// New output path. Existing artifacts are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Centre-sample retained JRC occurrence codes into a provisional global L10 field.
    JrcSurfaceWaterOccurrenceLayer {
        /// Stable hash inventory emitted by the JRC acquisition helper.
        #[arg(long)]
        source_inventory: PathBuf,
        /// Root containing inventory artifact paths, normally `data/source-cache`.
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "observed-water-occurrence-source-code")]
        layer_id: String,
        /// New release directory. An interrupted hidden staging tree is resumable.
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Number of parsed source TIFF strip tables retained in memory.
        #[arg(long, default_value_t = 16)]
        source_raster_cache: usize,
    },
    /// Centre-sample all twelve retained CHELSA temperature normals into global L10 fields.
    ChelsaAnnualTemperatureLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "near-surface-air-temperature-normal")]
        layer_id: String,
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Number of complete 12-month 500x500 source chunks retained in memory.
        #[arg(long, default_value_t = 8)]
        source_chunk_cache: usize,
    },
}

const PUBLIC_SEED_SCHEMA_VERSION: u16 = 1;
const QUICKNET_CHAIN_HASH: &str =
    "52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971";
const QUICKNET_PUBLIC_KEY: &str = "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";
const QUICKNET_SCHEME: &str = "bls-unchained-g1-rfc9380";
const QUICKNET_GENESIS_UNIX_SECONDS: u64 = 1_692_803_367;
const QUICKNET_PERIOD_SECONDS: u64 = 3;
const MINIMUM_UNREVEALED_ROUNDS: u64 = 200;
const PUBLIC_SEED_DERIVATION_DOMAIN: &str = "a-tiny-civilization/world-seed/v1";
const DRAND_RESPONSE_LIMIT_BYTES: usize = 16 * 1024;
const DRAND_RELAYS: [&str; 2] = ["https://api.drand.sh", "https://drand.cloudflare.com"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DrandBeacon {
    round: u64,
    randomness: String,
    signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_signature: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicSeedCommitment {
    schema_version: u16,
    chain_hash: String,
    public_key: String,
    scheme: String,
    period_seconds: u64,
    genesis_unix_seconds: u64,
    target_round: u64,
    target_unix_seconds: u64,
    minimum_unrevealed_rounds: u64,
    observed_beacon: DrandBeacon,
    derivation_domain: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ResolvedPublicSeed {
    schema_version: u16,
    commitment_digest: Digest,
    target_beacon: DrandBeacon,
    derivation_domain: String,
    derivation_digest: Digest,
    world_seed: WorldSeed,
    world_id: WorldId,
    verified_relays: [String; 2],
}

async fn commit_public_world_seed(round: u64, output: &Path) -> Result<()> {
    let latest = fetch_latest_verified_quicknet_beacon().await?;
    let minimum_round = latest
        .round
        .checked_add(MINIMUM_UNREVEALED_ROUNDS)
        .context("minimum future drand round overflow")?;
    if round < minimum_round {
        bail!(
            "target drand round {round} must be at least {MINIMUM_UNREVEALED_ROUNDS} rounds after verified round {}",
            latest.round
        );
    }
    let commitment = PublicSeedCommitment {
        schema_version: PUBLIC_SEED_SCHEMA_VERSION,
        chain_hash: QUICKNET_CHAIN_HASH.to_owned(),
        public_key: QUICKNET_PUBLIC_KEY.to_owned(),
        scheme: QUICKNET_SCHEME.to_owned(),
        period_seconds: QUICKNET_PERIOD_SECONDS,
        genesis_unix_seconds: QUICKNET_GENESIS_UNIX_SECONDS,
        target_round: round,
        target_unix_seconds: quicknet_round_unix_seconds(round)?,
        minimum_unrevealed_rounds: MINIMUM_UNREVEALED_ROUNDS,
        observed_beacon: latest,
        derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
    };
    validate_public_seed_commitment(&commitment)?;
    write_pretty_json_artifact(output, &commitment)?;
    println!(
        "committed future quicknet round {} at Unix second {}; publish this artifact before resolution",
        commitment.target_round, commitment.target_unix_seconds
    );
    Ok(())
}

async fn resolve_public_world_seed(commitment_path: &Path, output: &Path) -> Result<()> {
    let bytes = fs::read(commitment_path)
        .with_context(|| format!("failed to read {}", commitment_path.display()))?;
    let commitment: PublicSeedCommitment = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to decode {}", commitment_path.display()))?;
    validate_public_seed_commitment(&commitment)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock precedes Unix epoch")?
        .as_secs();
    if now < commitment.target_unix_seconds {
        bail!(
            "committed drand round {} is not due until Unix second {}",
            commitment.target_round,
            commitment.target_unix_seconds
        );
    }
    let beacon = fetch_matching_quicknet_round(commitment.target_round).await?;
    let (world_seed, derivation_digest) = derive_public_world_seed(&beacon)?;
    let resolved = ResolvedPublicSeed {
        schema_version: PUBLIC_SEED_SCHEMA_VERSION,
        commitment_digest: Digest::canonical(&commitment)?,
        target_beacon: beacon,
        derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
        derivation_digest,
        world_seed,
        world_id: derive_public_world_id(derivation_digest),
        verified_relays: DRAND_RELAYS.map(str::to_owned),
    };
    write_pretty_json_artifact(output, &resolved)?;
    println!(
        "resolved committed round {} to world seed {}",
        resolved.target_beacon.round, resolved.world_seed
    );
    Ok(())
}

fn verify_resolved_public_seed(commitment_path: &Path, resolution_path: &Path) -> Result<()> {
    let commitment_bytes = fs::read(commitment_path)
        .with_context(|| format!("failed to read {}", commitment_path.display()))?;
    let commitment: PublicSeedCommitment = serde_json::from_slice(&commitment_bytes)
        .with_context(|| format!("failed to decode {}", commitment_path.display()))?;
    validate_public_seed_commitment(&commitment)?;
    let resolution_bytes = fs::read(resolution_path)
        .with_context(|| format!("failed to read {}", resolution_path.display()))?;
    let resolution: ResolvedPublicSeed = serde_json::from_slice(&resolution_bytes)
        .with_context(|| format!("failed to decode {}", resolution_path.display()))?;
    validate_resolved_public_seed(&commitment, &resolution)?;
    println!("{} {}", resolution.world_id, resolution.world_seed);
    Ok(())
}

fn validate_resolved_public_seed(
    commitment: &PublicSeedCommitment,
    resolution: &ResolvedPublicSeed,
) -> Result<()> {
    let (world_seed, derivation_digest) = derive_public_world_seed(&resolution.target_beacon)?;
    let expected = ResolvedPublicSeed {
        schema_version: PUBLIC_SEED_SCHEMA_VERSION,
        commitment_digest: Digest::canonical(&commitment)?,
        target_beacon: resolution.target_beacon.clone(),
        derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
        derivation_digest,
        world_seed,
        world_id: derive_public_world_id(derivation_digest),
        verified_relays: DRAND_RELAYS.map(str::to_owned),
    };
    if resolution.target_beacon.round != commitment.target_round || resolution != &expected {
        bail!("public seed resolution does not exactly match its commitment and derivation");
    }
    Ok(())
}

async fn fetch_latest_verified_quicknet_beacon() -> Result<DrandBeacon> {
    let latest = fetch_quicknet_beacon(DRAND_RELAYS[0], "latest").await?;
    let matching = fetch_matching_quicknet_round(latest.round).await?;
    if latest != matching {
        bail!("latest drand response differs from the verified exact-round response");
    }
    Ok(matching)
}

async fn fetch_matching_quicknet_round(round: u64) -> Result<DrandBeacon> {
    let resource = round.to_string();
    let first = fetch_quicknet_beacon(DRAND_RELAYS[0], &resource).await?;
    let second = fetch_quicknet_beacon(DRAND_RELAYS[1], &resource).await?;
    if first != second {
        bail!("drand relays returned different bytes for quicknet round {round}");
    }
    verify_quicknet_beacon(&first)?;
    Ok(first)
}

async fn fetch_quicknet_beacon(relay: &str, resource: &str) -> Result<DrandBeacon> {
    let endpoint = format!("{relay}/{QUICKNET_CHAIN_HASH}/public/{resource}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build drand HTTP client")?;
    let response = client
        .get(&endpoint)
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("request drand relay {relay}"))?
        .error_for_status()
        .with_context(|| format!("drand relay {relay} rejected the request"))?;
    if response
        .content_length()
        .is_some_and(|length| length > DRAND_RESPONSE_LIMIT_BYTES as u64)
    {
        bail!("drand relay response exceeds the bounded size");
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("read drand relay {relay}"))?;
    if bytes.len() > DRAND_RESPONSE_LIMIT_BYTES {
        bail!("drand relay response exceeds the bounded size");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("decode drand relay {relay}"))
}

fn validate_public_seed_commitment(commitment: &PublicSeedCommitment) -> Result<()> {
    if commitment.schema_version != PUBLIC_SEED_SCHEMA_VERSION
        || commitment.chain_hash != QUICKNET_CHAIN_HASH
        || commitment.public_key != QUICKNET_PUBLIC_KEY
        || commitment.scheme != QUICKNET_SCHEME
        || commitment.period_seconds != QUICKNET_PERIOD_SECONDS
        || commitment.genesis_unix_seconds != QUICKNET_GENESIS_UNIX_SECONDS
        || commitment.minimum_unrevealed_rounds != MINIMUM_UNREVEALED_ROUNDS
        || commitment.derivation_domain != PUBLIC_SEED_DERIVATION_DOMAIN
    {
        bail!("public seed commitment differs from the pinned procedure");
    }
    if commitment.target_unix_seconds != quicknet_round_unix_seconds(commitment.target_round)? {
        bail!("public seed commitment has an inconsistent target time");
    }
    verify_quicknet_beacon(&commitment.observed_beacon)?;
    let minimum_round = commitment
        .observed_beacon
        .round
        .checked_add(MINIMUM_UNREVEALED_ROUNDS)
        .context("committed future round overflow")?;
    if commitment.target_round < minimum_round {
        bail!("public seed commitment did not precede its beacon by the required interval");
    }
    Ok(())
}

fn quicknet_round_unix_seconds(round: u64) -> Result<u64> {
    let elapsed_rounds = round
        .checked_sub(1)
        .context("drand round zero is invalid")?;
    QUICKNET_GENESIS_UNIX_SECONDS
        .checked_add(
            elapsed_rounds
                .checked_mul(QUICKNET_PERIOD_SECONDS)
                .context("drand round time overflow")?,
        )
        .context("drand round time overflow")
}

fn verify_quicknet_beacon(beacon: &DrandBeacon) -> Result<()> {
    if beacon.round == 0
        || beacon
            .previous_signature
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    {
        bail!("quicknet beacon must be a nonzero unchained round");
    }
    let public_key = decode_hex_length(QUICKNET_PUBLIC_KEY, 96, "quicknet public key")?;
    let signature = decode_hex_length(&beacon.signature, 48, "quicknet signature")?;
    let randomness = decode_hex_length(&beacon.randomness, 32, "quicknet randomness")?;
    let public_key = G2PubkeyRfc::from_variable(&public_key)
        .map_err(|error| anyhow::anyhow!("invalid quicknet public key: {error}"))?;
    let verified = public_key
        .verify(beacon.round, b"", &signature)
        .map_err(|error| anyhow::anyhow!("quicknet signature verification failed: {error}"))?;
    if !verified {
        bail!("quicknet signature does not verify");
    }
    if derive_randomness(&signature).as_slice() != randomness.as_slice() {
        bail!("quicknet randomness is not SHA-256 of its verified signature");
    }
    Ok(())
}

fn derive_public_world_seed(beacon: &DrandBeacon) -> Result<(WorldSeed, Digest)> {
    verify_quicknet_beacon(beacon)?;
    let preimage = seed_derivation_preimage(beacon)?;
    let digest: [u8; 32] = Sha256::digest(preimage).into();
    let seed_bytes: [u8; 8] = digest[..8]
        .try_into()
        .context("world seed derivation prefix has the wrong length")?;
    Ok((
        WorldSeed::new(u64::from_be_bytes(seed_bytes)),
        Digest::from_bytes(digest),
    ))
}

fn derive_public_world_id(derivation_digest: Digest) -> WorldId {
    let name = format!("https://atinycivilization.com/worlds/{}", derivation_digest);
    WorldId::from_uuid(Uuid::new_v5(&Uuid::NAMESPACE_URL, name.as_bytes()))
}

fn seed_derivation_preimage(beacon: &DrandBeacon) -> Result<Vec<u8>> {
    let chain_hash = decode_hex_length(QUICKNET_CHAIN_HASH, 32, "quicknet chain hash")?;
    let randomness = decode_hex_length(&beacon.randomness, 32, "quicknet randomness")?;
    let mut preimage = Vec::with_capacity(PUBLIC_SEED_DERIVATION_DOMAIN.len() + 1 + 32 + 8 + 32);
    preimage.extend_from_slice(PUBLIC_SEED_DERIVATION_DOMAIN.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&chain_hash);
    preimage.extend_from_slice(&beacon.round.to_be_bytes());
    preimage.extend_from_slice(&randomness);
    Ok(preimage)
}

fn decode_hex_length(value: &str, expected_bytes: usize, field: &str) -> Result<Vec<u8>> {
    if value.len() != expected_bytes * 2
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        bail!("{field} is not canonical lowercase hex of the required length");
    }
    hex::decode(value).with_context(|| format!("decode {field}"))
}

fn write_pretty_json_artifact(output: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).context("encode public seed artifact")?;
    bytes.push(b'\n');
    write_new_artifact(output, &bytes)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate {
            bundle,
            configuration,
        } => validate(bundle, configuration.as_ref()),
        Command::ValidateProvisional {
            composition,
            artifact_root,
        } => validate_provisional_world(&composition, &artifact_root),
        Command::Seed { command } => match command {
            SeedCommand::Commit { round, output } => commit_public_world_seed(round, &output).await,
            SeedCommand::Resolve { commitment, output } => {
                resolve_public_world_seed(&commitment, &output).await
            }
            SeedCommand::Verify {
                commitment,
                resolution,
            } => verify_resolved_public_seed(&commitment, &resolution),
        },
        Command::Source { command } => match command {
            SourceCommand::Validate {
                manifest,
                artifact_root,
            } => validate_source(&manifest, &artifact_root),
            SourceCommand::Fetch {
                manifest,
                artifact_root,
            } => fetch_source(&manifest, &artifact_root).await,
        },
        Command::Inspect { command } => match command {
            InspectCommand::S2Geographic { s2_cell_id } => inspect_s2_geographic(s2_cell_id),
            InspectCommand::FaunaRangeCandidateSet { input } => {
                inspect_fauna_range_candidate_set(&input)
            }
            InspectCommand::FaunaSeededSelection { input, candidates } => {
                inspect_fauna_seeded_selection(&input, &candidates)
            }
            InspectCommand::FaunaPhysiologyProfileSet { input } => {
                inspect_fauna_physiology_profile_set(&input)
            }
            InspectCommand::FaunaPhysiologyProfileCatalog { input } => {
                inspect_fauna_physiology_profile_catalog(&input)
            }
            InspectCommand::ProvisionalWorldComposition { input } => {
                inspect_provisional_world_composition(&input)
            }
            InspectCommand::FaunaPopulationPlan {
                input,
                candidates,
                selection,
                origin_environment,
            } => {
                inspect_fauna_population_plan(&input, &candidates, &selection, &origin_environment)
            }
            InspectCommand::ProvisionalLandOriginSelection {
                input,
                land_reference_root_index,
                artifact_root,
            } => inspect_provisional_land_origin_selection(
                &input,
                &land_reference_root_index,
                &artifact_root,
            ),
            InspectCommand::ProvisionalOriginEnvironment {
                origin_selection,
                composition,
                artifact_root,
            } => inspect_provisional_origin_environment(
                &origin_selection,
                &composition,
                &artifact_root,
            ),
            InspectCommand::FaunaTerrestrialEvidence {
                candidates,
                elton_birds,
            } => inspect_fauna_terrestrial_evidence(&candidates, &elton_birds),
            InspectCommand::NaturalEarthLand {
                source_snapshot,
                artifact_root,
            } => inspect_natural_earth_land(&source_snapshot, &artifact_root),
            InspectCommand::NaturalEarthLandPoint {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_natural_earth_land_point(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::Etopo {
                source_snapshot,
                artifact_root,
            } => inspect_etopo(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaJanuaryTemperature {
                source_snapshot,
                artifact_root,
            } => inspect_chelsa_january_temperature(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaAnnualTemperature {
                source_snapshot,
                artifact_root,
            } => inspect_chelsa_annual_temperature(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaJanuaryCell {
                source_snapshot,
                artifact_root,
                row,
                column,
            } => inspect_chelsa_january_cell(&source_snapshot, &artifact_root, row, column),
            InspectCommand::ChelsaNearestCell {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_chelsa_nearest_cell(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::ChelsaAnnualCell {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_chelsa_annual_cell(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::Era5AnnualArchive {
                source_snapshot,
                artifact_root,
                year,
            } => inspect_era5_annual_archive(&source_snapshot, &artifact_root, year),
            InspectCommand::CopernicusLandCover {
                source_snapshot,
                artifact_root,
            } => inspect_copernicus_land_cover(&source_snapshot, &artifact_root),
            InspectCommand::CopernicusLandCoverCensus {
                source_snapshot,
                artifact_root,
                output,
            } => inspect_copernicus_land_cover_census(
                &source_snapshot,
                &artifact_root,
                output.as_deref(),
            ),
            InspectCommand::CopernicusLandCoverTargetSupport {
                s2_cell_id,
                points_per_axis,
            } => inspect_copernicus_land_cover_target_support(s2_cell_id, points_per_axis),
            InspectCommand::CopernicusLandCoverCellEvidence {
                source_snapshot,
                artifact_root,
                s2_cell_id,
                points_per_axis,
            } => inspect_copernicus_land_cover_cell_evidence(
                &source_snapshot,
                &artifact_root,
                s2_cell_id,
                points_per_axis,
            ),
            InspectCommand::GeographicRoute {
                latitude_e7,
                longitude_e7,
                s2_level,
            } => inspect_geographic_route(latitude_e7, longitude_e7, s2_level),
            InspectCommand::EtopoCellRoute {
                row,
                column,
                s2_level,
            } => inspect_etopo_cell_route(row, column, s2_level),
            InspectCommand::EtopoCellQuadrature {
                row,
                column,
                s2_level,
                points_per_axis,
            } => inspect_etopo_cell_quadrature(row, column, s2_level, points_per_axis),
            InspectCommand::EtopoQuadratureThroughput {
                start_row,
                source_rows,
                target_s2_level,
                points_per_axis,
            } => inspect_etopo_quadrature_throughput(
                start_row,
                source_rows,
                target_s2_level,
                points_per_axis,
            ),
            InspectCommand::EtopoTerrainLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            } => inspect_etopo_terrain_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            ),
            InspectCommand::NaturalEarthLandReferenceLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
            } => inspect_natural_earth_land_reference_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
            ),
            InspectCommand::CopernicusLandCoverLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            } => inspect_copernicus_land_cover_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            ),
            InspectCommand::SoilgridsTopsoil { input_directory } => {
                inspect_soilgrids_topsoil(&input_directory)
            }
            InspectCommand::SoilgridsTopsoilLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
            } => inspect_soilgrids_topsoil_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
            ),
            InspectCommand::GbifBackbone { archive } => inspect_gbif_backbone(&archive),
            InspectCommand::GbifAnimaliaCatalog { input } => inspect_gbif_animalia_catalog(&input),
            InspectCommand::FaunaTraitTaxa {
                catalog,
                animaltraits,
                elton_birds,
                elton_mammals,
            } => inspect_fauna_trait_taxa(&catalog, &animaltraits, &elton_birds, &elton_mammals),
            InspectCommand::JrcSurfaceWaterOccurrence { input_directory } => {
                inspect_jrc_surface_water_occurrence(&input_directory)
            }
            InspectCommand::JrcSurfaceWaterOccurrenceLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
            } => inspect_jrc_surface_water_occurrence_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
            ),
            InspectCommand::ChelsaAnnualTemperatureLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
            } => inspect_chelsa_annual_temperature_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
            ),
            InspectCommand::JplDe441 { input_directory } => inspect_jpl_de441(&input_directory),
            InspectCommand::JplDe441Epoch {
                input_directory,
                tdb_seconds_from_j2000,
            } => inspect_jpl_de441_epoch(&input_directory, tdb_seconds_from_j2000),
        },
        Command::Derive { command } => match command {
            DeriveCommand::ProvisionalLandOriginSelection {
                land_reference_root_index,
                artifact_root,
                world_seed,
                embodied_patch_level,
                output,
            } => derive_provisional_land_origin_selection(
                &land_reference_root_index,
                &artifact_root,
                WorldSeed::new(world_seed),
                embodied_patch_level,
                &output,
            ),
            DeriveCommand::ProvisionalOriginEnvironment {
                origin_selection,
                composition,
                artifact_root,
                output,
            } => derive_and_write_provisional_origin_environment(
                &origin_selection,
                &composition,
                &artifact_root,
                &output,
            ),
            DeriveCommand::FaunaSeededSelection {
                candidates,
                world_seed,
                species_limit,
                individual_fauna_only,
                output,
            } => derive_fauna_seeded_selection(
                &candidates,
                WorldSeed::new(world_seed),
                species_limit,
                individual_fauna_only,
                &output,
            ),
            DeriveCommand::ProvisionalFaunaPopulationPlan {
                candidates,
                selection,
                origin_environment,
                output,
            } => derive_provisional_fauna_population_plan(
                &candidates,
                &selection,
                &origin_environment,
                &output,
            ),
            DeriveCommand::LocalFaunaOccurrenceEvidence {
                source_directory,
                output,
            } => derive_local_fauna_occurrence_evidence(&source_directory, &output),
            DeriveCommand::CorroboratedFaunaCandidates {
                candidates,
                occurrence_evidence,
                output,
            } => derive_corroborated_fauna_candidates(&candidates, &occurrence_evidence, &output),
            DeriveCommand::FaunaMetabolicRatePlan {
                population_plan,
                candidates,
                selection,
                origin_environment,
                metabolic_profiles,
                output,
            } => derive_fauna_metabolic_rate_plan(
                &population_plan,
                &candidates,
                &selection,
                &origin_environment,
                &metabolic_profiles,
                &output,
            ),
            DeriveCommand::ProvisionalOrganismBodyProfilePlan {
                population_plan,
                candidates,
                selection,
                origin_environment,
                metabolic_profiles,
                metabolic_rate_plan,
                life_history_profiles,
                body_mass_profiles,
                tick_duration_seconds,
                output,
            } => derive_provisional_organism_body_profile_plan(
                PopulationPlanInputPaths {
                    population_plan: &population_plan,
                    candidates: &candidates,
                    selection: &selection,
                    origin_environment: &origin_environment,
                },
                metabolic_profiles.as_deref(),
                metabolic_rate_plan.as_deref(),
                life_history_profiles.as_deref(),
                body_mass_profiles.as_deref(),
                tick_duration_seconds,
                &output,
            ),
            DeriveCommand::ProvisionalMaterialResourcePlan {
                population_plan,
                candidates,
                selection,
                origin_environment,
                organism_body_profile_plan,
                output,
            } => derive_provisional_material_resource_plan(
                PopulationPlanInputPaths {
                    population_plan: &population_plan,
                    candidates: &candidates,
                    selection: &selection,
                    origin_environment: &origin_environment,
                },
                &organism_body_profile_plan,
                &output,
            ),
            DeriveCommand::EtopoGrid {
                source_snapshot,
                artifact_root,
                sample_arc_minutes,
                output,
            } => derive_etopo_grid(
                &source_snapshot,
                &artifact_root,
                sample_arc_minutes,
                &output,
            ),
            DeriveCommand::EtopoCentreIndex {
                source_snapshot,
                artifact_root,
                sample_arc_minutes,
                s2_level,
                output,
            } => derive_etopo_centre_index(
                &source_snapshot,
                &artifact_root,
                sample_arc_minutes,
                s2_level,
                &output,
            ),
            DeriveCommand::CentreSummary {
                input,
                s2_level,
                output,
            } => derive_etopo_centre_summary(&input, s2_level, &output),
            DeriveCommand::EtopoQuadratureContribution {
                source_snapshot,
                artifact_root,
                row,
                column,
                target_s2_level,
                points_per_axis,
            } => derive_etopo_quadrature_contribution(
                &source_snapshot,
                &artifact_root,
                row,
                column,
                target_s2_level,
                points_per_axis,
            ),
            DeriveCommand::EtopoTerrainLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            } => derive_etopo_terrain_layer(
                &source_snapshot,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            ),
            DeriveCommand::NaturalEarthLandReferenceLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
            } => derive_natural_earth_land_reference_layer(
                &source_snapshot,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
            ),
            DeriveCommand::CopernicusLandCoverLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
                source_chunk_cache,
            } => derive_copernicus_land_cover_layer(CopernicusLandCoverDerivationOptions {
                manifest_path: &source_snapshot,
                artifact_root: &artifact_root,
                layer_id: &layer_id,
                output_directory: &output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
                source_chunk_cache,
            }),
            DeriveCommand::SoilgridsTopsoilLayer {
                source_inventory,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
            } => derive_soilgrids_topsoil_layer(
                &source_inventory,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
            ),
            DeriveCommand::GbifAnimaliaCatalog { archive, output } => {
                derive_gbif_animalia_catalog(&archive, &output)
            }
            DeriveCommand::JrcSurfaceWaterOccurrenceLayer {
                source_inventory,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
                source_raster_cache,
            } => derive_jrc_surface_water_occurrence_layer(
                &source_inventory,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
                source_raster_cache,
            ),
            DeriveCommand::ChelsaAnnualTemperatureLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
                source_chunk_cache,
            } => derive_chelsa_annual_temperature_layer(
                &source_snapshot,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
                source_chunk_cache,
            ),
        },
    }
}

#[derive(Debug, Serialize)]
struct GbifBackboneInspection {
    inspection_schema_version: u16,
    release: &'static str,
    archive_path: String,
    archive_byte_length: u64,
    member_count: usize,
    dataset_metadata_member_count: usize,
    uncompressed_byte_length: u64,
    taxon_columns: Vec<String>,
    taxon_first_record: BTreeMap<String, String>,
    members: Vec<GbifBackboneMemberInspection>,
}

#[derive(Debug, Serialize)]
struct GbifBackboneMemberInspection {
    path: String,
    directory: bool,
    compression: String,
    compressed_byte_length: u64,
    uncompressed_byte_length: u64,
    crc32: u32,
}

fn inspect_gbif_backbone(archive_path: &Path) -> Result<()> {
    let metadata = fs::metadata(archive_path)
        .with_context(|| format!("inspect GBIF archive {}", archive_path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!(
            "GBIF archive is not a nonempty regular file: {}",
            archive_path.display()
        );
    }
    let file = File::open(archive_path)
        .with_context(|| format!("open GBIF archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("parse GBIF archive {}", archive_path.display()))?;
    let mut members = Vec::with_capacity(archive.len());
    let mut dataset_metadata_member_count = 0_usize;
    let mut uncompressed_byte_length = 0_u64;
    for index in 0..archive.len() {
        let member = archive
            .by_index(index)
            .with_context(|| format!("inspect GBIF archive member {index}"))?;
        if member.enclosed_name().is_none() {
            bail!(
                "GBIF archive member has an unsafe path: {:?}",
                member.name()
            );
        }
        uncompressed_byte_length = uncompressed_byte_length
            .checked_add(member.size())
            .context("GBIF archive uncompressed byte length overflow")?;
        if member.name().starts_with("dataset/") && member.name().ends_with(".xml") {
            dataset_metadata_member_count += 1;
        } else {
            members.push(GbifBackboneMemberInspection {
                path: member.name().to_owned(),
                directory: member.is_dir(),
                compression: format!("{:?}", member.compression()),
                compressed_byte_length: member.compressed_size(),
                uncompressed_byte_length: member.size(),
                crc32: member.crc32(),
            });
        }
    }
    members.sort_by(|left, right| left.path.cmp(&right.path));
    let (taxon_columns, taxon_first_record) = {
        let mut member = archive
            .by_name("Taxon.tsv")
            .context("GBIF Backbone archive is missing Taxon.tsv")?;
        let mut reader = BufReader::new(&mut member);
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .context("read GBIF Taxon.tsv header")?;
        let columns = header
            .trim_end_matches(['\r', '\n'])
            .split('\t')
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut first_record = String::new();
        reader
            .read_line(&mut first_record)
            .context("read first GBIF Taxon.tsv record")?;
        let values = first_record
            .trim_end_matches(['\r', '\n'])
            .split('\t')
            .collect::<Vec<_>>();
        if values.len() != columns.len() {
            bail!("first GBIF Taxon.tsv record does not match its header");
        }
        let record = columns
            .iter()
            .cloned()
            .zip(values.into_iter().map(ToOwned::to_owned))
            .collect::<BTreeMap<_, _>>();
        (columns, record)
    };
    println!(
        "{}",
        serde_json::to_string(&GbifBackboneInspection {
            inspection_schema_version: 1,
            release: "2023-08-28",
            archive_path: archive_path.display().to_string(),
            archive_byte_length: metadata.len(),
            member_count: archive.len(),
            dataset_metadata_member_count,
            uncompressed_byte_length,
            taxon_columns,
            taxon_first_record,
            members,
        })?
    );
    Ok(())
}

const GBIF_ANIMALIA_CATALOG_MAGIC: &[u8; 8] = b"ATCGBF01";
const GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION: u16 = 1;
const GBIF_ANIMALIA_CATALOG_RECORD_COUNT_OFFSET: u64 = 8 + 2 + 32;

#[derive(Debug, Serialize)]
struct GbifAnimaliaCatalogDerivation {
    derivation_schema_version: u16,
    source_release: &'static str,
    source_archive_hash: Digest,
    source_archive_byte_length: u64,
    source_taxon_records: u64,
    accepted_animalia_species: u64,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
    ordering: &'static str,
}

fn derive_gbif_animalia_catalog(archive_path: &Path, output_path: &Path) -> Result<()> {
    let (source_archive_byte_length, source_archive_hash) = digest_file(archive_path)
        .with_context(|| format!("hash frozen GBIF archive {}", archive_path.display()))?;
    let archive_file = File::open(archive_path)
        .with_context(|| format!("open frozen GBIF archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .with_context(|| format!("parse frozen GBIF archive {}", archive_path.display()))?;
    let taxon_member = archive
        .by_name("Taxon.tsv")
        .context("GBIF Backbone archive is missing Taxon.tsv")?;
    let mut reader = BufReader::new(taxon_member);

    let parent = output_path
        .parent()
        .context("GBIF catalog output has no parent directory")?
        .canonicalize()
        .with_context(|| {
            format!(
                "resolve GBIF catalog output directory {}",
                output_path.display()
            )
        })?;
    let file_name = output_path
        .file_name()
        .and_then(OsStr::to_str)
        .context("GBIF catalog output filename is not UTF-8")?;
    let destination = parent.join(file_name);
    if fs::symlink_metadata(&destination).is_ok() {
        bail!(
            "GBIF catalog output already exists: {}",
            destination.display()
        );
    }
    let mut partial = PartialDownload::create(&parent, file_name)?;
    partial.file.write_all(GBIF_ANIMALIA_CATALOG_MAGIC)?;
    partial
        .file
        .write_all(&GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION.to_le_bytes())?;
    partial.file.write_all(source_archive_hash.as_bytes())?;
    partial.file.write_all(&0_u64.to_le_bytes())?;

    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        bail!("GBIF Taxon.tsv is empty");
    }
    let columns = line
        .trim_end_matches(['\r', '\n'])
        .split('\t')
        .enumerate()
        .map(|(index, name)| (name.to_owned(), index))
        .collect::<HashMap<_, _>>();
    let required = |name: &str| -> Result<usize> {
        columns
            .get(name)
            .copied()
            .with_context(|| format!("GBIF Taxon.tsv is missing column {name}"))
    };
    let taxon_id_index = required("taxonID")?;
    let scientific_name_index = required("scientificName")?;
    let canonical_name_index = required("canonicalName")?;
    let taxon_rank_index = required("taxonRank")?;
    let taxonomic_status_index = required("taxonomicStatus")?;
    let kingdom_index = required("kingdom")?;
    let phylum_index = required("phylum")?;
    let class_index = required("class")?;
    let order_index = required("order")?;
    let family_index = required("family")?;
    let genus_index = required("genus")?;
    let column_count = columns.len();

    let mut source_taxon_records = 0_u64;
    let mut accepted_animalia_species = 0_u64;
    let mut accepted_keys = HashSet::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        source_taxon_records = source_taxon_records
            .checked_add(1)
            .context("GBIF source record count overflow")?;
        let fields = line
            .trim_end_matches(['\r', '\n'])
            .split('\t')
            .collect::<Vec<_>>();
        if fields.len() != column_count {
            bail!(
                "GBIF Taxon.tsv record {} has {} fields, expected {}",
                source_taxon_records,
                fields.len(),
                column_count
            );
        }
        if fields[kingdom_index] != "Animalia"
            || fields[taxon_rank_index] != "species"
            || fields[taxonomic_status_index] != "accepted"
        {
            continue;
        }
        let taxon_key = fields[taxon_id_index].parse::<u64>().with_context(|| {
            format!(
                "accepted Animalia species has invalid GBIF taxonID at record {}",
                source_taxon_records
            )
        })?;
        if taxon_key == 0 || !accepted_keys.insert(taxon_key) {
            bail!("accepted Animalia species has a zero or duplicate GBIF key {taxon_key}");
        }
        if fields[scientific_name_index].is_empty() {
            bail!("accepted Animalia species {taxon_key} has no scientific name");
        }
        partial.file.write_all(&taxon_key.to_le_bytes())?;
        for value in [
            fields[scientific_name_index],
            fields[canonical_name_index],
            fields[phylum_index],
            fields[class_index],
            fields[order_index],
            fields[family_index],
            fields[genus_index],
        ] {
            write_length_prefixed_utf8(&mut partial.file, value)?;
        }
        accepted_animalia_species = accepted_animalia_species
            .checked_add(1)
            .context("GBIF Animalia species count overflow")?;
        if source_taxon_records.is_multiple_of(1_000_000) {
            eprintln!(
                "GBIF Animalia catalog progress: {source_taxon_records} source records, {accepted_animalia_species} accepted species"
            );
        }
    }
    if accepted_animalia_species == 0 {
        bail!("GBIF derivation found no accepted Animalia species");
    }
    partial
        .file
        .seek(SeekFrom::Start(GBIF_ANIMALIA_CATALOG_RECORD_COUNT_OFFSET))?;
    partial
        .file
        .write_all(&accepted_animalia_species.to_le_bytes())?;
    partial.file.sync_all()?;
    let (output_byte_length, output_hash) = digest_file(&partial.path)?;
    partial.persist_without_replacement(&destination)?;

    println!(
        "{}",
        serde_json::to_string(&GbifAnimaliaCatalogDerivation {
            derivation_schema_version: 1,
            source_release: "GBIF Backbone 2023-08-28",
            source_archive_hash,
            source_archive_byte_length,
            source_taxon_records,
            accepted_animalia_species,
            output_path: destination.display().to_string(),
            output_hash,
            output_byte_length,
            ordering: "exact retained Taxon.tsv record order; taxonID uniqueness enforced",
        })?
    );
    Ok(())
}

fn write_length_prefixed_utf8(writer: &mut impl Write, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    writer.write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
    writer.write_all(bytes)?;
    Ok(())
}

fn digest_file(path: &Path) -> Result<(u64, Digest)> {
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("open artifact {}", path.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("read artifact {}", path.display()))?;
        if read == 0 {
            break;
        }
        byte_length = byte_length
            .checked_add(u64::try_from(read)?)
            .context("artifact byte length overflow")?;
        hasher.update(&buffer[..read]);
    }
    Ok((byte_length, Digest::from_bytes(hasher.finalize().into())))
}

#[derive(Debug, Serialize)]
struct GbifAnimaliaCatalogInspection {
    inspection_schema_version: u16,
    source_archive_hash: Digest,
    record_count: u64,
    distinct_taxon_keys: usize,
    input_path: String,
    input_hash: Digest,
    input_byte_length: u64,
    first_species: GbifCatalogSpeciesInspection,
    last_species: GbifCatalogSpeciesInspection,
}

#[derive(Clone, Debug, Serialize)]
struct GbifCatalogSpeciesInspection {
    gbif_taxon_key: u64,
    scientific_name: String,
    canonical_name: String,
    phylum: String,
    class: String,
    order: String,
    family: String,
    genus: String,
}

fn inspect_gbif_animalia_catalog(input_path: &Path) -> Result<()> {
    let (input_byte_length, input_hash) = digest_file(input_path)?;
    let mut reader = BufReader::new(
        File::open(input_path)
            .with_context(|| format!("open GBIF Animalia catalog {}", input_path.display()))?,
    );
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != GBIF_ANIMALIA_CATALOG_MAGIC {
        bail!("GBIF Animalia catalog magic is invalid");
    }
    let mut schema_bytes = [0_u8; 2];
    reader.read_exact(&mut schema_bytes)?;
    let schema_version = u16::from_le_bytes(schema_bytes);
    if schema_version != GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION {
        bail!("GBIF Animalia catalog schema {schema_version} is unsupported");
    }
    let mut source_hash_bytes = [0_u8; 32];
    reader.read_exact(&mut source_hash_bytes)?;
    let source_archive_hash = Digest::from_bytes(source_hash_bytes);
    let mut record_count_bytes = [0_u8; 8];
    reader.read_exact(&mut record_count_bytes)?;
    let record_count = u64::from_le_bytes(record_count_bytes);
    if record_count == 0 {
        bail!("GBIF Animalia catalog has no species");
    }

    let mut keys = HashSet::with_capacity(usize::try_from(record_count)?);
    let mut first_species = None;
    let mut last_species = None;
    for index in 0..record_count {
        let mut key_bytes = [0_u8; 8];
        reader
            .read_exact(&mut key_bytes)
            .with_context(|| format!("read GBIF catalog species {index} key"))?;
        let gbif_taxon_key = u64::from_le_bytes(key_bytes);
        if gbif_taxon_key == 0 || !keys.insert(gbif_taxon_key) {
            bail!("GBIF Animalia catalog has zero or duplicate key {gbif_taxon_key}");
        }
        let species = GbifCatalogSpeciesInspection {
            gbif_taxon_key,
            scientific_name: read_length_prefixed_utf8(&mut reader)?,
            canonical_name: read_length_prefixed_utf8(&mut reader)?,
            phylum: read_length_prefixed_utf8(&mut reader)?,
            class: read_length_prefixed_utf8(&mut reader)?,
            order: read_length_prefixed_utf8(&mut reader)?,
            family: read_length_prefixed_utf8(&mut reader)?,
            genus: read_length_prefixed_utf8(&mut reader)?,
        };
        if species.scientific_name.is_empty() {
            bail!("GBIF Animalia catalog species {gbif_taxon_key} has no scientific name");
        }
        if first_species.is_none() {
            first_species = Some(species.clone());
        }
        last_species = Some(species);
    }
    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing)? != 0 {
        bail!("GBIF Animalia catalog contains trailing bytes");
    }
    println!(
        "{}",
        serde_json::to_string(&GbifAnimaliaCatalogInspection {
            inspection_schema_version: 1,
            source_archive_hash,
            record_count,
            distinct_taxon_keys: keys.len(),
            input_path: input_path.display().to_string(),
            input_hash,
            input_byte_length,
            first_species: first_species.context("GBIF Animalia catalog first species missing")?,
            last_species: last_species.context("GBIF Animalia catalog last species missing")?,
        })?
    );
    Ok(())
}

fn read_length_prefixed_utf8(reader: &mut impl Read) -> Result<String> {
    let mut length_bytes = [0_u8; 4];
    reader.read_exact(&mut length_bytes)?;
    let length = usize::try_from(u32::from_le_bytes(length_bytes))?;
    if length > 1024 * 1024 {
        bail!("GBIF catalog string exceeds the one-megabyte safety bound");
    }
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).context("GBIF catalog string is not UTF-8")
}

#[derive(Debug, Serialize)]
struct FaunaTraitTaxaInspection {
    inspection_schema_version: u16,
    catalog: FaunaTaxonomyCatalogInspection,
    sources: Vec<FaunaTraitSourceTaxaInspection>,
    policy: &'static str,
}

fn inspect_fauna_range_candidate_set(input: &Path) -> Result<()> {
    let bytes = fs::read(input)
        .with_context(|| format!("read fauna range candidate set {}", input.display()))?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&bytes)
        .with_context(|| format!("validate fauna range candidate set {}", input.display()))?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "candidate_set_id": candidates.candidate_set_id,
            "candidate_count": candidates.candidates.len(),
            "content_hash": Digest::sha256(&bytes),
            "inaturalist_release": candidates.inaturalist_release,
            "latitude_e7": candidates.query_point.latitude_e7,
            "longitude_e7": candidates.query_point.longitude_e7,
            "status": "modeled-range-candidates-not-population-or-abundance",
        }))?
    );
    Ok(())
}

fn inspect_fauna_physiology_profile_set(input: &Path) -> Result<()> {
    let bytes = fs::read(input)
        .with_context(|| format!("read fauna physiology profile set {}", input.display()))?;
    let profiles = FaunaPhysiologyProfileSet::from_canonical_slice(&bytes)
        .context("validate fauna physiology profile set")?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "profile_count": profiles.profiles.len(),
            "source_artifact_digest": profiles.source_artifact_digest,
        }))?
    );
    Ok(())
}

fn inspect_fauna_physiology_profile_catalog(input: &Path) -> Result<()> {
    let bytes = fs::read(input)
        .with_context(|| format!("read fauna physiology profile catalog {}", input.display()))?;
    let catalog = FaunaPhysiologyProfileCatalog::from_canonical_slice(&bytes)
        .context("validate fauna physiology profile catalog")?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "profile_set_count": catalog.profile_sets.len(),
            "profile_count": catalog.profile_sets.iter().map(|entry| entry.profile_count).sum::<u64>(),
        }))?
    );
    Ok(())
}

fn inspect_provisional_world_composition(input: &Path) -> Result<()> {
    let composition = load_provisional_world_composition(input)
        .context("validate provisional world composition")?;
    let bytes = fs::read(input)
        .with_context(|| format!("read provisional world composition {}", input.display()))?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "composition_id": composition.composition_id,
            "composition_version": composition.composition_version,
            "earth_layer_count": composition.earth_layers.len(),
            "world_component_count": composition.world_components.len(),
            "status": composition.status,
        }))?
    );
    Ok(())
}

fn inspect_s2_geographic(s2_cell_id: S2CellId) -> Result<()> {
    let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
        decode_s2_face_ij(s2_cell_id),
    )?)?)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "latitude_e7": coordinate.latitude_e7(),
            "longitude_e7": coordinate.longitude_e7(),
            "s2_cell_id": s2_cell_id,
        }))?
    );
    Ok(())
}

fn inspect_fauna_terrestrial_evidence(
    candidates_path: &Path,
    elton_birds_path: &Path,
) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path)
        .with_context(|| format!("read fauna candidates {}", candidates_path.display()))?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes)
        .context("validate fauna candidates")?;
    let raw = fs::read(elton_birds_path)
        .with_context(|| format!("read Elton bird traits {}", elton_birds_path.display()))?;
    let rows = parse_delimited_records(&decode_windows_1252(&raw), '\t')?;
    let header = rows
        .first()
        .cloned()
        .context("Elton bird traits has no header")?;
    let column = |name: &str| {
        header
            .iter()
            .position(|field| field == name)
            .with_context(|| format!("Elton bird traits is missing {name}"))
    };
    let scientific = column("Scientific")?;
    let pelagic = column("PelagicSpecialist")?;
    let water_below = column("ForStrat-watbelowsurf")?;
    let water_around = column("ForStrat-wataroundsurf")?;
    let mut records = BTreeMap::<String, Vec<(u16, u16, u16)>>::new();
    for (row_number, row) in rows.into_iter().enumerate().skip(1) {
        if row.iter().all(String::is_empty) {
            continue;
        }
        if row.len() != header.len() {
            bail!("Elton bird row {} has a wrong column count", row_number + 1);
        }
        let name = row[scientific].trim();
        if name.is_empty() {
            continue;
        }
        let numeric = |index: usize, field: &str| -> Result<u16> {
            row[index]
                .trim()
                .parse()
                .with_context(|| format!("Elton bird row {} has invalid {field}", row_number + 1))
        };
        records.entry(name.to_owned()).or_default().push((
            numeric(pelagic, "PelagicSpecialist")?,
            numeric(water_below, "ForStrat-watbelowsurf")?,
            numeric(water_around, "ForStrat-wataroundsurf")?,
        ));
    }
    let mut exact_single_record_birds = 0_u64;
    let mut terrestrial_foraging_birds = 0_u64;
    let mut ambiguous_bird_records = 0_u64;
    for candidate in &candidates.candidates {
        match records
            .get(&candidate.species.scientific_name)
            .map(Vec::as_slice)
        {
            Some([record]) => {
                exact_single_record_birds += 1;
                if is_elton_terrestrial_foraging(*record) {
                    terrestrial_foraging_birds += 1;
                }
            }
            Some(_) => ambiguous_bird_records += 1,
            None => {}
        }
    }
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "candidate_set_digest": Digest::sha256(&candidate_bytes),
            "candidate_count": candidates.candidates.len(),
            "elton_bird_source_digest": Digest::sha256(&raw),
            "exact_single_record_birds": exact_single_record_birds,
            "terrestrial_foraging_birds": terrestrial_foraging_birds,
            "ambiguous_bird_records": ambiguous_bird_records,
            "status": "coverage-only-not-habitat-suitability-or-population",
            "terrestrial_foraging_rule": "PelagicSpecialist=0 AND ForStrat-watbelowsurf=0 AND ForStrat-wataroundsurf=0",
        }))?
    );
    Ok(())
}

/// A deliberately narrow trait-only condition. This is not a habitat model.
fn is_elton_terrestrial_foraging(record: (u16, u16, u16)) -> bool {
    let (pelagic_specialist, water_below_surface, water_around_surface) = record;
    pelagic_specialist == 0 && water_below_surface == 0 && water_around_surface == 0
}

fn derive_fauna_seeded_selection(
    candidates_path: &Path,
    world_seed: WorldSeed,
    species_limit: u32,
    individual_fauna_only: bool,
    output_path: &Path,
) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path).with_context(|| {
        format!(
            "read fauna range candidate set {}",
            candidates_path.display()
        )
    })?;
    let candidates =
        FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes).with_context(|| {
            format!(
                "validate fauna range candidate set {}",
                candidates_path.display()
            )
        })?;
    let selection = if individual_fauna_only {
        candidates.select_seeded_individual_candidates(world_seed, species_limit)
    } else {
        candidates.select_seeded_candidates(world_seed, species_limit)
    }
    .context("derive deterministic fauna seeded selection")?;
    let selection_bytes = selection
        .canonical_bytes_against(&candidates)
        .context("encode canonical fauna seeded selection")?;
    write_new_artifact(output_path, &selection_bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "candidate_set_id": candidates.candidate_set_id,
            "candidate_set_digest": selection.candidate_set_digest,
            "content_hash": Digest::sha256(&selection_bytes),
            "selected_candidate_count": selection.selected_candidates.len(),
            "species_limit": selection.species_limit,
            "identity_tier_policy": selection.identity_tier_policy,
            "status": "seeded-range-selection-not-population-or-organism-creation",
            "world_seed": selection.world_seed,
        }))?
    );
    Ok(())
}

fn load_population_plan_inputs(
    candidates_path: &Path,
    selection_path: &Path,
    origin_environment_path: &Path,
    population_plan_path: &Path,
) -> Result<(
    FaunaRangeCandidateSet,
    FaunaSeededSelection,
    ProvisionalOriginEnvironment,
    FaunaPopulationPlan,
)> {
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(
        &fs::read(candidates_path)
            .with_context(|| format!("read fauna candidates {}", candidates_path.display()))?,
    )
    .context("validate fauna candidates")?;
    let selection = FaunaSeededSelection::from_canonical_slice_against(
        &fs::read(selection_path)
            .with_context(|| format!("read fauna seeded selection {}", selection_path.display()))?,
        &candidates,
    )
    .context("validate fauna seeded selection")?;
    let environment = ProvisionalOriginEnvironment::from_canonical_slice(
        &fs::read(origin_environment_path).with_context(|| {
            format!(
                "read origin environment {}",
                origin_environment_path.display()
            )
        })?,
    )
    .context("validate provisional origin environment")?;
    let plan = FaunaPopulationPlan::from_canonical_slice_against_environment(
        &fs::read(population_plan_path).with_context(|| {
            format!(
                "read fauna population plan {}",
                population_plan_path.display()
            )
        })?,
        &candidates,
        &selection,
        &environment,
    )
    .context("validate fauna population plan")?;
    Ok((candidates, selection, environment, plan))
}

fn load_metabolic_profiles(path: &Path) -> Result<(FaunaPhysiologyProfileSet, Digest)> {
    let bytes = fs::read(path)
        .with_context(|| format!("read fauna metabolic profile set {}", path.display()))?;
    let profiles = FaunaPhysiologyProfileSet::from_canonical_slice(&bytes)
        .context("validate fauna metabolic profile set")?;
    Ok((profiles, Digest::sha256(&bytes)))
}

fn canonical_metabolic_profile<'a>(
    profiles: &'a FaunaPhysiologyProfileSet,
    species: &SpeciesIdentity,
) -> Option<&'a world_data::FaunaPhysiologyProfile> {
    // The set validator fixes `(species, trait_id, source_record_id)` order. The first
    // matching positive watt observation is therefore a stable, source-addressable rule.
    profiles.profiles.iter().find(|profile| {
        profile.species.catalog == species.catalog
            && profile.species.identifier == species.identifier
            && profile.trait_id == "standardized-metabolic-rate"
            && profile.value.unit == "W"
            && profile.value.value > 0
    })
}

fn provisional_founder_entries(selection: &FaunaSeededSelection) -> Vec<FaunaPopulationPlanEntry> {
    let female = BirthCategory::new("female").expect("static valid birth category");
    let male = BirthCategory::new("male").expect("static valid birth category");
    let mut entries = selection
        .selected_candidates
        .iter()
        .map(|candidate| FaunaPopulationPlanEntry {
            species: candidate.species.clone(),
            initial_individual_count: 2,
            birth_category_counts: vec![
                FaunaBirthCategoryCount {
                    category: female.clone(),
                    count: 1,
                },
                FaunaBirthCategoryCount {
                    category: male.clone(),
                    count: 1,
                },
            ],
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        entry
            .species
            .identifier
            .parse::<u64>()
            .expect("selected GBIF candidate identifier validated as a positive integer")
    });
    entries
}

fn derive_provisional_fauna_population_plan(
    candidates_path: &Path,
    selection_path: &Path,
    origin_environment_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path)
        .with_context(|| format!("read fauna candidates {}", candidates_path.display()))?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes)
        .context("validate fauna candidates")?;
    let selection_bytes = fs::read(selection_path)
        .with_context(|| format!("read fauna seeded selection {}", selection_path.display()))?;
    let selection =
        FaunaSeededSelection::from_canonical_slice_against(&selection_bytes, &candidates)
            .context("validate fauna seeded selection")?;
    let environment_bytes = fs::read(origin_environment_path).with_context(|| {
        format!(
            "read origin environment {}",
            origin_environment_path.display()
        )
    })?;
    let environment = ProvisionalOriginEnvironment::from_canonical_slice(&environment_bytes)
        .context("validate provisional origin environment")?;
    let entries = provisional_founder_entries(&selection);
    let plan = FaunaPopulationPlan {
        population_plan_schema_version: FAUNA_POPULATION_PLAN_SCHEMA_VERSION,
        status: "provisional-not-scientifically-admitted".to_owned(),
        world_seed: selection.world_seed,
        origin_environment_digest: Digest::sha256(&environment_bytes),
        embodied_patch: environment.selected_embodied_patch,
        candidate_set_digest: Digest::sha256(&candidate_bytes),
        seeded_selection_digest: Digest::sha256(&selection_bytes),
        entries,
    };
    let bytes = plan
        .canonical_bytes_against(&candidates, &selection)
        .context("encode provisional fauna population plan")?;
    plan.validate_against_environment(&candidates, &selection, &environment)
        .context("validate provisional fauna population plan against origin")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "species_count": plan.entries.len(),
            "initial_individual_count": plan.entries.len() * 2,
            "status": plan.status,
            "policy": "every species in the bounded seed-derived selection receives one female and one male provisional founder; range candidates are not abundance measurements",
        }))?
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct InaturalistOccurrenceSourceManifest {
    manifest_schema_version: u16,
    endpoint: String,
    query: BTreeMap<String, String>,
    semantics: InaturalistOccurrenceSemantics,
    total_results: u64,
    pages: Vec<InaturalistOccurrencePageReference>,
}

#[derive(Debug, Deserialize)]
struct InaturalistOccurrenceSemantics {
    candidate_use: String,
    commercial_observation_licenses: Vec<String>,
    wild_filter: String,
}

#[derive(Debug, Deserialize)]
struct InaturalistOccurrencePageReference {
    byte_length: u64,
    content_hash: Digest,
    page: u32,
    path: String,
    result_count: u32,
}

#[derive(Debug, Deserialize)]
struct InaturalistOccurrencePage {
    total_results: u64,
    results: Vec<InaturalistRawObservation>,
}

#[derive(Debug, Deserialize)]
struct InaturalistRawObservation {
    id: u64,
    license_code: Option<String>,
    quality_grade: String,
    captive: bool,
    observed_on: Option<String>,
    positional_accuracy: Option<u64>,
    taxon: InaturalistRawTaxon,
}

#[derive(Debug, Deserialize)]
struct InaturalistRawTaxon {
    id: u64,
    name: String,
}

fn parse_source_e7(value: &str, field: &str) -> Result<i32> {
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |unsigned| (true, unsigned));
    let (whole, fractional) = unsigned
        .split_once('.')
        .with_context(|| format!("{field} must use an exact seven-place decimal"))?;
    if whole.is_empty()
        || fractional.len() != 7
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("{field} must use an exact seven-place decimal");
    }
    let magnitude = whole
        .parse::<i64>()?
        .checked_mul(10_000_000)
        .and_then(|scaled| fractional.parse::<i64>().ok()?.checked_add(scaled))
        .context("source coordinate overflow")?;
    let signed = if negative { -magnitude } else { magnitude };
    i32::try_from(signed).context("source coordinate exceeds E7 range")
}

fn source_child(directory: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        bail!("occurrence source page path is not one portable filename");
    }
    let joined = directory.join(path);
    let metadata = fs::symlink_metadata(&joined)
        .with_context(|| format!("inspect occurrence source page {}", joined.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("occurrence source page is not a regular file");
    }
    Ok(joined)
}

fn derive_local_fauna_occurrence_evidence(
    source_directory: &Path,
    output_path: &Path,
) -> Result<()> {
    let manifest_path = source_directory.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).with_context(|| {
        format!(
            "read occurrence source manifest {}",
            manifest_path.display()
        )
    })?;
    let manifest: InaturalistOccurrenceSourceManifest =
        serde_json::from_slice(&manifest_bytes).context("decode occurrence source manifest")?;
    if manifest.manifest_schema_version != 1
        || manifest.endpoint != "https://api.inaturalist.org/v1/observations"
        || manifest.semantics.candidate_use
            != "corroborated-local-presence-not-abundance-or-native-status"
        || manifest.semantics.commercial_observation_licenses != ["cc0", "cc-by"]
        || manifest.semantics.wild_filter != "captive=false"
        || manifest.pages.is_empty()
        || manifest.total_results == 0
        || manifest.total_results > 10_000
    {
        bail!("occurrence source manifest has unsupported semantics");
    }
    let expected_query = [
        ("captive", "false"),
        ("license", "cc0,cc-by"),
        ("order", "asc"),
        ("order_by", "id"),
        ("per_page", "200"),
        ("quality_grade", "research"),
        ("taxon_id", "1"),
    ];
    for (key, value) in expected_query {
        if manifest.query.get(key).map(String::as_str) != Some(value) {
            bail!("occurrence source query changed required filter {key}");
        }
    }
    if manifest.query.len() != expected_query.len() + 3 {
        bail!("occurrence source query contains an unknown parameter");
    }
    let latitude_e7 = parse_source_e7(
        manifest
            .query
            .get("lat")
            .context("source query lacks lat")?,
        "lat",
    )?;
    let longitude_e7 = parse_source_e7(
        manifest
            .query
            .get("lng")
            .context("source query lacks lng")?,
        "lng",
    )?;
    GeographicCoordinateE7::new(latitude_e7, longitude_e7)
        .context("source query coordinate is outside WGS84")?;
    let radius_kilometers = manifest
        .query
        .get("radius")
        .context("source query lacks radius")?
        .parse::<u16>()?;
    if !(1..=100).contains(&radius_kilometers) {
        bail!("source query radius is outside the admitted bound");
    }

    let mut records = Vec::new();
    for (index, reference) in manifest.pages.iter().enumerate() {
        if reference.page != u32::try_from(index + 1)?
            || reference.path != format!("page-{:05}.json", index + 1)
        {
            bail!("occurrence source pages are not contiguous and canonical");
        }
        let page_path = source_child(source_directory, &reference.path)?;
        let page_bytes = fs::read(&page_path)
            .with_context(|| format!("read occurrence source page {}", page_path.display()))?;
        if u64::try_from(page_bytes.len())? != reference.byte_length
            || Digest::sha256(&page_bytes) != reference.content_hash
        {
            bail!("occurrence source page bytes do not match their manifest");
        }
        let page: InaturalistOccurrencePage =
            serde_json::from_slice(&page_bytes).context("decode occurrence source page")?;
        if page.total_results != manifest.total_results
            || page.results.len() != usize::try_from(reference.result_count)?
            || page.results.len() > 200
        {
            bail!("occurrence source page envelope changed during acquisition");
        }
        for raw in page.results {
            if raw.id == 0
                || raw.taxon.id == 0
                || raw.quality_grade != "research"
                || raw.captive
                || !matches!(raw.license_code.as_deref(), Some("cc0" | "cc-by"))
            {
                bail!("occurrence source contains a record outside the admitted filters");
            }
            let positional_accuracy_meters = raw
                .positional_accuracy
                .map(u32::try_from)
                .transpose()
                .context("occurrence positional accuracy exceeds portable range")?;
            records.push(LocalFaunaOccurrenceRecord {
                observation_id: raw.id,
                inaturalist_taxon_id: raw.taxon.id,
                scientific_name: raw.taxon.name,
                observed_on: raw
                    .observed_on
                    .context("research-grade occurrence lacks observation date")?,
                observation_license: raw.license_code.expect("validated license presence"),
                source_url: format!("https://www.inaturalist.org/observations/{}", raw.id),
                positional_accuracy_meters,
            });
        }
    }
    if u64::try_from(records.len())? != manifest.total_results {
        bail!("occurrence source pages do not cover the declared result count");
    }
    records.sort_by_key(|record| record.observation_id);
    let evidence = LocalFaunaOccurrenceEvidenceSet {
        evidence_schema_version: LOCAL_FAUNA_OCCURRENCE_EVIDENCE_SCHEMA_VERSION,
        source_manifest_digest: Digest::sha256(&manifest_bytes),
        query_latitude_e7: latitude_e7,
        query_longitude_e7: longitude_e7,
        radius_kilometers,
        records,
    };
    let bytes = evidence
        .canonical_bytes()
        .context("encode local fauna occurrence evidence")?;
    write_new_artifact(output_path, &bytes)?;
    let corroborated_taxon_count = evidence
        .records
        .iter()
        .map(|record| record.inaturalist_taxon_id)
        .collect::<BTreeSet<_>>()
        .len();
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "observation_count": evidence.records.len(),
            "corroborated_taxon_count": corroborated_taxon_count,
            "radius_kilometers": evidence.radius_kilometers,
            "status": "corroborated-local-presence-not-abundance-or-native-status",
        }))?
    );
    Ok(())
}

fn derive_corroborated_fauna_candidates(
    candidates_path: &Path,
    occurrence_evidence_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path)
        .with_context(|| format!("read fauna candidates {}", candidates_path.display()))?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes)
        .context("validate modeled-range fauna candidates")?;
    let evidence_bytes = fs::read(occurrence_evidence_path).with_context(|| {
        format!(
            "read local fauna occurrence evidence {}",
            occurrence_evidence_path.display()
        )
    })?;
    let evidence = LocalFaunaOccurrenceEvidenceSet::from_canonical_slice(&evidence_bytes)
        .context("validate local fauna occurrence evidence")?;
    let corroborated = candidates
        .corroborated_by_local_occurrences(&evidence)
        .context("intersect modeled range and local occurrence evidence")?;
    let bytes = corroborated
        .canonical_bytes()
        .context("encode corroborated fauna candidates")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "candidate_count": corroborated.candidates.len(),
            "modeled_range_candidate_count": candidates.candidates.len(),
            "local_occurrence_evidence_digest": Digest::sha256(&evidence_bytes),
            "status": "modeled-range-and-local-occurrence-corroborated-not-abundance-or-native-status",
        }))?
    );
    Ok(())
}

fn derive_fauna_metabolic_rate_plan(
    population_plan_path: &Path,
    candidates_path: &Path,
    selection_path: &Path,
    origin_environment_path: &Path,
    metabolic_profiles_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let (_, _, _, population) = load_population_plan_inputs(
        candidates_path,
        selection_path,
        origin_environment_path,
        population_plan_path,
    )?;
    let (profiles, profile_set_digest) = load_metabolic_profiles(metabolic_profiles_path)?;
    let mut selections = population
        .entries
        .iter()
        .filter_map(|entry| {
            let profile = canonical_metabolic_profile(&profiles, &entry.species)?;
            Some(FaunaMetabolicRateSelection {
                selection_schema_version: 1,
                profile_set_digest,
                species: entry.species.clone(),
                source_record_id: profile.source_record_id.clone(),
            })
        })
        .collect::<Vec<_>>();
    selections.sort_by(|left, right| {
        (&left.species.catalog, &left.species.identifier)
            .cmp(&(&right.species.catalog, &right.species.identifier))
    });
    let plan = FaunaMetabolicRatePlan {
        plan_schema_version: world_data::FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION,
        selections,
    };
    let bytes = plan
        .canonical_bytes()
        .context("encode fauna metabolic-rate plan")?;
    for selection in &plan.selections {
        selection
            .resolve(&profiles)
            .context("resolve selected metabolic observation")?;
    }
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "planned_species_count": population.entries.len(),
            "source_measured_species_count": plan.selections.len(),
            "uncovered_species_count": population.entries.len() - plan.selections.len(),
            "policy": "first canonical exact positive standardized-metabolic-rate observation for each covered planned species; absence remains explicit and is never estimated as source evidence",
        }))?
    );
    Ok(())
}

const SECONDS_PER_DAY: u64 = 86_400;
const DAYS_PER_YEAR: u64 = 365;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProvisionalLifeHistory {
    initial_age_seconds: u64,
    maturity_age_seconds: u64,
    development_seconds: u64,
    recovery_seconds: u64,
    opportunity_interval_seconds: u64,
    initiation_probability_millionths: u32,
}

fn days(value: u64) -> u64 {
    value.checked_mul(SECONDS_PER_DAY).expect("bounded days")
}

fn years(value: u64) -> u64 {
    days(value.checked_mul(DAYS_PER_YEAR).expect("bounded years"))
}

fn duration_ticks(seconds: u64, tick_duration_seconds: u32) -> u64 {
    seconds.div_ceil(u64::from(tick_duration_seconds))
}

fn provisional_life_history(range_package: &str) -> ProvisionalLifeHistory {
    // These are deliberately conservative, coarse engineering guardrails, not
    // species-level scientific claims. They keep a five-minute world from
    // producing minute-scale maturity or development while the cited life-history
    // evidence pipeline is still provisional.
    match range_package {
        "homo_sapiens" => ProvisionalLifeHistory {
            initial_age_seconds: years(20),
            maturity_age_seconds: years(15),
            development_seconds: days(280),
            recovery_seconds: years(1),
            opportunity_interval_seconds: days(28),
            initiation_probability_millionths: 200_000,
        },
        package if package.starts_with("insecta_") => ProvisionalLifeHistory {
            initial_age_seconds: days(180),
            maturity_age_seconds: days(30),
            development_seconds: days(14),
            recovery_seconds: days(14),
            opportunity_interval_seconds: days(7),
            initiation_probability_millionths: 250_000,
        },
        "arachnida" => ProvisionalLifeHistory {
            initial_age_seconds: years(1),
            maturity_age_seconds: days(180),
            development_seconds: days(30),
            recovery_seconds: days(30),
            opportunity_interval_seconds: days(14),
            initiation_probability_millionths: 250_000,
        },
        "aves_1" => ProvisionalLifeHistory {
            initial_age_seconds: years(2),
            maturity_age_seconds: years(1),
            development_seconds: days(30),
            recovery_seconds: days(90),
            opportunity_interval_seconds: days(30),
            initiation_probability_millionths: 200_000,
        },
        "mammalia" => ProvisionalLifeHistory {
            initial_age_seconds: years(4),
            maturity_age_seconds: years(2),
            development_seconds: days(120),
            recovery_seconds: days(180),
            opportunity_interval_seconds: days(30),
            initiation_probability_millionths: 200_000,
        },
        "reptilia" => ProvisionalLifeHistory {
            initial_age_seconds: years(5),
            maturity_age_seconds: years(3),
            development_seconds: days(60),
            recovery_seconds: days(120),
            opportunity_interval_seconds: days(30),
            initiation_probability_millionths: 200_000,
        },
        "amphibia" | "actinopterygii" | "mollusca" | "otheranimalia" => ProvisionalLifeHistory {
            initial_age_seconds: years(1),
            maturity_age_seconds: days(180),
            development_seconds: days(14),
            recovery_seconds: days(30),
            opportunity_interval_seconds: days(14),
            initiation_probability_millionths: 200_000,
        },
        _ => ProvisionalLifeHistory {
            initial_age_seconds: years(1),
            maturity_age_seconds: days(180),
            development_seconds: days(30),
            recovery_seconds: days(60),
            opportunity_interval_seconds: days(30),
            initiation_probability_millionths: 100_000,
        },
    }
}

fn engineering_body_profile_entry(
    species: SpeciesIdentity,
    metabolic_rate: MetabolicRateCommitment,
    tick_duration_seconds: u32,
    range_package: &str,
    life_history_profiles: Option<&(FaunaPhysiologyProfileSet, Digest)>,
    body_mass_profiles: Option<&(FaunaPhysiologyProfileSet, Digest)>,
) -> Result<ProvisionalOrganismBodyProfileEntry> {
    let life_history = provisional_life_history(range_package);
    let profile_digest = Digest::sha256(
        format!(
            "a-tiny-civilization/provisional-body-assumptions/v2/{}/{}/{}",
            range_package, species.catalog, species.identifier
        )
        .as_bytes(),
    );
    let female = BirthCategory::new("female").expect("static valid birth category");
    let male = BirthCategory::new("male").expect("static valid birth category");
    let category_maturity = [&female, &male]
        .into_iter()
        .map(|category| {
            let trait_id = format!("{}-maturity", category.as_str());
            let source = life_history_profiles.and_then(|(profiles, digest)| {
                profiles
                    .profiles
                    .iter()
                    .find(|profile| {
                        profile.species.catalog == species.catalog
                            && profile.species.identifier == species.identifier
                            && profile.trait_id == trait_id
                            && profile.value.unit == "d"
                            && profile.value.decimal_places == 0
                            && profile.value.value > 0
                    })
                    .map(|profile| (profile, *digest))
            });
            match source {
                Some((profile, profile_set_digest)) => ReproductiveCategoryMaturityCommitment {
                    category: category.clone(),
                    maturity_age_ticks: duration_ticks(
                        days(u64::try_from(profile.value.value).expect("positive maturity days")),
                        tick_duration_seconds,
                    ),
                    evidence_basis: PhysiologicalEvidenceBasis::LiteratureApproximation,
                    source_profile_set_digest: profile_set_digest,
                    source_record_id: profile.source_record_id.clone(),
                    source_record_digest: profile.source_record_digest,
                },
                None => ReproductiveCategoryMaturityCommitment {
                    category: category.clone(),
                    maturity_age_ticks: duration_ticks(
                        life_history.maturity_age_seconds,
                        tick_duration_seconds,
                    ),
                    evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
                    source_profile_set_digest: profile_digest,
                    source_record_id: format!(
                        "engineering-assumption-{}-maturity-v1",
                        category.as_str()
                    ),
                    source_record_digest: profile_digest,
                },
            }
        })
        .collect::<Vec<_>>();
    let reproductive_profile_digest = Digest::sha256(
        serde_json::to_vec(&category_maturity)
            .expect("category maturity commitments serialize")
            .as_slice(),
    );
    let matching_mass_profiles = body_mass_profiles
        .map(|(profiles, _)| {
            profiles
                .profiles
                .iter()
                .filter(|profile| {
                    profile.species.catalog == species.catalog
                        && profile.species.identifier == species.identifier
                        && profile.trait_id == "adult-body-mass"
                        && profile.value.unit == "g"
                        && profile.value.value > 0
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if matching_mass_profiles.len() > 1 {
        bail!(
            "body-mass profile set has multiple exact adult-body-mass aggregates for {}:{}",
            species.catalog,
            species.identifier
        );
    }
    let adult_body_mass = matching_mass_profiles.first().map_or_else(
        || {
            let assumption_digest = Digest::sha256(
                format!(
                    "a-tiny-civilization/adult-body-mass-assumption/v1/{}/{}/{}",
                    range_package, species.catalog, species.identifier
                )
                .as_bytes(),
            );
            AdultBodyMassCommitment {
                commitment_schema_version: ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION,
                species: species.clone(),
                evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
                profile_set_digest: assumption_digest,
                source_record_id: "engineering-assumption-adult-body-mass-v1".to_owned(),
                source_record_digest: assumption_digest,
                mass_grams_value: provisional_adult_body_mass_grams(range_package),
                mass_grams_decimal_places: 0,
            }
        },
        |profile| {
            let (_, profile_set_digest) =
                body_mass_profiles.expect("a matching profile requires its profile set");
            AdultBodyMassCommitment {
                commitment_schema_version: ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION,
                species: species.clone(),
                evidence_basis: PhysiologicalEvidenceBasis::LiteratureApproximation,
                profile_set_digest: *profile_set_digest,
                source_record_id: profile.source_record_id.clone(),
                source_record_digest: profile.source_record_digest,
                mass_grams_value: profile.value.value,
                mass_grams_decimal_places: profile.value.decimal_places,
            }
        },
    );
    adult_body_mass
        .validate()
        .context("validate provisional adult-body-mass commitment")?;
    Ok(ProvisionalOrganismBodyProfileEntry {
        species: species.clone(),
        initial_age_ticks: duration_ticks(life_history.initial_age_seconds, tick_duration_seconds),
        metabolic_rate,
        physiological_regulation: PhysiologicalRegulationCommitment {
            commitment_schema_version: PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
            profile_id: "provisional-engineering-regulation-v1".to_owned(),
            profile_digest: reproductive_profile_digest,
            species: species.clone(),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            usable_energy_reserve_joules: 10_000_000,
            hydration_failure_seconds: 604_800,
            fatigue_failure_seconds: 57_600,
            fatigue_recovery_seconds: 28_800,
            thermoneutral_min_millicelsius: 0,
            thermoneutral_max_millicelsius: 40_000,
            thermal_failure_millicelsius_seconds: 86_400_000,
            thermal_recovery_seconds: 28_800,
        },
        reproductive_physiology: ReproductivePhysiologyCommitment {
            commitment_schema_version: REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
            profile_id: "provisional-engineering-reproduction-v2".to_owned(),
            profile_digest,
            species: species.clone(),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            tick_duration_seconds,
            maturity_age_ticks: duration_ticks(
                life_history.maturity_age_seconds,
                tick_duration_seconds,
            ),
            category_maturity,
            development_ticks: duration_ticks(
                life_history.development_seconds,
                tick_duration_seconds,
            ),
            recovery_ticks: duration_ticks(life_history.recovery_seconds, tick_duration_seconds),
            opportunity_interval_ticks: duration_ticks(
                life_history.opportunity_interval_seconds,
                tick_duration_seconds,
            ),
            initiation_probability_millionths: life_history.initiation_probability_millionths,
            compatible_pairs: vec![ReproductiveCategoryPair {
                first: female.clone(),
                second: male.clone(),
                developing_parent: female.clone(),
            }],
            offspring_categories: vec![
                OffspringCategoryWeight {
                    category: female,
                    weight: 1,
                },
                OffspringCategoryWeight {
                    category: male,
                    weight: 1,
                },
            ],
        },
        adult_body_mass: Some(adult_body_mass),
        heritable_disposition_profile: Some(HeritableDispositionProfile {
            profile_schema_version: HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
            profile_id: "provisional-engineering-heredity-v1".to_owned(),
            profile_digest,
            species,
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            minimum_action_weight: 4,
            neutral_action_weight: 16,
            maximum_action_weight: 28,
            founder_variation_steps: 3,
            mutation_probability_millionths: 100_000,
            mutation_max_step: 2,
        }),
    })
}

fn provisional_adult_body_mass_grams(range_package: &str) -> i64 {
    match range_package {
        "homo_sapiens" => 70_000,
        "mammalia" => 10_000,
        "aves_1" => 500,
        "reptilia" => 1_000,
        "amphibia" => 100,
        _ => 1_000,
    }
}

fn engineering_metabolic_commitment(species: SpeciesIdentity) -> MetabolicRateCommitment {
    let assumption_digest = Digest::sha256(
        format!(
            "a-tiny-civilization/provisional-metabolic-assumption/v1/{}/{}",
            species.catalog, species.identifier
        )
        .as_bytes(),
    );
    MetabolicRateCommitment {
        commitment_schema_version: METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
        evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
        profile_set_digest: assumption_digest,
        observed_species: species,
        source_record_id: "engineering-assumption-metabolic-v1".to_owned(),
        source_record_digest: assumption_digest,
        measured_power_value: 100,
        measured_power_decimal_places: 0,
    }
}

struct PopulationPlanInputPaths<'a> {
    population_plan: &'a Path,
    candidates: &'a Path,
    selection: &'a Path,
    origin_environment: &'a Path,
}

fn derive_provisional_organism_body_profile_plan(
    inputs: PopulationPlanInputPaths<'_>,
    metabolic_profiles_path: Option<&Path>,
    metabolic_rate_plan_path: Option<&Path>,
    life_history_profiles_path: Option<&Path>,
    body_mass_profiles_path: Option<&Path>,
    tick_duration_seconds: u32,
    output_path: &Path,
) -> Result<()> {
    if tick_duration_seconds == 0 {
        bail!("tick duration must be positive");
    }
    let (_, selection, _, population) = load_population_plan_inputs(
        inputs.candidates,
        inputs.selection,
        inputs.origin_environment,
        inputs.population_plan,
    )?;
    let sourced_metabolic = match (metabolic_profiles_path, metabolic_rate_plan_path) {
        (None, None) => None,
        (Some(profiles_path), Some(plan_path)) => {
            let (profiles, _) = load_metabolic_profiles(profiles_path)?;
            let metabolic_plan =
                FaunaMetabolicRatePlan::from_canonical_slice(&fs::read(plan_path).with_context(
                    || format!("read fauna metabolic-rate plan {}", plan_path.display()),
                )?)
                .context("validate fauna metabolic-rate plan")?;
            for selection in &metabolic_plan.selections {
                if !population
                    .entries
                    .iter()
                    .any(|entry| entry.species == selection.species)
                {
                    bail!(
                        "fauna metabolic-rate plan contains unplanned species {}",
                        selection.species.scientific_name
                    );
                }
                selection
                    .resolve(&profiles)
                    .context("resolve selected fauna metabolic observation")?;
            }
            Some((profiles, metabolic_plan))
        }
        _ => bail!(
            "metabolic profiles and metabolic-rate plan must be supplied together or both omitted"
        ),
    };
    let life_history_profiles = life_history_profiles_path
        .map(|path| {
            let bytes = fs::read(path)
                .with_context(|| format!("read life-history profiles {}", path.display()))?;
            let profiles = FaunaPhysiologyProfileSet::from_canonical_slice(&bytes)
                .context("validate life-history profile set")?;
            Ok::<_, anyhow::Error>((profiles, Digest::sha256(&bytes)))
        })
        .transpose()?;
    let body_mass_profiles = body_mass_profiles_path
        .map(|path| {
            let bytes = fs::read(path)
                .with_context(|| format!("read body-mass profiles {}", path.display()))?;
            let profiles = FaunaPhysiologyProfileSet::from_canonical_slice(&bytes)
                .context("validate body-mass profile set")?;
            Ok::<_, anyhow::Error>((profiles, Digest::sha256(&bytes)))
        })
        .transpose()?;
    let mut entries = Vec::with_capacity(population.entries.len() + 1);
    let human = SpeciesIdentity::new(
        "gbif",
        "2436436",
        "Homo sapiens",
        "https://www.gbif.org/species/2436436",
    )
    .expect("static valid Homo sapiens identity");
    entries.push(engineering_body_profile_entry(
        human.clone(),
        engineering_metabolic_commitment(human),
        tick_duration_seconds,
        "homo_sapiens",
        None,
        None,
    )?);
    for fauna in &population.entries {
        let metabolic_rate = sourced_metabolic
            .as_ref()
            .and_then(|(profiles, metabolic_plan)| {
                metabolic_plan
                    .selection_for(&fauna.species)
                    .map(|selection| (profiles, selection))
            })
            .map(|(profiles, selection)| {
                selection
                    .resolve_commitment(profiles)
                    .context("resolve exact fauna metabolic commitment")
            })
            .transpose()?
            .unwrap_or_else(|| engineering_metabolic_commitment(fauna.species.clone()));
        entries.push(engineering_body_profile_entry(
            fauna.species.clone(),
            metabolic_rate,
            tick_duration_seconds,
            &selection
                .selected_candidates
                .iter()
                .find(|candidate| candidate.species == fauna.species)
                .expect("validated population species belongs to its seeded selection")
                .range_package,
            life_history_profiles.as_ref(),
            body_mass_profiles.as_ref(),
        )?);
    }
    entries.sort_by(|left, right| {
        (&left.species.catalog, &left.species.identifier)
            .cmp(&(&right.species.catalog, &right.species.identifier))
    });
    let plan = ProvisionalOrganismBodyProfilePlan {
        plan_schema_version: PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION,
        status: PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS.to_owned(),
        tick_duration_seconds,
        entries,
    };
    let bytes = plan
        .canonical_bytes()
        .context("encode provisional organism body-profile plan")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "species_count": plan.entries.len(),
            "status": plan.status,
            "source_measured_fauna_metabolic_count": sourced_metabolic.as_ref().map_or(0, |(_, plan)| plan.selections.len()),
            "engineering_assumption_fauna_metabolic_count": population.entries.len() - sourced_metabolic.as_ref().map_or(0, |(_, plan)| plan.selections.len()),
            "source_informed_category_maturity_count": plan.entries.iter().flat_map(|entry| &entry.reproductive_physiology.category_maturity).filter(|entry| entry.evidence_basis == PhysiologicalEvidenceBasis::LiteratureApproximation).count(),
            "engineering_assumption_category_maturity_count": plan.entries.iter().flat_map(|entry| &entry.reproductive_physiology.category_maturity).filter(|entry| entry.evidence_basis == PhysiologicalEvidenceBasis::EngineeringAssumption).count(),
            "source_informed_adult_body_mass_count": plan.entries.iter().filter_map(|entry| entry.adult_body_mass.as_ref()).filter(|entry| entry.evidence_basis == PhysiologicalEvidenceBasis::LiteratureApproximation).count(),
            "engineering_assumption_adult_body_mass_count": plan.entries.iter().filter_map(|entry| entry.adult_body_mass.as_ref()).filter(|entry| entry.evidence_basis == PhysiologicalEvidenceBasis::EngineeringAssumption).count(),
            "provisional_reproduction_pacing": "each female and male category uses its exact retained species maturity aggregate when present; every missing category falls back independently to a coarse simulation-time engineering guardrail",
            "policy": "human metabolism, regulation, reproduction, body mass, and heredity remain engineering assumptions; retained Amniote maturity and adult-body-mass aggregates are source-addressed literature approximations, never raw observations; every uncovered category, mass, and metabolic rate remains an explicit engineering assumption",
        }))?
    );
    Ok(())
}

fn provisional_oral_transfer_profiles(
    material: &MaterialIdentity,
    body_profiles: &ProvisionalOrganismBodyProfilePlan,
    transfer_mass_milligrams: u64,
    recoverable_energy_joules: u64,
    hydration_recovery_seconds: u64,
) -> Vec<OralTransferCommitment> {
    body_profiles
        .entries
        .iter()
        .map(|entry| {
            let profile_id = format!(
                "{}-{}-{}-oral-v1",
                material
                    .canonical_name
                    .to_ascii_lowercase()
                    .replace(|character: char| !character.is_ascii_alphanumeric(), "-"),
                entry.species.catalog,
                entry.species.identifier
            );
            let profile_digest = Digest::sha256(
                format!(
                    "a-tiny-civilization/provisional-oral-transfer/v1/{}/{}/{}/{}/{}/{}/{}",
                    material.catalog,
                    material.identifier,
                    entry.species.catalog,
                    entry.species.identifier,
                    transfer_mass_milligrams,
                    recoverable_energy_joules,
                    hydration_recovery_seconds
                )
                .as_bytes(),
            );
            OralTransferCommitment {
                commitment_schema_version: ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
                profile_id,
                profile_digest,
                material: material.clone(),
                species: entry.species.clone(),
                evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
                transfer_mass_milligrams,
                recoverable_energy_joules,
                hydration_recovery_seconds,
            }
        })
        .collect()
}

struct ProvisionalMaterialSourceSpec {
    source_id: &'static str,
    initial_mass_milligrams: u64,
    maximum_mass_milligrams: u64,
    replenishment_mass_milligrams_per_tick: u64,
    transfer_mass_milligrams: u64,
    recoverable_energy_joules: u64,
    hydration_recovery_seconds: u64,
}

fn provisional_material_source(
    material: MaterialIdentity,
    anchor_patch: S2CellId,
    coverage_patch: S2CellId,
    spec: ProvisionalMaterialSourceSpec,
    body_profiles: &ProvisionalOrganismBodyProfilePlan,
) -> ProvisionalMaterialResourceSource {
    let profile_digest = Digest::sha256(
        format!(
            "a-tiny-civilization/provisional-material-reservoir/v1/{}/{}/{}/{}/{}/{}/{}",
            material.catalog,
            material.identifier,
            coverage_patch,
            spec.initial_mass_milligrams,
            spec.maximum_mass_milligrams,
            spec.replenishment_mass_milligrams_per_tick,
            spec.source_id
        )
        .as_bytes(),
    );
    let oral_transfer_profiles = provisional_oral_transfer_profiles(
        &material,
        body_profiles,
        spec.transfer_mass_milligrams,
        spec.recoverable_energy_joules,
        spec.hydration_recovery_seconds,
    );
    ProvisionalMaterialResourceSource {
        source_id: spec.source_id.to_owned(),
        material: material.clone(),
        anchor_patch,
        initial_mass_milligrams: spec.initial_mass_milligrams,
        reservoir: Some(MaterialReservoirCommitment {
            commitment_schema_version: MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
            profile_id: format!("{}-v1", spec.source_id),
            profile_digest,
            material,
            evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
            coverage_patch,
            maximum_mass_milligrams: spec.maximum_mass_milligrams,
            replenishment_mass_milligrams_per_tick: spec.replenishment_mass_milligrams_per_tick,
        }),
        oral_transfer_profiles,
    }
}

fn provisional_material_object(
    material: MaterialIdentity,
    anchor_patch: S2CellId,
    source_id: &'static str,
    initial_mass_milligrams: u64,
) -> ProvisionalMaterialResourceSource {
    ProvisionalMaterialResourceSource {
        source_id: source_id.to_owned(),
        material,
        anchor_patch,
        initial_mass_milligrams,
        reservoir: None,
        oral_transfer_profiles: Vec::new(),
    }
}

fn derive_provisional_material_resource_plan(
    inputs: PopulationPlanInputPaths<'_>,
    body_profile_plan_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let (_, _, environment, population) = load_population_plan_inputs(
        inputs.candidates,
        inputs.selection,
        inputs.origin_environment,
        inputs.population_plan,
    )?;
    let population_bytes = fs::read(inputs.population_plan).with_context(|| {
        format!(
            "read fauna population plan {}",
            inputs.population_plan.display()
        )
    })?;
    let environment_bytes = fs::read(inputs.origin_environment).with_context(|| {
        format!(
            "read origin environment {}",
            inputs.origin_environment.display()
        )
    })?;
    let body_profile_bytes = fs::read(body_profile_plan_path).with_context(|| {
        format!(
            "read organism body-profile plan {}",
            body_profile_plan_path.display()
        )
    })?;
    let body_profiles =
        ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&body_profile_bytes)
            .context("validate organism body-profile plan")?;
    let expected_species_count = population.entries.len() + 1;
    if body_profiles.entries.len() != expected_species_count {
        bail!(
            "organism body-profile plan must cover Homo sapiens plus every planned fauna species"
        );
    }

    let glucose = MaterialIdentity::new(
        "pubchem",
        "5793",
        "D-glucose",
        "https://pubchem.ncbi.nlm.nih.gov/compound/5793",
    )?;
    let water = MaterialIdentity::new(
        "pubchem",
        "962",
        "water",
        "https://pubchem.ncbi.nlm.nih.gov/compound/962",
    )?;
    let silicon_dioxide = MaterialIdentity::new(
        "pubchem",
        "24261",
        "silicon dioxide",
        "https://pubchem.ncbi.nlm.nih.gov/compound/24261",
    )?;
    let mut sources = vec![
        provisional_material_object(
            silicon_dioxide,
            environment.selected_embodied_patch,
            "pubchem-24261-object",
            100_000,
        ),
        provisional_material_source(
            glucose,
            environment.selected_embodied_patch,
            environment.selected_l10_patch,
            ProvisionalMaterialSourceSpec {
                source_id: "pubchem-5793-reservoir",
                initial_mass_milligrams: 10_000_000_000,
                maximum_mass_milligrams: 100_000_000_000,
                replenishment_mass_milligrams_per_tick: 5_000_000,
                transfer_mass_milligrams: 100_000,
                recoverable_energy_joules: 1_600_000,
                hydration_recovery_seconds: 0,
            },
            &body_profiles,
        ),
        provisional_material_source(
            water,
            environment.selected_embodied_patch,
            environment.selected_l10_patch,
            ProvisionalMaterialSourceSpec {
                source_id: "pubchem-962-reservoir",
                initial_mass_milligrams: 100_000_000_000,
                maximum_mass_milligrams: 1_000_000_000_000,
                replenishment_mass_milligrams_per_tick: 50_000_000,
                transfer_mass_milligrams: 250_000,
                recoverable_energy_joules: 0,
                hydration_recovery_seconds: 21_600,
            },
            &body_profiles,
        ),
    ];
    sources.sort_by(|left, right| {
        (
            &left.material.catalog,
            &left.material.identifier,
            &left.source_id,
        )
            .cmp(&(
                &right.material.catalog,
                &right.material.identifier,
                &right.source_id,
            ))
    });
    let plan = ProvisionalMaterialResourcePlan {
        plan_schema_version: PROVISIONAL_MATERIAL_RESOURCE_PLAN_SCHEMA_VERSION,
        status: PROVISIONAL_MATERIAL_RESOURCE_PLAN_STATUS.to_owned(),
        world_seed: population.world_seed,
        tick_duration_seconds: body_profiles.tick_duration_seconds,
        origin_environment_digest: Digest::sha256(&environment_bytes),
        fauna_population_plan_digest: Digest::sha256(&population_bytes),
        organism_body_profile_plan_digest: Digest::sha256(&body_profile_bytes),
        embodied_patch: environment.selected_embodied_patch,
        sources,
    };
    let bytes = plan
        .canonical_bytes(&body_profiles)
        .context("encode provisional material-resource plan")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "source_count": plan.sources.len(),
            "species_count": body_profiles.entries.len(),
            "coverage_patch": environment.selected_l10_patch,
            "status": plan.status,
            "policy": "real PubChem material identities; regional availability, replenishment, and species responses are explicit engineering assumptions",
        }))?
    );
    Ok(())
}

fn inspect_fauna_seeded_selection(input: &Path, candidates_path: &Path) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path).with_context(|| {
        format!(
            "read fauna range candidate set {}",
            candidates_path.display()
        )
    })?;
    let candidates =
        FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes).with_context(|| {
            format!(
                "validate fauna range candidate set {}",
                candidates_path.display()
            )
        })?;
    let selection_bytes = fs::read(input)
        .with_context(|| format!("read fauna seeded selection {}", input.display()))?;
    let selection =
        FaunaSeededSelection::from_canonical_slice_against(&selection_bytes, &candidates)
            .with_context(|| format!("validate fauna seeded selection {}", input.display()))?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "candidate_set_digest": selection.candidate_set_digest,
            "content_hash": Digest::sha256(&selection_bytes),
            "selected_candidate_count": selection.selected_candidates.len(),
            "species_limit": selection.species_limit,
            "status": "verified-seeded-range-selection-not-population-or-organism-creation",
            "world_seed": selection.world_seed,
        }))?
    );
    Ok(())
}

fn derive_provisional_land_origin_selection(
    root_index_path: &Path,
    artifact_root: &Path,
    world_seed: WorldSeed,
    embodied_patch_level: u8,
    output_path: &Path,
) -> Result<()> {
    let (root_digest, eligible_patches) =
        eligible_land_reference_patches(root_index_path, artifact_root)?;
    let selection = ProvisionalLandOriginSelection::select(
        world_seed,
        root_digest,
        eligible_patches,
        embodied_patch_level,
    )
    .context("select deterministic provisional land origin")?;
    let bytes = selection
        .canonical_bytes()
        .context("encode provisional land origin")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "eligible_patch_count": selection.eligible_patch_count,
            "land_reference_root_digest": selection.land_reference_root_digest,
            "selected_patch": selection.selected_patch,
            "selected_embodied_patch": selection.selected_embodied_patch,
            "status": "seed-derived-land-origin-not-habitat-or-population",
            "world_seed": selection.world_seed,
        }))?
    );
    Ok(())
}

fn inspect_provisional_land_origin_selection(
    input: &Path,
    root_index_path: &Path,
    artifact_root: &Path,
) -> Result<()> {
    let bytes = fs::read(input)
        .with_context(|| format!("read provisional land origin {}", input.display()))?;
    let selection = ProvisionalLandOriginSelection::from_canonical_slice(&bytes)
        .with_context(|| format!("decode provisional land origin {}", input.display()))?;
    let (root_digest, eligible_patches) =
        eligible_land_reference_patches(root_index_path, artifact_root)?;
    if selection.land_reference_root_digest != root_digest {
        bail!("provisional land origin references a different land-reference root");
    }
    selection
        .validate_against(eligible_patches)
        .context("recompute provisional land origin")?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "eligible_patch_count": selection.eligible_patch_count,
            "land_reference_root_digest": selection.land_reference_root_digest,
            "selected_patch": selection.selected_patch,
            "selected_embodied_patch": selection.selected_embodied_patch,
            "status": "verified-seed-derived-land-origin-not-habitat-or-population",
            "world_seed": selection.world_seed,
        }))?
    );
    Ok(())
}

fn inspect_fauna_population_plan(
    input: &Path,
    candidates_path: &Path,
    selection_path: &Path,
    origin_environment_path: &Path,
) -> Result<()> {
    let candidate_bytes = fs::read(candidates_path)
        .with_context(|| format!("read fauna candidates {}", candidates_path.display()))?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes)
        .context("validate fauna candidates")?;
    let selection_bytes = fs::read(selection_path)
        .with_context(|| format!("read fauna seeded selection {}", selection_path.display()))?;
    let selection =
        FaunaSeededSelection::from_canonical_slice_against(&selection_bytes, &candidates)
            .context("validate fauna seeded selection")?;
    let environment_bytes = fs::read(origin_environment_path).with_context(|| {
        format!(
            "read provisional origin environment {}",
            origin_environment_path.display()
        )
    })?;
    let environment = ProvisionalOriginEnvironment::from_canonical_slice(&environment_bytes)
        .context("validate provisional origin environment")?;
    let bytes = fs::read(input)
        .with_context(|| format!("read fauna population plan {}", input.display()))?;
    let plan = FaunaPopulationPlan::from_canonical_slice_against_environment(
        &bytes,
        &candidates,
        &selection,
        &environment,
    )
    .context("validate fauna population plan")?;
    let initial_individual_count = plan.entries.iter().try_fold(0_u64, |total, entry| {
        total
            .checked_add(u64::from(entry.initial_individual_count))
            .context("fauna population count overflow")
    })?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "origin_environment_digest": plan.origin_environment_digest,
            "embodied_patch": plan.embodied_patch,
            "species_count": plan.entries.len(),
            "initial_individual_count": initial_individual_count,
            "status": plan.status,
        }))?
    );
    Ok(())
}

/// Join just two already-pinned full-Earth layers at the selected L10 origin.
///
/// This intentionally stops at evidence. In particular, a surface class and a
/// temperature normal are not a habitat model, and a habitat model is not an
/// abundance or species-occurrence model.
fn inspect_provisional_origin_environment(
    origin_selection_path: &Path,
    composition_path: &Path,
    artifact_root: &Path,
) -> Result<()> {
    let environment = derive_provisional_origin_environment(
        origin_selection_path,
        composition_path,
        artifact_root,
    )?;
    println!("{}", serde_json::to_string(&environment)?);
    Ok(())
}

fn derive_provisional_origin_environment(
    origin_selection_path: &Path,
    composition_path: &Path,
    artifact_root: &Path,
) -> Result<ProvisionalOriginEnvironment> {
    let origin_bytes = fs::read(origin_selection_path).with_context(|| {
        format!(
            "read provisional origin selection {}",
            origin_selection_path.display()
        )
    })?;
    let origin = ProvisionalLandOriginSelection::from_canonical_slice(&origin_bytes)
        .context("decode provisional origin selection")?;
    let composition_bytes = fs::read(composition_path).with_context(|| {
        format!(
            "read provisional composition {}",
            composition_path.display()
        )
    })?;
    let composition = load_provisional_world_composition(composition_path)
        .context("decode provisional composition")?;
    let coastline = composition
        .earth_layers
        .iter()
        .find(|layer| layer.kind == DataLayerKind::Coastline)
        .context("provisional composition has no coastline layer")?;
    if coastline.release.content_hash != origin.land_reference_root_digest {
        bail!("provisional origin selection does not match composition coastline root");
    }
    let habitat = composition
        .earth_layers
        .iter()
        .find(|layer| layer.kind == DataLayerKind::Habitat)
        .context("provisional composition has no habitat layer")?;
    let climate = composition
        .earth_layers
        .iter()
        .find(|layer| layer.kind == DataLayerKind::Climate)
        .context("provisional composition has no climate layer")?;

    let habitat_root = read_pinned_provisional_root(artifact_root, &habitat.release)?;
    let habitat_tile_root = provisional_tile_tree_parent(artifact_root, &habitat.release)?;
    let habitat_entry = find_tile_entry_for_target(
        &habitat_tile_root,
        &habitat_root,
        "observed-land-cover",
        origin.selected_patch,
    )?;
    let habitat_bytes = read_tile_tree_artifact(&habitat_tile_root, &habitat_entry)?;
    let habitat_tile = PackedLandCoverEvidenceTile::from_canonical_slice(&habitat_bytes)
        .context("decode observed-land-cover tile")?;
    if habitat_tile.layer_id != "observed-land-cover" || habitat_tile.target_s2_level != 10 {
        bail!("observed-land-cover tile has an unexpected layer or target level");
    }
    let observed_land_cover = habitat_tile
        .cells
        .into_iter()
        .find(|cell| cell.s2_cell_id == origin.selected_patch)
        .context("observed-land-cover tile does not contain selected origin")?;

    let climate_root = read_pinned_provisional_root(artifact_root, &climate.release)?;
    let climate_tile_root = provisional_tile_tree_parent(artifact_root, &climate.release)?;
    let climate_entry = find_tile_entry_for_target(
        &climate_tile_root,
        &climate_root,
        "near-surface-air-temperature-normal",
        origin.selected_patch,
    )?;
    let climate_bytes = read_tile_tree_artifact(&climate_tile_root, &climate_entry)?;
    let climate_tile = PackedSeasonalScalarFieldTile::from_canonical_slice(&climate_bytes)
        .context("decode near-surface-air-temperature-normal tile")?;
    if climate_tile.layer_id != "near-surface-air-temperature-normal"
        || climate_tile.target_s2_level != 10
    {
        bail!("temperature tile has an unexpected layer or target level");
    }
    let air_temperature_normal = climate_tile
        .cells
        .into_iter()
        .find(|cell| cell.s2_cell_id == origin.selected_patch)
        .context("temperature tile does not contain selected origin")?;

    let environment = ProvisionalOriginEnvironment {
        environment_schema_version: 1,
        status: "evidence-only-not-habitat-suitability-or-population".to_owned(),
        origin_selection_digest: Digest::sha256(&origin_bytes),
        composition_digest: Digest::sha256(&composition_bytes),
        selected_l10_patch: origin.selected_patch,
        selected_embodied_patch: origin.selected_embodied_patch,
        observed_land_cover_root_digest: habitat.release.content_hash,
        observed_land_cover_tile_digest: Digest::sha256(&habitat_bytes),
        observed_land_cover,
        air_temperature_normal_root_digest: climate.release.content_hash,
        air_temperature_normal_tile_digest: Digest::sha256(&climate_bytes),
        air_temperature_normal_unit: climate_tile.unit,
        air_temperature_normal_decimal_places: climate_tile.decimal_places,
        air_temperature_normal,
    };
    environment
        .validate()
        .context("validate origin environment")?;
    Ok(environment)
}

fn derive_and_write_provisional_origin_environment(
    origin_selection_path: &Path,
    composition_path: &Path,
    artifact_root: &Path,
    output_path: &Path,
) -> Result<()> {
    let environment = derive_provisional_origin_environment(
        origin_selection_path,
        composition_path,
        artifact_root,
    )?;
    let bytes = environment
        .canonical_bytes()
        .context("encode canonical provisional origin environment")?;
    write_new_artifact(output_path, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "content_hash": Digest::sha256(&bytes),
            "output_path": output_path,
            "selected_l10_patch": environment.selected_l10_patch,
            "selected_embodied_patch": environment.selected_embodied_patch,
            "status": environment.status,
        }))?
    );
    Ok(())
}

fn read_pinned_provisional_root(
    artifact_root: &Path,
    release: &world_data::ProvisionalArtifactReference,
) -> Result<TileTreeIndex> {
    let bytes = read_release_file(artifact_root, &release.artifact_path)?;
    if u64::try_from(bytes.len())? != release.byte_length
        || Digest::sha256(&bytes) != release.content_hash
    {
        bail!("provisional layer root differs from its composition reference");
    }
    TileTreeIndex::from_canonical_slice(&bytes).context("decode canonical provisional layer root")
}

fn provisional_tile_tree_parent(
    artifact_root: &Path,
    release: &world_data::ProvisionalArtifactReference,
) -> Result<PathBuf> {
    let canonical_root = artifact_root
        .canonicalize()
        .with_context(|| format!("resolve artifact root {}", artifact_root.display()))?;
    let index_path = canonical_root.join(&release.artifact_path);
    let metadata = fs::symlink_metadata(&index_path)
        .with_context(|| format!("inspect provisional layer root {}", index_path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("provisional layer root must be a regular file");
    }
    let resolved = index_path
        .canonicalize()
        .with_context(|| format!("resolve provisional layer root {}", index_path.display()))?;
    if !resolved.starts_with(&canonical_root) {
        bail!("provisional layer root escapes the artifact root");
    }
    // A full-Earth layer root is stored at `layers/<layer-id>/root.index`, while
    // index entries are relative to the release directory (`layers/...`).
    resolved
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("provisional layer root is not beneath layers/<layer-id>/root.index")
}

fn find_tile_entry_for_target(
    artifact_root: &Path,
    root: &TileTreeIndex,
    expected_layer_id: &str,
    target: S2CellId,
) -> Result<TileTreeEntry> {
    let canonical_root = artifact_root
        .canonicalize()
        .with_context(|| format!("resolve artifact root {}", artifact_root.display()))?;
    let mut index = root.clone();
    loop {
        if index.layer_id != expected_layer_id {
            bail!("tile-tree index has unexpected layer id");
        }
        let matches = index
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .s2_cell_id
                    .parse::<S2CellId>()
                    .is_ok_and(|container| container.contains(target))
            })
            .collect::<Vec<_>>();
        let [entry] = matches.as_slice() else {
            bail!("tile tree does not have exactly one container for selected origin");
        };
        match entry.kind {
            TileTreeEntryKind::Tile => return Ok((*entry).clone()),
            TileTreeEntryKind::Index => {
                let bytes = read_tile_tree_artifact(&canonical_root, entry)?;
                index = TileTreeIndex::from_canonical_slice(&bytes)
                    .context("decode canonical child tile-tree index")?;
            }
        }
    }
}

fn eligible_land_reference_patches(
    root_index_path: &Path,
    artifact_root: &Path,
) -> Result<(Digest, Vec<S2CellId>)> {
    let root_bytes = fs::read(root_index_path).with_context(|| {
        format!(
            "read land-reference root index {}",
            root_index_path.display()
        )
    })?;
    let root_digest = Digest::sha256(&root_bytes);
    let root_index = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical land-reference root index")?;
    if root_index.layer_id != "land-reference" {
        bail!("land-origin selection requires the land-reference layer");
    }
    let artifact_root = artifact_root.canonicalize().with_context(|| {
        format!(
            "resolve land-reference artifact root {}",
            artifact_root.display()
        )
    })?;
    let mut pending = VecDeque::from(root_index.entries);
    let mut tile_entries = Vec::new();
    while let Some(entry) = pending.pop_front() {
        match entry.kind {
            TileTreeEntryKind::Index => {
                let bytes = read_tile_tree_artifact(&artifact_root, &entry)?;
                let index = TileTreeIndex::from_canonical_slice(&bytes)
                    .context("decode canonical land-reference child index")?;
                if index.layer_id != "land-reference" {
                    bail!("land-reference child index has another layer id");
                }
                pending.extend(index.entries);
            }
            TileTreeEntryKind::Tile => tile_entries.push(entry),
        }
    }
    let eligible_patches = tile_entries
        .par_iter()
        .map(|entry| {
            let bytes = read_tile_tree_artifact(&artifact_root, entry)?;
            let tile = PackedBooleanFieldTile::from_canonical_slice(&bytes)
                .context("decode canonical land-reference tile")?;
            if tile.layer_id != "land-reference" {
                bail!("land-reference tile has another layer id");
            }
            Ok::<_, anyhow::Error>(
                tile.cells
                    .into_iter()
                    .filter(|cell| cell.true_samples > 0)
                    .map(|cell| cell.s2_cell_id)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut eligible_patches = eligible_patches;
    eligible_patches.sort_unstable();
    if eligible_patches.windows(2).any(|pair| pair[0] == pair[1]) {
        bail!("land-reference tile tree contains duplicate target patches");
    }
    Ok((root_digest, eligible_patches))
}

fn read_tile_tree_artifact(artifact_root: &Path, entry: &TileTreeEntry) -> Result<Vec<u8>> {
    if entry
        .artifact
        .path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        bail!("unsafe tile-tree artifact path {:?}", entry.artifact.path);
    }
    let requested = artifact_root.join(&entry.artifact.path);
    let metadata = fs::symlink_metadata(&requested)
        .with_context(|| format!("inspect tile-tree artifact {}", requested.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!(
            "tile-tree artifact is not a regular file: {}",
            requested.display()
        );
    }
    let resolved = requested
        .canonicalize()
        .with_context(|| format!("resolve tile-tree artifact {}", requested.display()))?;
    if !resolved.starts_with(artifact_root) {
        bail!("tile-tree artifact escapes the artifact root");
    }
    let bytes = fs::read(&resolved)
        .with_context(|| format!("read tile-tree artifact {}", resolved.display()))?;
    if u64::try_from(bytes.len())? != entry.artifact.byte_length
        || Digest::sha256(&bytes) != entry.artifact.content_hash
    {
        bail!("tile-tree artifact bytes disagree with its canonical reference");
    }
    Ok(bytes)
}

#[derive(Debug, Serialize)]
struct FaunaTaxonomyCatalogInspection {
    input_path: String,
    input_hash: Digest,
    input_byte_length: u64,
    accepted_species: u64,
}

#[derive(Debug, Serialize)]
struct FaunaTraitSourceTaxaInspection {
    source_id: &'static str,
    input_path: String,
    input_hash: Digest,
    input_byte_length: u64,
    source_records: u64,
    records_without_scientific_name: u64,
    distinct_scientific_names: usize,
    exact_accepted_matches: usize,
    ambiguous_accepted_matches: usize,
    unmatched_names: usize,
    unmatched_name_examples: Vec<String>,
}

fn inspect_fauna_trait_taxa(
    catalog_path: &Path,
    animaltraits_path: &Path,
    elton_birds_path: &Path,
    elton_mammals_path: &Path,
) -> Result<()> {
    let (catalog_byte_length, catalog_hash) = digest_file(catalog_path)?;
    let (accepted_species, catalog_names) = load_gbif_accepted_species_names(catalog_path)?;
    let sources = [
        (
            "animal-traits-1.0.7",
            animaltraits_path,
            b',' as char,
            "species",
            SourceTextEncoding::Utf8,
        ),
        (
            "elton-traits-1.0-birds",
            elton_birds_path,
            b'\t' as char,
            "Scientific",
            SourceTextEncoding::Windows1252,
        ),
        (
            "elton-traits-1.0-mammals",
            elton_mammals_path,
            b'\t' as char,
            "Scientific",
            SourceTextEncoding::Windows1252,
        ),
    ];
    let mut inspections = Vec::with_capacity(sources.len());
    for (source_id, path, delimiter, scientific_column, encoding) in sources {
        let (byte_length, hash) = digest_file(path)?;
        let (source_records, records_without_scientific_name, names) =
            read_source_scientific_names(path, delimiter, scientific_column, encoding)?;
        let mut exact_accepted_matches = 0_usize;
        let mut ambiguous_accepted_matches = 0_usize;
        let mut unmatched = Vec::new();
        for name in &names {
            match catalog_names.get(name.as_str()).map(Vec::len) {
                Some(1) => exact_accepted_matches += 1,
                Some(_) => ambiguous_accepted_matches += 1,
                None => unmatched.push(name.clone()),
            }
        }
        inspections.push(FaunaTraitSourceTaxaInspection {
            source_id,
            input_path: path.display().to_string(),
            input_hash: hash,
            input_byte_length: byte_length,
            source_records,
            records_without_scientific_name,
            distinct_scientific_names: names.len(),
            exact_accepted_matches,
            ambiguous_accepted_matches,
            unmatched_names: unmatched.len(),
            unmatched_name_examples: unmatched.into_iter().take(25).collect(),
        });
    }
    println!(
        "{}",
        serde_json::to_string(&FaunaTraitTaxaInspection {
            inspection_schema_version: 1,
            catalog: FaunaTaxonomyCatalogInspection {
                input_path: catalog_path.display().to_string(),
                input_hash: catalog_hash,
                input_byte_length: catalog_byte_length,
                accepted_species,
            },
            sources: inspections,
            policy: "scientific names are matched byte-for-byte after removing only leading and trailing ASCII whitespace; zero or multiple accepted GBIF records remain unresolved and no synonym, fuzzy, genus-only, or taxonomic-rank inference is performed",
        })?
    );
    Ok(())
}

fn load_gbif_accepted_species_names(path: &Path) -> Result<(u64, BTreeMap<String, Vec<u64>>)> {
    let mut reader = BufReader::new(
        File::open(path)
            .with_context(|| format!("open GBIF Animalia catalog {}", path.display()))?,
    );
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != GBIF_ANIMALIA_CATALOG_MAGIC {
        bail!("GBIF Animalia catalog magic is invalid");
    }
    let mut schema_bytes = [0_u8; 2];
    reader.read_exact(&mut schema_bytes)?;
    if u16::from_le_bytes(schema_bytes) != GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION {
        bail!("GBIF Animalia catalog schema is unsupported");
    }
    let mut source_hash = [0_u8; 32];
    reader.read_exact(&mut source_hash)?;
    let mut count_bytes = [0_u8; 8];
    reader.read_exact(&mut count_bytes)?;
    let record_count = u64::from_le_bytes(count_bytes);
    if record_count == 0 {
        bail!("GBIF Animalia catalog has no species");
    }
    let mut names = BTreeMap::<String, Vec<u64>>::new();
    for index in 0..record_count {
        let mut key_bytes = [0_u8; 8];
        reader
            .read_exact(&mut key_bytes)
            .with_context(|| format!("read GBIF catalog species {index} key"))?;
        let taxon_key = u64::from_le_bytes(key_bytes);
        if taxon_key == 0 {
            bail!("GBIF Animalia catalog has zero taxon key");
        }
        let _authored_scientific_name = read_length_prefixed_utf8(&mut reader)?;
        let canonical_name = read_length_prefixed_utf8(&mut reader)?;
        for _ in 0..5 {
            let _ = read_length_prefixed_utf8(&mut reader)?;
        }
        if canonical_name.is_empty() {
            continue;
        }
        names.entry(canonical_name).or_default().push(taxon_key);
    }
    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing)? != 0 {
        bail!("GBIF Animalia catalog contains trailing bytes");
    }
    Ok((record_count, names))
}

fn read_source_scientific_names(
    path: &Path,
    delimiter: char,
    scientific_column: &str,
    encoding: SourceTextEncoding,
) -> Result<(u64, u64, BTreeSet<String>)> {
    let raw =
        fs::read(path).with_context(|| format!("read retained fauna source {}", path.display()))?;
    let input = match encoding {
        SourceTextEncoding::Utf8 => String::from_utf8(raw).with_context(|| {
            format!(
                "retained fauna source {} is not valid UTF-8",
                path.display()
            )
        })?,
        SourceTextEncoding::Windows1252 => decode_windows_1252(&raw),
    };
    let mut rows = parse_delimited_records(&input, delimiter)?;
    let columns = rows
        .first()
        .cloned()
        .context("retained fauna source has no header row")?;
    rows.remove(0);
    let scientific_index = columns
        .iter()
        .position(|column| column == scientific_column)
        .with_context(|| {
            format!("retained fauna source is missing {scientific_column:?} column")
        })?;
    let mut records = 0_u64;
    let mut records_without_scientific_name = 0_u64;
    let mut names = BTreeSet::new();
    for (record_index, fields) in rows.into_iter().enumerate() {
        // The published tab-delimited Elton files terminate with completely blank
        // fixed-width rows. They are not source records. Preserve a structurally
        // nonempty anonymous row for reporting, but do not manufacture a record
        // from one whose every declared column is blank.
        if fields.iter().all(String::is_empty) {
            continue;
        }
        if fields.len() != columns.len() {
            bail!(
                "fauna source record {} has {} fields, expected {}",
                record_index + 2,
                fields.len(),
                columns.len()
            );
        }
        records = records
            .checked_add(1)
            .context("fauna source record count overflow")?;
        let name = fields[scientific_index]
            .trim_matches(|character: char| character.is_ascii_whitespace());
        if name.is_empty() {
            records_without_scientific_name = records_without_scientific_name
                .checked_add(1)
                .context("fauna source unnamed-record count overflow")?;
            continue;
        }
        names.insert(name.to_owned());
    }
    if records == 0 || names.is_empty() {
        bail!("retained fauna source has no data records");
    }
    Ok((records, records_without_scientific_name, names))
}

#[derive(Clone, Copy)]
enum SourceTextEncoding {
    Utf8,
    Windows1252,
}

/// Decodes the legacy Windows-1252 text files published by EltonTraits without
/// changing the source bytes used for provenance hashing.
fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match *byte {
            0x80 => '\u{20AC}',
            0x82 => '\u{201A}',
            0x83 => '\u{0192}',
            0x84 => '\u{201E}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02C6}',
            0x89 => '\u{2030}',
            0x8A => '\u{0160}',
            0x8B => '\u{2039}',
            0x8C => '\u{0152}',
            0x8E => '\u{017D}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201C}',
            0x94 => '\u{201D}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02DC}',
            0x99 => '\u{2122}',
            0x9A => '\u{0161}',
            0x9B => '\u{203A}',
            0x9C => '\u{0153}',
            0x9E => '\u{017E}',
            0x9F => '\u{0178}',
            byte => char::from(byte),
        })
        .collect()
}

fn parse_delimited_records(input: &str, delimiter: char) -> Result<Vec<Vec<String>>> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut at_field_start = true;
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            if character == '"' {
                if characters.peek().copied() == Some('"') {
                    field.push('"');
                    let _ = characters.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
        } else if character == delimiter {
            fields.push(std::mem::take(&mut field));
            at_field_start = true;
        } else if character == '\n' {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            at_field_start = true;
        } else if character == '\r' {
            if characters.peek().copied() == Some('\n') {
                let _ = characters.next();
            }
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            at_field_start = true;
        } else if character == '"' && at_field_start {
            quoted = true;
            at_field_start = false;
        } else {
            field.push(character);
            at_field_start = false;
        }
    }
    if quoted {
        bail!("unterminated quoted field");
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        records.push(fields);
    }
    Ok(records)
}

const JRC_OCCURRENCE_LONGITUDES: std::ops::Range<i32> = -18..18;
const JRC_OCCURRENCE_LATITUDES: std::ops::RangeInclusive<i32> = -5..=8;

#[derive(Debug, Serialize)]
struct JrcSurfaceWaterOccurrenceInspection {
    inspection_schema_version: u16,
    release: &'static str,
    evidence_period: &'static str,
    artifact_count: usize,
    byte_length: u64,
    coverage: &'static str,
    raster_width: u32,
    raster_height: u32,
    bits_per_sample: Vec<u16>,
    photometric_interpretation_code: u16,
    compression_code: u16,
    predictor_code: u16,
    chunk_type: String,
    chunk_width: u32,
    chunk_height: u32,
    chunks_per_artifact: u32,
    longitude_pixel_scale_ieee754_bits_hex: String,
    latitude_pixel_scale_ieee754_bits_hex: String,
    gdal_nodata: Option<String>,
    sampled_value_counts: BTreeMap<u8, u64>,
}

#[derive(Debug, Eq, PartialEq)]
struct JrcOccurrenceTiffProfile {
    width: u32,
    height: u32,
    bits_per_sample: Vec<u16>,
    photometric_interpretation_code: u16,
    compression_code: u16,
    predictor_code: u16,
    chunk_type: String,
    chunk_width: u32,
    chunk_height: u32,
    chunk_count: u32,
    longitude_scale_bits: u64,
    latitude_scale_bits: u64,
    gdal_nodata: Option<String>,
}

fn inspect_jrc_surface_water_occurrence(input_directory: &Path) -> Result<()> {
    let mut artifact_count = 0_usize;
    let mut byte_length = 0_u64;
    let mut common_profile: Option<JrcOccurrenceTiffProfile> = None;
    let mut sampled_value_counts = BTreeMap::new();

    for latitude_band in JRC_OCCURRENCE_LATITUDES.rev() {
        let latitude = latitude_band * 10;
        for longitude_band in JRC_OCCURRENCE_LONGITUDES {
            let longitude = longitude_band * 10;
            let longitude_code = jrc_coordinate_code(longitude, 'W', 'E');
            let latitude_code = jrc_coordinate_code(latitude, 'S', 'N');
            let filename = format!("occurrence_{longitude_code}_{latitude_code}_v1_5_2024.tif");
            let path = input_directory.join(&filename);
            let metadata = fs::metadata(&path)
                .with_context(|| format!("inspect JRC occurrence tile {}", path.display()))?;
            if !metadata.is_file() || metadata.len() == 0 {
                bail!(
                    "JRC occurrence tile is not a nonempty regular file: {}",
                    path.display()
                );
            }
            artifact_count += 1;
            byte_length = byte_length
                .checked_add(metadata.len())
                .context("JRC occurrence retained byte length overflow")?;
            let mut decoder = TiffDecoder::new(
                File::open(&path)
                    .with_context(|| format!("open JRC occurrence tile {}", path.display()))?,
            )
            .with_context(|| format!("parse JRC occurrence TIFF {}", path.display()))?;
            let (width, height) = decoder.dimensions()?;
            let bits_per_sample = decoder.get_tag_u16_vec(TiffTag::BitsPerSample)?;
            let photometric_interpretation_code =
                decoder.get_tag_unsigned::<u16>(TiffTag::PhotometricInterpretation)?;
            let compression_code = decoder.get_tag_unsigned::<u16>(TiffTag::Compression)?;
            let predictor_code = decoder
                .get_tag_unsigned::<u16>(TiffTag::Predictor)
                .unwrap_or(1);
            let chunk_kind = decoder.get_chunk_type();
            let chunk_type = match chunk_kind {
                TiffChunkType::Strip => "strip",
                TiffChunkType::Tile => "tile",
            }
            .to_owned();
            let chunk_count = match chunk_kind {
                TiffChunkType::Strip => decoder.strip_count(),
                TiffChunkType::Tile => decoder.tile_count(),
            }?;
            if chunk_count == 0 {
                bail!("JRC occurrence tile has no chunks: {}", path.display());
            }
            let (chunk_width, chunk_height) = decoder.chunk_dimensions();
            let pixel_scale = decoder
                .get_tag_f64_vec(TiffTag::ModelPixelScaleTag)
                .with_context(|| format!("read JRC pixel scale in {}", path.display()))?;
            let tiepoint = decoder
                .get_tag_f64_vec(TiffTag::ModelTiepointTag)
                .with_context(|| format!("read JRC tiepoint in {}", path.display()))?;
            if pixel_scale.len() < 2 || tiepoint.len() < 6 {
                bail!(
                    "JRC GeoTIFF geometry tags are incomplete: {}",
                    path.display()
                );
            }
            let west = tiepoint[3] - tiepoint[0] * pixel_scale[0];
            let north = tiepoint[4] + tiepoint[1] * pixel_scale[1];
            let east = west + f64::from(width) * pixel_scale[0];
            let south = north - f64::from(height) * pixel_scale[1];
            for (actual, expected, boundary) in [
                (west, f64::from(longitude), "west"),
                (east, f64::from(longitude + 10), "east"),
                (north, f64::from(latitude), "north"),
                (south, f64::from(latitude - 10), "south"),
            ] {
                if !actual.is_finite() || (actual - expected).abs() > 1e-9 {
                    bail!(
                        "JRC tile {} {boundary} boundary is {actual}, expected {expected}",
                        path.display()
                    );
                }
            }
            let gdal_nodata = decoder
                .get_tag_ascii_string(TiffTag::GdalNodata)
                .ok()
                .map(|value| value.trim_matches(['\0', ' ', '\r', '\n']).to_owned());
            let strip_offsets = decoder.get_tag_u64_vec(TiffTag::StripOffsets)?;
            let strip_byte_counts = decoder.get_tag_u64_vec(TiffTag::StripByteCounts)?;
            if strip_offsets.len() != usize::try_from(chunk_count)?
                || strip_byte_counts.len() != strip_offsets.len()
            {
                bail!("JRC strip tables are incomplete: {}", path.display());
            }
            let profile = JrcOccurrenceTiffProfile {
                width,
                height,
                bits_per_sample,
                photometric_interpretation_code,
                compression_code,
                predictor_code,
                chunk_type,
                chunk_width,
                chunk_height,
                chunk_count,
                longitude_scale_bits: pixel_scale[0].to_bits(),
                latitude_scale_bits: pixel_scale[1].to_bits(),
                gdal_nodata,
            };
            if let Some(expected) = &common_profile {
                if &profile != expected {
                    bail!("JRC occurrence TIFF profile differs: {}", path.display());
                }
            } else {
                common_profile = Some(profile);
            }

            if decoder.more_images() {
                bail!(
                    "JRC occurrence tile has unexpected extra images: {}",
                    path.display()
                );
            }
            drop(decoder);

            let mut raster_file = File::open(&path)?;
            for strip_index in [0, chunk_count / 2, chunk_count - 1] {
                let index = usize::try_from(strip_index)?;
                let offset = strip_offsets[index];
                let compressed_length = strip_byte_counts[index];
                if offset
                    .checked_add(compressed_length)
                    .is_none_or(|end| end > metadata.len())
                {
                    bail!("JRC strip lies outside its TIFF: {}", path.display());
                }
                raster_file.seek(SeekFrom::Start(offset))?;
                let mut compressed = vec![0_u8; usize::try_from(compressed_length)?];
                raster_file.read_exact(&mut compressed)?;
                let mut values = LzwDecoder::with_tiff_size_switch(LzwBitOrder::Msb, 8)
                    .decode(&compressed)
                    .with_context(|| {
                        format!("decode JRC LZW strip {strip_index} in {}", path.display())
                    })?;
                if values.len() != usize::try_from(width)? {
                    bail!(
                        "JRC strip {} decoded to {} samples, expected {width}",
                        path.display(),
                        values.len()
                    );
                }
                match predictor_code {
                    1 => {}
                    2 => undo_horizontal_u8_predictor(&mut values),
                    other => bail!("JRC TIFF uses unsupported predictor {other}"),
                }
                for value in values {
                    *sampled_value_counts.entry(value).or_default() += 1;
                }
            }
        }
    }

    let profile = common_profile.context("no JRC occurrence tiles were inspected")?;
    println!(
        "{}",
        serde_json::to_string(&JrcSurfaceWaterOccurrenceInspection {
            inspection_schema_version: 1,
            release: "JRC Global Surface Water v1.5 (2024 release)",
            evidence_period: "1984-2024 occurrence",
            artifact_count,
            byte_length,
            coverage: "longitude [-180, 180), latitude [-60, 80] in 10-degree source tiles",
            raster_width: profile.width,
            raster_height: profile.height,
            bits_per_sample: profile.bits_per_sample,
            photometric_interpretation_code: profile.photometric_interpretation_code,
            compression_code: profile.compression_code,
            predictor_code: profile.predictor_code,
            chunk_type: profile.chunk_type,
            chunk_width: profile.chunk_width,
            chunk_height: profile.chunk_height,
            chunks_per_artifact: profile.chunk_count,
            longitude_pixel_scale_ieee754_bits_hex: format!(
                "{:016x}",
                profile.longitude_scale_bits
            ),
            latitude_pixel_scale_ieee754_bits_hex: format!("{:016x}", profile.latitude_scale_bits),
            gdal_nodata: profile.gdal_nodata,
            sampled_value_counts,
        })?
    );
    Ok(())
}

fn jrc_coordinate_code(value: i32, negative_suffix: char, positive_suffix: char) -> String {
    format!(
        "{}{}",
        value.unsigned_abs(),
        if value < 0 {
            negative_suffix
        } else {
            positive_suffix
        }
    )
}

fn undo_horizontal_u8_predictor(values: &mut [u8]) {
    let mut previous = 0_u8;
    for value in values {
        *value = value.wrapping_add(previous);
        previous = *value;
    }
}

const JRC_OCCURRENCE_INVENTORY_SCHEMA_VERSION: u16 = 1;
const JRC_OCCURRENCE_RELEASE: &str = "VER1-5";
const JRC_OCCURRENCE_VERSION: &str = "v1_5_2024";
const JRC_OCCURRENCE_WIDTH: u32 = 40_000;
const JRC_OCCURRENCE_HEIGHT: u32 = 40_000;
const JRC_OCCURRENCE_PIXEL_E7: i32 = 2_500;
const JRC_OCCURRENCE_TILE_E7: i32 = 100_000_000;
const JRC_OCCURRENCE_MIN_LATITUDE_E7: i32 = -600_000_000;
const JRC_OCCURRENCE_MAX_LATITUDE_E7: i32 = 800_000_000;

#[derive(Debug, Deserialize)]
struct JrcOccurrenceInventory {
    inventory_schema_version: u16,
    release: String,
    version: String,
    artifact_count: usize,
    byte_length: u64,
    artifacts: Vec<JrcOccurrenceInventoryArtifact>,
}

#[derive(Clone, Debug, Deserialize)]
struct JrcOccurrenceInventoryArtifact {
    artifact_path: String,
    byte_length: u64,
    content_hash: Digest,
    download_url: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct JrcOccurrenceTileKey {
    west_degrees: i32,
    north_degrees: i32,
}

impl JrcOccurrenceTileKey {
    fn filename(self) -> String {
        format!(
            "occurrence_{}_{}_v1_5_2024.tif",
            jrc_coordinate_code(self.west_degrees, 'W', 'E'),
            jrc_coordinate_code(self.north_degrees, 'S', 'N')
        )
    }

    fn relative_path(self) -> String {
        format!(
            "jrc-global-surface-water-v1-5-2024/occurrence/{}",
            self.filename()
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct JrcOccurrenceSampleAddress {
    tile: JrcOccurrenceTileKey,
    row: u32,
    column: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct JrcOccurrenceSampleRequest {
    address: JrcOccurrenceSampleAddress,
    s2_cell_id: S2CellId,
}

/// Resolve a coordinate to the exact retained JRC source pixel containing it.
///
/// The official mosaic ends at 80 N and 60 S. A coordinate outside that footprint
/// returns `None`; callers retain that absence explicitly as -1 instead of silently
/// extending observed water evidence into polar gaps.
fn jrc_occurrence_sample_address(
    coordinate: GeographicCoordinateE7,
) -> Option<JrcOccurrenceSampleAddress> {
    let latitude_e7 = coordinate.latitude_e7();
    if latitude_e7 <= JRC_OCCURRENCE_MIN_LATITUDE_E7 || latitude_e7 > JRC_OCCURRENCE_MAX_LATITUDE_E7
    {
        return None;
    }
    let longitude_e7 = coordinate.longitude_e7();
    let west_band = longitude_e7.div_euclid(JRC_OCCURRENCE_TILE_E7);
    let west_e7 = west_band * JRC_OCCURRENCE_TILE_E7;
    let latitude_band = latitude_e7.div_euclid(JRC_OCCURRENCE_TILE_E7);
    let north_band = latitude_band + i32::from(latitude_e7.rem_euclid(JRC_OCCURRENCE_TILE_E7) != 0);
    let north_e7 = north_band * JRC_OCCURRENCE_TILE_E7;
    let row = u32::try_from((north_e7 - latitude_e7) / JRC_OCCURRENCE_PIXEL_E7).ok()?;
    let column = u32::try_from((longitude_e7 - west_e7) / JRC_OCCURRENCE_PIXEL_E7).ok()?;
    if row >= JRC_OCCURRENCE_HEIGHT || column >= JRC_OCCURRENCE_WIDTH {
        return None;
    }
    Some(JrcOccurrenceSampleAddress {
        tile: JrcOccurrenceTileKey {
            west_degrees: west_e7 / 10_000_000,
            north_degrees: north_e7 / 10_000_000,
        },
        row,
        column,
    })
}

struct JrcOccurrenceSourceSet {
    inventory_digest: Digest,
    artifact_set_digest: Digest,
    artifacts: BTreeMap<JrcOccurrenceTileKey, JrcOccurrenceInventoryArtifact>,
}

fn load_verified_jrc_occurrence_source_set(
    inventory_path: &Path,
    artifact_root: &Path,
) -> Result<JrcOccurrenceSourceSet> {
    let inventory_bytes = fs::read(inventory_path)
        .with_context(|| format!("read JRC source inventory {}", inventory_path.display()))?;
    let inventory: JrcOccurrenceInventory = serde_json::from_slice(&inventory_bytes)
        .with_context(|| format!("parse JRC source inventory {}", inventory_path.display()))?;
    if inventory.inventory_schema_version != JRC_OCCURRENCE_INVENTORY_SCHEMA_VERSION
        || inventory.release != JRC_OCCURRENCE_RELEASE
        || inventory.version != JRC_OCCURRENCE_VERSION
    {
        bail!("JRC occurrence inventory has an unsupported release contract");
    }
    let expected_count = JRC_OCCURRENCE_LONGITUDES.len()
        * usize::try_from(*JRC_OCCURRENCE_LATITUDES.end() - *JRC_OCCURRENCE_LATITUDES.start() + 1)?;
    if inventory.artifact_count != expected_count || inventory.artifacts.len() != expected_count {
        bail!("JRC occurrence inventory does not contain the complete 504-tile mosaic");
    }

    let mut artifacts = BTreeMap::new();
    let mut actual_total = 0_u64;
    let mut verified_artifacts = 0_usize;
    let mut artifact_set_hasher = Sha256::new();
    artifact_set_hasher.update(b"a-tiny-civilization:jrc-occurrence-artifact-set:v1\0");
    for north_band in JRC_OCCURRENCE_LATITUDES.rev() {
        for west_band in JRC_OCCURRENCE_LONGITUDES.clone() {
            let key = JrcOccurrenceTileKey {
                west_degrees: west_band * 10,
                north_degrees: north_band * 10,
            };
            let expected_path = key.relative_path();
            let artifact = inventory
                .artifacts
                .iter()
                .find(|artifact| artifact.artifact_path == expected_path)
                .with_context(|| format!("JRC source inventory is missing {expected_path}"))?
                .clone();
            if artifact
                .artifact_path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
            {
                bail!("JRC source inventory contains an unsafe artifact path");
            }
            if !artifact.download_url.starts_with(
                "https://storage.googleapis.com/water-world/download2024/VER1-5/occurrence/",
            ) || !artifact.download_url.ends_with(&key.filename())
            {
                bail!("JRC source inventory artifact URL does not match its release path");
            }
            let path = artifact_root.join(&artifact.artifact_path);
            let (actual_length, actual_hash) = digest_file(&path)
                .with_context(|| format!("verify retained JRC tile {}", path.display()))?;
            if actual_length != artifact.byte_length || actual_hash != artifact.content_hash {
                bail!(
                    "retained JRC tile differs from its hash inventory: {}",
                    path.display()
                );
            }
            actual_total = actual_total
                .checked_add(actual_length)
                .context("JRC occurrence source byte total overflow")?;
            artifact_set_hasher.update(u64::try_from(artifact.artifact_path.len())?.to_le_bytes());
            artifact_set_hasher.update(artifact.artifact_path.as_bytes());
            artifact_set_hasher.update(actual_length.to_le_bytes());
            artifact_set_hasher.update(actual_hash.as_bytes());
            if artifacts.insert(key, artifact).is_some() {
                bail!("JRC occurrence source inventory repeats a source tile");
            }
            verified_artifacts += 1;
            if verified_artifacts.is_multiple_of(32) || verified_artifacts == expected_count {
                eprintln!(
                    "JRC occurrence source verification progress: {verified_artifacts}/{expected_count} artifacts"
                );
            }
        }
    }
    if actual_total != inventory.byte_length {
        bail!("JRC occurrence source inventory byte total is inconsistent");
    }
    Ok(JrcOccurrenceSourceSet {
        inventory_digest: Digest::sha256(&inventory_bytes),
        artifact_set_digest: Digest::from_bytes(artifact_set_hasher.finalize().into()),
        artifacts,
    })
}

struct JrcOccurrenceRaster {
    file: File,
    byte_length: u64,
    strip_offsets: Vec<u64>,
    strip_byte_counts: Vec<u64>,
    predictor_code: u16,
}

impl JrcOccurrenceRaster {
    fn open(path: &Path, expected_length: u64) -> Result<Self> {
        let mut decoder = TiffDecoder::new(
            File::open(path).with_context(|| format!("open JRC raster {}", path.display()))?,
        )
        .with_context(|| format!("parse JRC raster {}", path.display()))?;
        let (width, height) = decoder.dimensions()?;
        let bits_per_sample = decoder.get_tag_u16_vec(TiffTag::BitsPerSample)?;
        let compression_code = decoder.get_tag_unsigned::<u16>(TiffTag::Compression)?;
        let predictor_code = decoder
            .get_tag_unsigned::<u16>(TiffTag::Predictor)
            .unwrap_or(1);
        if width != JRC_OCCURRENCE_WIDTH
            || height != JRC_OCCURRENCE_HEIGHT
            || bits_per_sample != [8]
            || compression_code != 5
            || decoder.get_chunk_type() != TiffChunkType::Strip
            || decoder.chunk_dimensions() != (JRC_OCCURRENCE_WIDTH, 1)
        {
            bail!("JRC occurrence raster has an unsupported TIFF packing profile");
        }
        let strip_offsets = decoder.get_tag_u64_vec(TiffTag::StripOffsets)?;
        let strip_byte_counts = decoder.get_tag_u64_vec(TiffTag::StripByteCounts)?;
        if strip_offsets.len() != usize::try_from(JRC_OCCURRENCE_HEIGHT)?
            || strip_byte_counts.len() != strip_offsets.len()
        {
            bail!("JRC occurrence raster has an incomplete strip table");
        }
        drop(decoder);
        Ok(Self {
            file: File::open(path)?,
            byte_length: expected_length,
            strip_offsets,
            strip_byte_counts,
            predictor_code,
        })
    }

    fn decode_row(&mut self, row: u32) -> Result<Vec<u8>> {
        let index = usize::try_from(row)?;
        let offset = *self
            .strip_offsets
            .get(index)
            .context("JRC occurrence row exceeds strip table")?;
        let compressed_length = *self
            .strip_byte_counts
            .get(index)
            .context("JRC occurrence row has no compressed length")?;
        if offset
            .checked_add(compressed_length)
            .is_none_or(|end| end > self.byte_length)
        {
            bail!("JRC occurrence strip lies outside its TIFF");
        }
        self.file.seek(SeekFrom::Start(offset))?;
        let mut compressed = vec![0_u8; usize::try_from(compressed_length)?];
        self.file.read_exact(&mut compressed)?;
        let mut values = LzwDecoder::with_tiff_size_switch(LzwBitOrder::Msb, 8)
            .decode(&compressed)
            .context("decode JRC occurrence LZW row")?;
        if values.len() != usize::try_from(JRC_OCCURRENCE_WIDTH)? {
            bail!("JRC occurrence row decoded to an unexpected width");
        }
        match self.predictor_code {
            1 => {}
            2 => undo_horizontal_u8_predictor(&mut values),
            other => bail!("JRC occurrence raster uses unsupported predictor {other}"),
        }
        Ok(values)
    }
}

struct JrcOccurrenceMosaic {
    artifact_root: PathBuf,
    artifacts: BTreeMap<JrcOccurrenceTileKey, JrcOccurrenceInventoryArtifact>,
    raster_cache_capacity: usize,
    raster_cache: VecDeque<(JrcOccurrenceTileKey, JrcOccurrenceRaster)>,
}

impl JrcOccurrenceMosaic {
    fn new(
        artifact_root: &Path,
        artifacts: BTreeMap<JrcOccurrenceTileKey, JrcOccurrenceInventoryArtifact>,
        raster_cache_capacity: usize,
    ) -> Result<Self> {
        if !(1..=128).contains(&raster_cache_capacity) {
            bail!("JRC source raster cache must contain between 1 and 128 rasters");
        }
        Ok(Self {
            artifact_root: artifact_root.to_owned(),
            artifacts,
            raster_cache_capacity,
            raster_cache: VecDeque::new(),
        })
    }

    fn decode_row(&mut self, key: JrcOccurrenceTileKey, row: u32) -> Result<Vec<u8>> {
        let position = self
            .raster_cache
            .iter()
            .position(|(candidate, _)| *candidate == key);
        let mut cached = if let Some(position) = position {
            self.raster_cache
                .remove(position)
                .context("JRC raster cache position disappeared")?
        } else {
            let artifact = self.artifacts.get(&key).with_context(|| {
                format!("JRC occurrence source tile is absent: {}", key.filename())
            })?;
            let path = self.artifact_root.join(&artifact.artifact_path);
            (
                key,
                JrcOccurrenceRaster::open(&path, artifact.byte_length)
                    .with_context(|| format!("open retained JRC source tile {}", path.display()))?,
            )
        };
        let values = cached.1.decode_row(row)?;
        self.raster_cache.push_back(cached);
        while self.raster_cache.len() > self.raster_cache_capacity {
            self.raster_cache.pop_front();
        }
        Ok(values)
    }
}

/// Resolve the entire target grid by source row, so TIFF/LZW work is proportional
/// to distinct observed rows rather than to S2 container boundaries.
fn sample_global_jrc_occurrence(
    mosaic: &mut JrcOccurrenceMosaic,
    target_s2_level: u8,
) -> Result<HashMap<S2CellId, i64>> {
    let targets = global_s2_cells_at_level(target_s2_level)?;
    let mut values = HashMap::with_capacity(targets.len());
    let mut requests = Vec::with_capacity(targets.len());
    for s2_cell_id in targets {
        let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
            decode_s2_face_ij(s2_cell_id),
        )?)?)?;
        if let Some(address) = jrc_occurrence_sample_address(coordinate) {
            requests.push(JrcOccurrenceSampleRequest {
                address,
                s2_cell_id,
            });
        } else if values.insert(s2_cell_id, -1).is_some() {
            bail!("global JRC target enumeration repeated an S2 cell");
        }
    }
    requests.sort_unstable();
    let mut offset = 0_usize;
    let mut decoded_rows = 0_u64;
    let mut completed_tiles = 0_u64;
    let mut previous_tile = None;
    while offset < requests.len() {
        let first = requests[offset];
        let mut end = offset + 1;
        while end < requests.len()
            && requests[end].address.tile == first.address.tile
            && requests[end].address.row == first.address.row
        {
            end += 1;
        }
        if previous_tile.is_some_and(|tile| tile != first.address.tile) {
            completed_tiles += 1;
            if completed_tiles.is_multiple_of(36) {
                eprintln!(
                    "JRC occurrence grouped sampling progress: {completed_tiles}/504 source tiles"
                );
            }
        }
        previous_tile = Some(first.address.tile);
        let source_row = mosaic.decode_row(first.address.tile, first.address.row)?;
        decoded_rows = decoded_rows
            .checked_add(1)
            .context("JRC decoded source-row count overflow")?;
        for request in &requests[offset..end] {
            let value = i64::from(
                *source_row
                    .get(usize::try_from(request.address.column)?)
                    .context("JRC grouped sample exceeds source row")?,
            );
            if values.insert(request.s2_cell_id, value).is_some() {
                bail!("global JRC sampling repeated an S2 target cell");
            }
        }
        offset = end;
    }
    if previous_tile.is_some() {
        completed_tiles += 1;
    }
    let expected = 6_usize
        .checked_mul(
            4_usize
                .checked_pow(u32::from(target_s2_level))
                .context("global JRC target-count exponent overflow")?,
        )
        .context("global JRC target count overflow")?;
    if completed_tiles != 504 || values.len() != expected {
        bail!(
            "global JRC grouped sampling covered {completed_tiles} source tiles and {} of {expected} targets",
            values.len()
        );
    }
    eprintln!(
        "JRC occurrence grouped sampling complete: {decoded_rows} distinct source rows for {expected} targets"
    );
    Ok(values)
}

#[derive(Debug, Serialize)]
struct JrcOccurrenceLayerDerivation {
    derivation_schema_version: u16,
    status: &'static str,
    source_inventory_digest: Digest,
    source_artifact_set_digest: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: &'static str,
    missing_source_code: i64,
    target_cells: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

fn derive_jrc_surface_water_occurrence_layer(
    source_inventory: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
    source_raster_cache: usize,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!("provisional JRC occurrence layer v1 requires L6 containers and L10 targets");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!("JRC occurrence output directory already exists");
    }
    let source_set = load_verified_jrc_occurrence_source_set(source_inventory, artifact_root)?;
    let source_inventory_digest = source_set.inventory_digest;
    let source_artifact_set_digest = source_set.artifact_set_digest;
    let mut mosaic =
        JrcOccurrenceMosaic::new(artifact_root, source_set.artifacts, source_raster_cache)?;
    let values = sample_global_jrc_occurrence(&mut mosaic, target_s2_level)?;
    drop(mosaic);
    let staging_directory =
        prepare_or_resume_layer_staging_directory(output_directory, "JRC occurrence")?;
    let (root_index_path, root_bytes) = write_packed_jrc_occurrence_layer(
        &staging_directory,
        layer_id,
        source_inventory_digest,
        source_artifact_set_digest,
        container_s2_level,
        target_s2_level,
        &values,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish JRC occurrence directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&JrcOccurrenceLayerDerivation {
            derivation_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            source_inventory_digest,
            source_artifact_set_digest,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: "s2-cell-centre-containing-jrc-pixel-v1",
            missing_source_code: -1,
            target_cells: u64::try_from(global_s2_cells_at_level(target_s2_level)?.len())?,
            output_directory: output_directory.display().to_string(),
            root_index_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

fn write_packed_jrc_occurrence_layer(
    output_directory: &Path,
    layer_id: &str,
    source_inventory_digest: Digest,
    source_artifact_set_digest: Digest,
    container_s2_level: u8,
    target_s2_level: u8,
    values: &HashMap<S2CellId, i64>,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{container_s2_level}");
    let tile_directory = output_directory
        .join("layers")
        .join(layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let containers = global_s2_cells_at_level(container_s2_level)?;
    let mut entries = Vec::with_capacity(containers.len());
    for (position, container) in containers.into_iter().enumerate() {
        let relative_path = format!("layers/{layer_id}/{level_directory}/{container}.tile");
        let artifact_path = output_directory.join(&relative_path);
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_jrc_occurrence_tile(
                    &existing,
                    layer_id,
                    source_inventory_digest,
                    source_artifact_set_digest,
                    container,
                    target_s2_level,
                )?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let tile = pack_jrc_occurrence_tile(
                    layer_id,
                    source_inventory_digest,
                    source_artifact_set_digest,
                    container,
                    target_s2_level,
                    values,
                )?;
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged JRC occurrence tile"),
        };
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 1_024 == 0 {
            eprintln!(
                "JRC occurrence normalization progress: {}/24576 containers",
                position + 1
            );
        }
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged JRC occurrence root differs from requested derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged JRC occurrence root"),
    }
    Ok((root_relative_path, root_bytes))
}

fn validate_resumable_jrc_occurrence_tile(
    bytes: &[u8],
    layer_id: &str,
    source_inventory_digest: Digest,
    source_artifact_set_digest: Digest,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<()> {
    let tile = PackedScalarFieldTile::from_canonical_slice(bytes)
        .context("decode staged JRC occurrence tile")?;
    if tile.layer_id != layer_id
        || tile.unit != "source_code"
        || tile.decimal_places != 0
        || tile.source_snapshot_digest != source_inventory_digest
        || tile.source_artifact_digest != source_artifact_set_digest
        || tile.quadrature_points_per_axis != 1
        || tile.container_s2_cell_id != container_s2_cell_id
        || tile.target_s2_level != target_s2_level
    {
        bail!("staged JRC occurrence tile differs from requested derivation");
    }
    Ok(())
}

fn pack_jrc_occurrence_tile(
    layer_id: &str,
    source_inventory_digest: Digest,
    source_artifact_set_digest: Digest,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
    values: &HashMap<S2CellId, i64>,
) -> Result<PackedScalarFieldTile> {
    let s2_cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?;
    let cells = s2_cells
        .into_iter()
        .map(|s2_cell_id| {
            let value = *values.get(&s2_cell_id).with_context(|| {
                format!("global JRC sampled field is missing target cell {s2_cell_id}")
            })?;
            Ok(ScalarFieldCell {
                s2_cell_id,
                support_samples: 1,
                minimum_value: value,
                mean_value: value,
                maximum_value: value,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedScalarFieldTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        unit: "source_code".to_owned(),
        decimal_places: 0,
        source_snapshot_digest: source_inventory_digest,
        source_artifact_digest: source_artifact_set_digest,
        quadrature_points_per_axis: 1,
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed JRC occurrence source-code tile is invalid")?;
    Ok(tile)
}

#[derive(Debug, Serialize)]
struct JrcOccurrenceLayerInspection {
    inspection_schema_version: u16,
    status: &'static str,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    source_inventory_digest: Digest,
    source_artifact_set_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    missing_source_cells: u64,
    source_code_counts: BTreeMap<i64, u64>,
    tile_byte_length: u64,
}

fn inspect_jrc_surface_water_occurrence_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical JRC occurrence root index")?;
    if root.layer_id != layer_id {
        bail!("JRC occurrence root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("JRC occurrence root does not cover every expected container");
    }
    let mut source_inventory_digest = None;
    let mut source_artifact_set_digest = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    let mut source_code_counts = BTreeMap::new();
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.media_type != PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE
        {
            bail!("JRC occurrence root contains an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("JRC occurrence tile fails its root reference");
        }
        let tile = PackedScalarFieldTile::from_canonical_slice(&bytes)
            .context("decode canonical JRC occurrence tile")?;
        if tile.layer_id != layer_id
            || tile.unit != "source_code"
            || tile.decimal_places != 0
            || tile.quadrature_points_per_axis != 1
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
        {
            bail!("JRC occurrence tile has inconsistent packing metadata");
        }
        match source_inventory_digest {
            Some(expected) if expected != tile.source_snapshot_digest => {
                bail!("JRC occurrence tiles disagree on source inventory digest")
            }
            None => source_inventory_digest = Some(tile.source_snapshot_digest),
            _ => {}
        }
        match source_artifact_set_digest {
            Some(expected) if expected != tile.source_artifact_digest => {
                bail!("JRC occurrence tiles disagree on source artifact-set digest")
            }
            None => source_artifact_set_digest = Some(tile.source_artifact_digest),
            _ => {}
        }
        for cell in &tile.cells {
            if cell.minimum_value != cell.mean_value || cell.mean_value != cell.maximum_value {
                bail!("JRC centre-sampled occurrence cell contains a non-point range");
            }
            if !(-1..=255).contains(&cell.mean_value) {
                bail!("JRC occurrence source code lies outside its retained byte range");
            }
            *source_code_counts.entry(cell.mean_value).or_default() += 1;
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("JRC occurrence tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("JRC occurrence target cell total overflow")?;
    }
    let missing_source_cells = *source_code_counts.get(&-1).unwrap_or(&0);
    println!(
        "{}",
        serde_json::to_string(&JrcOccurrenceLayerInspection {
            inspection_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            source_inventory_digest: source_inventory_digest
                .context("JRC occurrence root contains no tiles")?,
            source_artifact_set_digest: source_artifact_set_digest
                .context("JRC occurrence root contains no tiles")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            missing_source_cells,
            source_code_counts,
            tile_byte_length,
        })?
    );
    Ok(())
}

const DAF_RECORD_BYTES: usize = 1024;

#[derive(Debug, Serialize)]
struct JplDe441Inspection {
    inspection_schema_version: u16,
    release: &'static str,
    files: Vec<DafSpkFileInspection>,
}

#[derive(Debug, Serialize)]
struct DafSpkFileInspection {
    path: String,
    byte_length: u64,
    id_word: String,
    internal_name: String,
    binary_format: String,
    double_components: u32,
    integer_components: u32,
    first_summary_record: u32,
    last_summary_record: u32,
    first_free_double_word_address: u32,
    summary_record_count: usize,
    segment_count: usize,
    segments: Vec<DafSpkSegmentInspection>,
}

#[derive(Debug, Serialize)]
struct DafSpkSegmentInspection {
    name: String,
    start_tdb_seconds_from_j2000: f64,
    end_tdb_seconds_from_j2000: f64,
    start_tdb_ieee754_bits_hex: String,
    end_tdb_ieee754_bits_hex: String,
    target_naif_id: i32,
    center_naif_id: i32,
    reference_frame_naif_id: i32,
    spk_data_type: i32,
    initial_double_word_address: u32,
    final_double_word_address: u32,
}

fn inspect_jpl_de441(input_directory: &Path) -> Result<()> {
    let mut files = Vec::new();
    for filename in ["de441_part-1.bsp", "de441_part-2.bsp"] {
        files.push(inspect_daf_spk_file(&input_directory.join(filename))?);
    }
    println!(
        "{}",
        serde_json::to_string(&JplDe441Inspection {
            inspection_schema_version: 1,
            release: "JPL DE441",
            files,
        })?
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Serialize)]
struct CartesianKilometres {
    x: f64,
    y: f64,
    z: f64,
}

impl CartesianKilometres {
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    fn subtract(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    fn to_fixed_millimetres(self) -> Result<CartesianMillimetres> {
        Ok(CartesianMillimetres::new(
            f64_bits_to_rounded_scaled_integer(self.x.to_bits(), 1_000_000)?,
            f64_bits_to_rounded_scaled_integer(self.y.to_bits(), 1_000_000)?,
            f64_bits_to_rounded_scaled_integer(self.z.to_bits(), 1_000_000)?,
        ))
    }
}

#[derive(Debug, Serialize)]
struct JplDe441EpochInspection {
    inspection_schema_version: u16,
    release: &'static str,
    reference_frame: &'static str,
    coordinate_unit: &'static str,
    tdb_seconds_from_j2000: i64,
    source_files: Vec<String>,
    earth_barycentric: CartesianKilometres,
    sun_geocentric: CartesianKilometres,
    moon_geocentric: CartesianKilometres,
    fixed_scale_boundary: CelestialState,
}

fn inspect_jpl_de441_epoch(input_directory: &Path, tdb_seconds: i64) -> Result<()> {
    let files = ["de441_part-1.bsp", "de441_part-2.bsp"]
        .into_iter()
        .map(|filename| inspect_daf_spk_file(&input_directory.join(filename)))
        .collect::<Result<Vec<_>>>()?;
    let epoch = tdb_seconds as f64;
    let (earth_moon_barycenter, source_3) = evaluate_de441_target(&files, 3, 0, epoch)?;
    let (sun_barycentric, source_10) = evaluate_de441_target(&files, 10, 0, epoch)?;
    let (moon_from_emb, source_301) = evaluate_de441_target(&files, 301, 3, epoch)?;
    let (earth_from_emb, source_399) = evaluate_de441_target(&files, 399, 3, epoch)?;
    let earth_barycentric = earth_moon_barycenter.add(earth_from_emb);
    let moon_barycentric = earth_moon_barycenter.add(moon_from_emb);
    let sun_geocentric = sun_barycentric.subtract(earth_barycentric);
    let moon_geocentric = moon_barycentric.subtract(earth_barycentric);
    let mut source_files = vec![source_3, source_10, source_301, source_399];
    source_files.sort();
    source_files.dedup();
    println!(
        "{}",
        serde_json::to_string(&JplDe441EpochInspection {
            inspection_schema_version: 1,
            release: "JPL DE441",
            reference_frame: "ICRF/J2000 (NAIF frame 1)",
            coordinate_unit: "kilometres",
            tdb_seconds_from_j2000: tdb_seconds,
            source_files,
            earth_barycentric,
            sun_geocentric,
            moon_geocentric,
            fixed_scale_boundary: CelestialState::new(
                TdbSecondsSinceJ2000::new(i128::from(tdb_seconds)),
                sun_geocentric.to_fixed_millimetres()?,
                moon_geocentric.to_fixed_millimetres()?,
            ),
        })?
    );
    Ok(())
}

fn evaluate_de441_target(
    files: &[DafSpkFileInspection],
    target: i32,
    center: i32,
    epoch: f64,
) -> Result<(CartesianKilometres, String)> {
    let selected = files
        .iter()
        .flat_map(|file| {
            file.segments
                .iter()
                .filter(move |segment| {
                    segment.target_naif_id == target
                        && segment.center_naif_id == center
                        && epoch >= segment.start_tdb_seconds_from_j2000
                        && epoch <= segment.end_tdb_seconds_from_j2000
                })
                .map(move |segment| (file, segment))
        })
        .max_by(|(_, left), (_, right)| {
            left.start_tdb_seconds_from_j2000
                .total_cmp(&right.start_tdb_seconds_from_j2000)
        })
        .with_context(|| {
            format!("DE441 has no target {target} relative to {center} at epoch {epoch}")
        })?;
    let vector = evaluate_spk_type_2(Path::new(&selected.0.path), selected.1, epoch)?;
    Ok((vector, selected.0.path.clone()))
}

fn evaluate_spk_type_2(
    path: &Path,
    segment: &DafSpkSegmentInspection,
    epoch: f64,
) -> Result<CartesianKilometres> {
    if segment.spk_data_type != 2 {
        bail!(
            "SPK segment type {} is not supported",
            segment.spk_data_type
        );
    }
    let mut file = File::open(path)?;
    let footer_address = segment
        .final_double_word_address
        .checked_sub(3)
        .context("SPK type 2 segment is too short for its footer")?;
    let footer = read_daf_double_words(&mut file, footer_address, 4)?;
    let initial_epoch = footer[0];
    let interval_length = footer[1];
    let record_size = daf_control_integer(footer[2], "SPK record size")?;
    let record_count = daf_control_integer(footer[3], "SPK record count")?;
    if !initial_epoch.is_finite()
        || !interval_length.is_finite()
        || interval_length <= 0.0
        || record_size < 5
        || record_count == 0
        || (record_size - 2) % 3 != 0
    {
        bail!("SPK type 2 segment footer is invalid");
    }
    let relative = (epoch - initial_epoch) / interval_length;
    if !relative.is_finite() || relative < 0.0 || relative > f64::from(record_count) {
        bail!("epoch lies outside the SPK type 2 record table");
    }
    let record_index = if relative == f64::from(record_count) {
        record_count - 1
    } else {
        relative.floor() as u32
    };
    let record_address = segment
        .initial_double_word_address
        .checked_add(
            record_index
                .checked_mul(record_size)
                .context("SPK record offset overflow")?,
        )
        .context("SPK record address overflow")?;
    let record = read_daf_double_words(&mut file, record_address, record_size)?;
    let midpoint = record[0];
    let radius = record[1];
    if !midpoint.is_finite() || !radius.is_finite() || radius <= 0.0 {
        bail!("SPK type 2 record interval is invalid");
    }
    let normalized_epoch = (epoch - midpoint) / radius;
    if !normalized_epoch.is_finite() || normalized_epoch.abs() > 1.0 + 1e-12 {
        bail!("SPK type 2 record does not contain the requested epoch");
    }
    let coefficient_count = usize::try_from((record_size - 2) / 3)?;
    let coefficients = &record[2..];
    let x = evaluate_chebyshev(&coefficients[..coefficient_count], normalized_epoch)?;
    let y = evaluate_chebyshev(
        &coefficients[coefficient_count..coefficient_count * 2],
        normalized_epoch,
    )?;
    let z = evaluate_chebyshev(
        &coefficients[coefficient_count * 2..coefficient_count * 3],
        normalized_epoch,
    )?;
    Ok(CartesianKilometres { x, y, z })
}

fn read_daf_double_words(file: &mut File, first_address: u32, count: u32) -> Result<Vec<f64>> {
    if first_address == 0 || count == 0 {
        bail!("DAF double-word reads use positive one-based addresses and counts");
    }
    let offset = u64::from(first_address - 1)
        .checked_mul(8)
        .context("DAF double-word offset overflow")?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![
        0_u8;
        usize::try_from(count)?
            .checked_mul(8)
            .context("DAF read overflow")?
    ];
    file.read_exact(&mut bytes)?;
    bytes
        .chunks_exact(8)
        .map(le_f64)
        .collect::<Result<Vec<_>>>()
}

fn evaluate_chebyshev(coefficients: &[f64], x: f64) -> Result<f64> {
    if coefficients.is_empty() || !x.is_finite() {
        bail!("Chebyshev evaluation needs coefficients and a finite coordinate");
    }
    let mut next = 0.0;
    let mut after_next = 0.0;
    for coefficient in coefficients.iter().skip(1).rev() {
        if !coefficient.is_finite() {
            bail!("Chebyshev coefficient is not finite");
        }
        let current = 2.0 * x * next - after_next + coefficient;
        after_next = next;
        next = current;
    }
    if !coefficients[0].is_finite() {
        bail!("Chebyshev coefficient is not finite");
    }
    Ok(x * next - after_next + coefficients[0])
}

fn inspect_daf_spk_file(path: &Path) -> Result<DafSpkFileInspection> {
    let metadata =
        fs::metadata(path).with_context(|| format!("inspect DAF/SPK file {}", path.display()))?;
    if !metadata.is_file() || metadata.len() < DAF_RECORD_BYTES as u64 {
        bail!("DAF/SPK file is missing or too small: {}", path.display());
    }
    let mut file =
        File::open(path).with_context(|| format!("open DAF/SPK file {}", path.display()))?;
    let file_record = read_daf_record(&mut file, 1, metadata.len())?;
    let id_word = parse_fixed_ascii(&file_record[0..8], "DAF ID word")?;
    if id_word != "DAF/SPK" {
        bail!("{} is not a DAF/SPK file", path.display());
    }
    let double_components = u32::try_from(le_i32(&file_record[8..12])?)?;
    let integer_components = u32::try_from(le_i32(&file_record[12..16])?)?;
    if double_components != 2 || integer_components != 6 {
        bail!(
            "DE441 DAF summary shape is ND={double_components}, NI={integer_components}, expected 2/6"
        );
    }
    let internal_name = parse_fixed_ascii(&file_record[16..76], "DAF internal name")?;
    let first_summary_record = u32::try_from(le_i32(&file_record[76..80])?)?;
    let last_summary_record = u32::try_from(le_i32(&file_record[80..84])?)?;
    let first_free_double_word_address = u32::try_from(le_i32(&file_record[84..88])?)?;
    let binary_format = parse_fixed_ascii(&file_record[88..96], "DAF binary format")?;
    if binary_format != "LTL-IEEE" {
        bail!("DE441 DAF binary format {binary_format:?} is unsupported");
    }
    if first_summary_record == 0 || last_summary_record == 0 || first_free_double_word_address == 0
    {
        bail!("DE441 DAF file record has a zero directory pointer");
    }

    let summary_double_words = double_components + integer_components.div_ceil(2);
    let summary_bytes = usize::try_from(summary_double_words)?
        .checked_mul(8)
        .context("DAF summary byte length overflow")?;
    let name_bytes = summary_bytes;
    let maximum_summaries = (DAF_RECORD_BYTES - 24) / summary_bytes;
    let mut summary_record = first_summary_record;
    let mut previous_summary_record = 0_u32;
    let mut visited = HashSet::new();
    let mut summary_record_count = 0_usize;
    let mut segments = Vec::new();

    loop {
        if !visited.insert(summary_record) {
            bail!("DE441 DAF summary record chain contains a cycle");
        }
        let summary = read_daf_record(&mut file, summary_record, metadata.len())?;
        let next = daf_control_integer(le_f64(&summary[0..8])?, "next summary record")?;
        let previous = daf_control_integer(le_f64(&summary[8..16])?, "previous summary record")?;
        let summary_count = daf_control_integer(le_f64(&summary[16..24])?, "summary count")?;
        if previous != previous_summary_record
            || usize::try_from(summary_count)? > maximum_summaries
        {
            bail!("DE441 DAF summary record links or count are invalid");
        }
        let names = read_daf_record(
            &mut file,
            summary_record
                .checked_add(1)
                .context("DAF name-record number overflow")?,
            metadata.len(),
        )?;
        for index in 0..usize::try_from(summary_count)? {
            let offset = 24 + index * summary_bytes;
            let packed = &summary[offset..offset + summary_bytes];
            let start = le_f64(&packed[0..8])?;
            let end = le_f64(&packed[8..16])?;
            if !start.is_finite() || !end.is_finite() || start >= end {
                bail!("DE441 DAF segment has an invalid epoch interval");
            }
            let target_naif_id = le_i32(&packed[16..20])?;
            let center_naif_id = le_i32(&packed[20..24])?;
            let reference_frame_naif_id = le_i32(&packed[24..28])?;
            let spk_data_type = le_i32(&packed[28..32])?;
            let initial_double_word_address = u32::try_from(le_i32(&packed[32..36])?)?;
            let final_double_word_address = u32::try_from(le_i32(&packed[36..40])?)?;
            if initial_double_word_address == 0
                || initial_double_word_address > final_double_word_address
                || final_double_word_address >= first_free_double_word_address
                || u64::from(final_double_word_address)
                    .checked_mul(8)
                    .is_none_or(|bytes| bytes > metadata.len())
            {
                bail!("DE441 DAF segment address range is invalid");
            }
            let name_offset = index * name_bytes;
            let name = parse_fixed_ascii(
                &names[name_offset..name_offset + name_bytes],
                "DAF segment name",
            )?;
            if name.is_empty() {
                bail!("DE441 DAF segment name is empty");
            }
            segments.push(DafSpkSegmentInspection {
                name,
                start_tdb_seconds_from_j2000: start,
                end_tdb_seconds_from_j2000: end,
                start_tdb_ieee754_bits_hex: format!("{:016x}", start.to_bits()),
                end_tdb_ieee754_bits_hex: format!("{:016x}", end.to_bits()),
                target_naif_id,
                center_naif_id,
                reference_frame_naif_id,
                spk_data_type,
                initial_double_word_address,
                final_double_word_address,
            });
        }
        summary_record_count += 1;
        if next == 0 {
            if summary_record != last_summary_record {
                bail!("DE441 DAF last-summary pointer disagrees with the record chain");
            }
            break;
        }
        previous_summary_record = summary_record;
        summary_record = next;
    }
    if segments.is_empty() {
        bail!("DE441 DAF file contains no SPK segments");
    }
    Ok(DafSpkFileInspection {
        path: path.display().to_string(),
        byte_length: metadata.len(),
        id_word,
        internal_name,
        binary_format,
        double_components,
        integer_components,
        first_summary_record,
        last_summary_record,
        first_free_double_word_address,
        summary_record_count,
        segment_count: segments.len(),
        segments,
    })
}

fn read_daf_record(file: &mut File, record_number: u32, file_length: u64) -> Result<[u8; 1024]> {
    if record_number == 0 {
        bail!("DAF record numbers are one-based");
    }
    let offset = u64::from(record_number - 1)
        .checked_mul(DAF_RECORD_BYTES as u64)
        .context("DAF record offset overflow")?;
    if offset
        .checked_add(DAF_RECORD_BYTES as u64)
        .is_none_or(|end| end > file_length)
    {
        bail!("DAF record {record_number} lies outside the file");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut record = [0_u8; DAF_RECORD_BYTES];
    file.read_exact(&mut record)?;
    Ok(record)
}

fn parse_fixed_ascii(bytes: &[u8], field: &str) -> Result<String> {
    if !bytes.iter().all(u8::is_ascii) {
        bail!("{field} is not ASCII");
    }
    Ok(String::from_utf8(bytes.to_vec())?
        .trim_matches(['\0', ' '])
        .to_owned())
}

fn daf_control_integer(value: f64, field: &str) -> Result<u32> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        bail!("DAF {field} is not a nonnegative exact u32");
    }
    Ok(value as u32)
}

fn le_i32(bytes: &[u8]) -> Result<i32> {
    Ok(i32::from_le_bytes(bytes.try_into()?))
}

fn le_f64(bytes: &[u8]) -> Result<f64> {
    Ok(f64::from_bits(u64::from_le_bytes(bytes.try_into()?)))
}

const SOILGRIDS_TOPSOIL_PROPERTIES: [&str; 9] = [
    "bdod", "cec", "cfvo", "clay", "nitrogen", "phh2o", "sand", "silt", "soc",
];
const SOILGRIDS_TOPSOIL_QUANTILES: [&str; 3] = ["Q0.05", "Q0.5", "Q0.95"];

#[derive(Debug, Serialize)]
struct SoilgridsTopsoilInspection {
    inspection_schema_version: u16,
    source: &'static str,
    release: &'static str,
    depth: &'static str,
    artifact_count: usize,
    byte_length: u64,
    artifacts: Vec<SoilgridsArtifactInspection>,
}

#[derive(Debug, Serialize)]
struct SoilgridsArtifactInspection {
    property: String,
    quantile: String,
    vrt_path: String,
    vrt_byte_length: u64,
    vrt_hash: Digest,
    raster_width: u32,
    raster_height: u32,
    spatial_reference: String,
    geotransform_ieee754_bits_hex: Vec<String>,
    source_tile_count: usize,
    overview_path: String,
    overview_byte_length: u64,
    overview_hash: Digest,
    image_directories: Vec<SoilgridsImageDirectoryInspection>,
}

#[derive(Debug)]
struct SoilgridsVrtGeometry {
    raster_width: u32,
    raster_height: u32,
    spatial_reference: String,
    geotransform: [f64; 6],
    source_tile_count: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct SoilgridsImageDirectoryProfile {
    width: u32,
    height: u32,
    color_type: String,
    chunk_type: String,
    chunk_width: u32,
    chunk_height: u32,
    chunk_count: u32,
}

#[derive(Debug, Serialize)]
struct SoilgridsImageDirectoryInspection {
    image_index: usize,
    #[serde(flatten)]
    profile: SoilgridsImageDirectoryProfile,
    sampled_chunks: Vec<SoilgridsChunkInspection>,
}

#[derive(Debug, Serialize)]
struct SoilgridsChunkInspection {
    chunk_index: u32,
    data_width: u32,
    data_height: u32,
    sample_type: &'static str,
    sample_count: usize,
    finite_minimum: Option<String>,
    finite_maximum: Option<String>,
    non_finite_samples: usize,
}

fn text_between<'a>(text: &'a str, start: &str, end: &str, field: &str) -> Result<&'a str> {
    let remainder = text
        .split_once(start)
        .map(|(_, remainder)| remainder)
        .with_context(|| format!("SoilGrids VRT is missing {field}"))?;
    remainder
        .split_once(end)
        .map(|(value, _)| value)
        .with_context(|| format!("SoilGrids VRT has an unterminated {field}"))
}

fn parse_soilgrids_vrt_geometry(path: &Path) -> Result<SoilgridsVrtGeometry> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read SoilGrids VRT geometry {}", path.display()))?;
    let header = text.lines().next().context("SoilGrids VRT is empty")?;
    let parse_attribute = |name: &str| -> Result<u32> {
        let marker = format!("{name}=\"");
        let value = header
            .split_once(&marker)
            .map(|(_, remainder)| remainder)
            .and_then(|remainder| remainder.split_once('"').map(|(value, _)| value))
            .with_context(|| format!("SoilGrids VRT header is missing {name}"))?;
        value
            .parse::<u32>()
            .with_context(|| format!("SoilGrids VRT {name} is not u32"))
    };
    let raster_width = parse_attribute("rasterXSize")?;
    let raster_height = parse_attribute("rasterYSize")?;
    let spatial_reference = text_between(&text, "<SRS>", "</SRS>", "SRS")?.to_owned();
    if !spatial_reference.contains("Interrupted_Goode_Homolosine")
        || !spatial_reference.contains("WGS_1984")
        || !spatial_reference.contains("UNIT[\"Meter\",1]")
    {
        bail!("SoilGrids VRT does not declare the expected WGS84 Interrupted Goode Homolosine CRS");
    }
    let geotransform_values =
        text_between(&text, "<GeoTransform>", "</GeoTransform>", "GeoTransform")?
            .split(',')
            .map(|value| {
                value
                    .trim()
                    .parse::<f64>()
                    .context("SoilGrids GeoTransform value is not binary64")
            })
            .collect::<Result<Vec<_>>>()?;
    let geotransform: [f64; 6] = geotransform_values
        .try_into()
        .map_err(|_| anyhow::anyhow!("SoilGrids GeoTransform must contain six values"))?;
    if !matches!(geotransform[0], -19_949_000.0 | -19_949_750.0)
        || geotransform[1..] != [250.0, 0.0, 8_361_000.0, 0.0, -250.0]
    {
        bail!("SoilGrids VRT GeoTransform differs from the retained global grid contract");
    }
    if !text.contains("<NoDataValue>-32768</NoDataValue>") {
        bail!("SoilGrids VRT does not retain the signed no-data sentinel");
    }
    let source_tile_count = text.matches("<ComplexSource>").count();
    if source_tile_count == 0 {
        bail!("SoilGrids VRT contains no source mosaic tiles");
    }
    Ok(SoilgridsVrtGeometry {
        raster_width,
        raster_height,
        spatial_reference,
        geotransform,
        source_tile_count,
    })
}

#[derive(Clone, Copy, Debug)]
struct SoilgridsProjectedCell {
    s2_cell_id: S2CellId,
    easting_e12: i128,
    northing_e12: i128,
}

struct SoilgridsProjection {
    proj_version: String,
    cells: Vec<SoilgridsProjectedCell>,
}

fn write_degrees_e7(output: &mut impl Write, value: i32) -> Result<()> {
    let negative = value < 0;
    let magnitude = i64::from(value).unsigned_abs();
    if negative {
        output.write_all(b"-")?;
    }
    write!(
        output,
        "{}.{:07}",
        magnitude / 10_000_000,
        magnitude % 10_000_000
    )?;
    Ok(())
}

fn parse_fixed_decimal(value: &str, decimal_places: u32) -> Result<i128> {
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > usize::try_from(decimal_places)?
    {
        bail!("projected coordinate is not a fixed decimal: {value:?}");
    }
    let scale = 10_i128
        .checked_pow(decimal_places)
        .context("projected coordinate scale overflow")?;
    let whole = whole
        .parse::<i128>()
        .context("projected coordinate whole part overflow")?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .context("projected coordinate fraction overflow")?
            .checked_mul(
                10_i128
                    .checked_pow(decimal_places - u32::try_from(fraction.len())?)
                    .context("projected coordinate padding overflow")?,
            )
            .context("projected coordinate fraction scaling overflow")?
    };
    let magnitude = whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction))
        .context("projected coordinate overflow")?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// Project every L10 centre through the standard PROJ `igh` operation in one
/// streaming child process. This is explicitly a provisional offline adapter; the
/// output records the installed PROJ version and final admission will cross-check it
/// against an independent implementation.
fn project_global_s2_centres_to_soilgrids(target_s2_level: u8) -> Result<SoilgridsProjection> {
    if target_s2_level != 10 {
        bail!("provisional SoilGrids projection currently requires L10 targets");
    }
    let version_output = ProcessCommand::new("pkg-config")
        .args(["--modversion", "proj"])
        .output()
        .context("query installed PROJ version through pkg-config")?;
    if !version_output.status.success() {
        bail!("pkg-config could not identify the installed PROJ version");
    }
    let proj_version = String::from_utf8(version_output.stdout)?.trim().to_owned();
    if proj_version.is_empty() {
        bail!("installed PROJ version is empty");
    }

    let targets = global_s2_cells_at_level(target_s2_level)?;
    let mut input = BufWriter::new(Vec::with_capacity(targets.len().saturating_mul(32)));
    for target in &targets {
        let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
            decode_s2_face_ij(*target),
        )?)?)?;
        write_degrees_e7(&mut input, coordinate.longitude_e7())?;
        input.write_all(b" ")?;
        write_degrees_e7(&mut input, coordinate.latitude_e7())?;
        input.write_all(b"\n")?;
    }
    input.flush()?;
    let input = input
        .into_inner()
        .map_err(|error| anyhow::anyhow!("finish PROJ input buffer: {error}"))?;

    let mut child = ProcessCommand::new("cs2cs")
        .args([
            "-f",
            "%.12f",
            "+proj=longlat",
            "+datum=WGS84",
            "+to",
            "+proj=igh",
            "+ellps=WGS84",
            "+units=m",
            "+no_defs",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start PROJ cs2cs SoilGrids transform")?;
    let mut child_stdin = child.stdin.take().context("cs2cs stdin is unavailable")?;
    let writer = thread::spawn(move || -> std::io::Result<()> {
        child_stdin.write_all(&input)?;
        child_stdin.flush()
    });
    let child_stdout = child.stdout.take().context("cs2cs stdout is unavailable")?;
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();
    let mut cells = Vec::with_capacity(targets.len());
    let read_result = (|| -> Result<()> {
        for target in targets {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                bail!("cs2cs ended before every S2 target was projected");
            }
            let mut fields = line.split_whitespace();
            let x = parse_fixed_decimal(fields.next().context("cs2cs output has no easting")?, 12)?;
            let y =
                parse_fixed_decimal(fields.next().context("cs2cs output has no northing")?, 12)?;
            cells.push(SoilgridsProjectedCell {
                s2_cell_id: target,
                easting_e12: x,
                northing_e12: y,
            });
        }
        line.clear();
        while reader.read_line(&mut line)? != 0 {
            if !line.trim().is_empty() {
                bail!("cs2cs emitted more coordinates than requested");
            }
            line.clear();
        }
        Ok(())
    })();
    if read_result.is_err() {
        let _ = child.kill();
    }
    let writer_result = writer
        .join()
        .map_err(|_| anyhow::anyhow!("PROJ input writer thread panicked"))?;
    let status = child.wait().context("wait for PROJ cs2cs transform")?;
    read_result?;
    writer_result.context("write all coordinates to PROJ cs2cs")?;
    if !status.success() {
        bail!("PROJ cs2cs SoilGrids transform exited unsuccessfully");
    }
    Ok(SoilgridsProjection {
        proj_version,
        cells,
    })
}

fn inspect_soilgrids_topsoil(input_directory: &Path) -> Result<()> {
    let mut artifacts =
        Vec::with_capacity(SOILGRIDS_TOPSOIL_PROPERTIES.len() * SOILGRIDS_TOPSOIL_QUANTILES.len());
    let mut byte_length = 0_u64;

    for property in SOILGRIDS_TOPSOIL_PROPERTIES {
        for quantile in SOILGRIDS_TOPSOIL_QUANTILES {
            let vrt_filename = format!("{property}_0-5cm_{quantile}.vrt");
            let overview_filename = format!("{vrt_filename}.ovr");
            let vrt_path = input_directory.join(property).join(&vrt_filename);
            let overview_path = input_directory.join(property).join(&overview_filename);
            let vrt_geometry = parse_soilgrids_vrt_geometry(&vrt_path)?;
            let (vrt_byte_length, vrt_hash) = digest_file(&vrt_path)?;
            byte_length = byte_length
                .checked_add(vrt_byte_length)
                .context("SoilGrids retained byte length overflow")?;
            let metadata = fs::metadata(&overview_path).with_context(|| {
                format!(
                    "inspect retained SoilGrids artifact {}",
                    overview_path.display()
                )
            })?;
            if !metadata.is_file() || metadata.len() == 0 {
                bail!(
                    "SoilGrids artifact is not a nonempty regular file: {}",
                    overview_path.display()
                );
            }
            byte_length = byte_length
                .checked_add(metadata.len())
                .context("SoilGrids retained byte length overflow")?;
            let (overview_byte_length, overview_hash) = digest_file(&overview_path)?;
            if overview_byte_length != metadata.len() {
                bail!("SoilGrids overview changed while it was inspected");
            }
            let file = File::open(&overview_path).with_context(|| {
                format!(
                    "open retained SoilGrids artifact {}",
                    overview_path.display()
                )
            })?;
            let mut decoder = TiffDecoder::new(file).with_context(|| {
                format!("parse SoilGrids BigTIFF header {}", overview_path.display())
            })?;
            let mut image_directories = Vec::new();

            loop {
                let image_index = image_directories.len();
                let (width, height) = decoder.dimensions().with_context(|| {
                    format!(
                        "read SoilGrids image {image_index} dimensions in {}",
                        overview_path.display()
                    )
                })?;
                let color_type = format!(
                    "{:?}",
                    decoder.colortype().with_context(|| {
                        format!(
                            "read SoilGrids image {image_index} color type in {}",
                            overview_path.display()
                        )
                    })?
                );
                let chunk_kind = decoder.get_chunk_type();
                let chunk_count = match chunk_kind {
                    TiffChunkType::Strip => decoder.strip_count(),
                    TiffChunkType::Tile => decoder.tile_count(),
                }
                .with_context(|| {
                    format!(
                        "read SoilGrids image {image_index} chunk count in {}",
                        overview_path.display()
                    )
                })?;
                if chunk_count == 0 {
                    bail!(
                        "SoilGrids image {image_index} has no chunks: {}",
                        overview_path.display()
                    );
                }
                let (chunk_width, chunk_height) = decoder.chunk_dimensions();
                let profile = SoilgridsImageDirectoryProfile {
                    width,
                    height,
                    color_type,
                    chunk_type: match chunk_kind {
                        TiffChunkType::Strip => "strip",
                        TiffChunkType::Tile => "tile",
                    }
                    .to_owned(),
                    chunk_width,
                    chunk_height,
                    chunk_count,
                };

                let mut sample_indices = vec![0, chunk_count / 2, chunk_count - 1];
                sample_indices.sort_unstable();
                sample_indices.dedup();
                let mut sampled_chunks = Vec::with_capacity(sample_indices.len());
                for chunk_index in sample_indices {
                    let (data_width, data_height) = decoder.chunk_data_dimensions(chunk_index);
                    let decoded = decoder.read_chunk(chunk_index).with_context(|| {
                        format!(
                            "decode SoilGrids image {image_index} chunk {chunk_index} in {}",
                            overview_path.display()
                        )
                    })?;
                    sampled_chunks.push(inspect_soilgrids_chunk(
                        chunk_index,
                        data_width,
                        data_height,
                        &decoded,
                    ));
                }
                image_directories.push(SoilgridsImageDirectoryInspection {
                    image_index,
                    profile,
                    sampled_chunks,
                });
                if !decoder.more_images() {
                    break;
                }
                decoder.next_image().with_context(|| {
                    format!(
                        "advance SoilGrids image directory in {}",
                        overview_path.display()
                    )
                })?;
            }

            if !image_directories.windows(2).all(|images| {
                images[1].profile.width <= images[0].profile.width
                    && images[1].profile.height <= images[0].profile.height
            }) {
                bail!(
                    "SoilGrids overview dimensions do not descend monotonically: {}",
                    overview_path.display()
                );
            }

            let first_overview = image_directories
                .first()
                .context("SoilGrids overview has no image directories")?;
            if first_overview.profile.width != vrt_geometry.raster_width.div_ceil(4)
                || first_overview.profile.height != vrt_geometry.raster_height.div_ceil(4)
            {
                bail!("SoilGrids first overview is not the expected 4x source reduction");
            }

            artifacts.push(SoilgridsArtifactInspection {
                property: property.to_owned(),
                quantile: quantile.to_owned(),
                vrt_path: format!("{property}/{vrt_filename}"),
                vrt_byte_length,
                vrt_hash,
                raster_width: vrt_geometry.raster_width,
                raster_height: vrt_geometry.raster_height,
                spatial_reference: vrt_geometry.spatial_reference,
                geotransform_ieee754_bits_hex: vrt_geometry
                    .geotransform
                    .map(|value| format!("{:016x}", value.to_bits()))
                    .to_vec(),
                source_tile_count: vrt_geometry.source_tile_count,
                overview_path: format!("{property}/{overview_filename}"),
                overview_byte_length,
                overview_hash,
                image_directories,
            });
        }
    }

    println!(
        "{}",
        serde_json::to_string(&SoilgridsTopsoilInspection {
            inspection_schema_version: 1,
            source: "ISRIC SoilGrids 2.0 official global VRT overview pyramids",
            release: "latest (retained source bytes)",
            depth: "0-5cm",
            artifact_count: artifacts.len() * 2,
            byte_length,
            artifacts,
        })?
    );
    Ok(())
}

fn inspect_soilgrids_chunk(
    chunk_index: u32,
    data_width: u32,
    data_height: u32,
    decoded: &TiffDecodingResult,
) -> SoilgridsChunkInspection {
    macro_rules! integer_summary {
        ($values:expr, $sample_type:literal) => {{
            let minimum = $values.iter().min().map(ToString::to_string);
            let maximum = $values.iter().max().map(ToString::to_string);
            ($sample_type, $values.len(), minimum, maximum, 0)
        }};
    }
    macro_rules! float_summary {
        ($values:expr, $sample_type:literal, $convert:expr) => {{
            let mut minimum: Option<f64> = None;
            let mut maximum: Option<f64> = None;
            let mut non_finite = 0_usize;
            for value in $values {
                let value: f64 = $convert(value);
                if value.is_finite() {
                    minimum = Some(minimum.map_or(value, |current| current.min(value)));
                    maximum = Some(maximum.map_or(value, |current| current.max(value)));
                } else {
                    non_finite += 1;
                }
            }
            (
                $sample_type,
                $values.len(),
                minimum.map(|value| value.to_string()),
                maximum.map(|value| value.to_string()),
                non_finite,
            )
        }};
    }

    let (sample_type, sample_count, finite_minimum, finite_maximum, non_finite_samples) =
        match decoded {
            TiffDecodingResult::U8(values) => integer_summary!(values, "u8"),
            TiffDecodingResult::U16(values) => integer_summary!(values, "u16"),
            TiffDecodingResult::U32(values) => integer_summary!(values, "u32"),
            TiffDecodingResult::U64(values) => integer_summary!(values, "u64"),
            TiffDecodingResult::I8(values) => integer_summary!(values, "i8"),
            TiffDecodingResult::I16(values) => integer_summary!(values, "i16"),
            TiffDecodingResult::I32(values) => integer_summary!(values, "i32"),
            TiffDecodingResult::I64(values) => integer_summary!(values, "i64"),
            TiffDecodingResult::F16(values) => {
                let expanded = values
                    .iter()
                    .map(|value| f64::from(f32::from(*value)))
                    .collect::<Vec<_>>();
                float_summary!(&expanded, "f16", |value: &f64| *value)
            }
            TiffDecodingResult::F32(values) => {
                float_summary!(values, "f32", |value: &f32| f64::from(*value))
            }
            TiffDecodingResult::F64(values) => float_summary!(values, "f64", |value: &f64| *value),
        };
    SoilgridsChunkInspection {
        chunk_index,
        data_width,
        data_height,
        sample_type,
        sample_count,
        finite_minimum,
        finite_maximum,
        non_finite_samples,
    }
}

const SOILGRIDS_INVENTORY_SCHEMA_VERSION: u16 = 1;
const SOILGRIDS_RELEASE: &str = "latest";
const SOILGRIDS_DEPTH: &str = "0-5cm";
const SOILGRIDS_OVERVIEW_REDUCTION: u32 = 4;
const SOILGRIDS_OVERVIEW_CHUNK_WIDTH: u32 = 128;
const SOILGRIDS_OVERVIEW_CHUNK_HEIGHT: u32 = 128;
const SOILGRIDS_SAMPLING_REPROJECTION_METHOD: &str =
    "s2-cell-centre-proj-igh-nearest-native-overview-grid-v2";

#[derive(Debug, Deserialize)]
struct SoilgridsInventory {
    inventory_schema_version: u16,
    release: String,
    depth: String,
    artifact_count: usize,
    byte_length: u64,
    artifacts: Vec<SoilgridsInventoryArtifact>,
}

#[derive(Clone, Debug, Deserialize)]
struct SoilgridsInventoryArtifact {
    artifact_path: String,
    byte_length: u64,
    content_hash: Digest,
    download_url: String,
    role: String,
}

#[derive(Clone, Debug)]
struct SoilgridsSourceRasterArtifact {
    artifact_path: String,
    byte_length: u64,
    width: u32,
    height: u32,
    grid: SoilgridsOverviewGrid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SoilgridsOverviewGrid {
    west_e12: i128,
    north_e12: i128,
    pixel_size_e12: i128,
    width: u32,
    height: u32,
}

struct SoilgridsSourceSet {
    inventory_digest: Digest,
    property_sources: Vec<SoilGridsPropertySource>,
    rasters: Vec<SoilgridsSourceRasterArtifact>,
}

fn soilgrids_property(index: usize) -> Result<SoilGridsProperty> {
    [
        SoilGridsProperty::Bdod,
        SoilGridsProperty::Cec,
        SoilGridsProperty::Cfvo,
        SoilGridsProperty::Clay,
        SoilGridsProperty::Nitrogen,
        SoilGridsProperty::Phh2o,
        SoilGridsProperty::Sand,
        SoilGridsProperty::Silt,
        SoilGridsProperty::Soc,
    ]
    .get(index)
    .copied()
    .context("SoilGrids property index exceeds its canonical schema")
}

fn validate_soilgrids_inventory_artifact(
    artifact: &SoilgridsInventoryArtifact,
    expected_path: &str,
    expected_url: &str,
    expected_role: &str,
) -> Result<()> {
    if artifact.artifact_path != expected_path
        || artifact.download_url != expected_url
        || artifact.role != expected_role
        || artifact.byte_length == 0
        || artifact.content_hash == Digest::ZERO
        || artifact
            .artifact_path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        bail!("SoilGrids inventory artifact does not match its expected release identity");
    }
    Ok(())
}

fn load_verified_soilgrids_source_set(
    inventory_path: &Path,
    artifact_root: &Path,
) -> Result<SoilgridsSourceSet> {
    let inventory_bytes = fs::read(inventory_path).with_context(|| {
        format!(
            "read SoilGrids source inventory {}",
            inventory_path.display()
        )
    })?;
    let inventory: SoilgridsInventory =
        serde_json::from_slice(&inventory_bytes).with_context(|| {
            format!(
                "parse SoilGrids source inventory {}",
                inventory_path.display()
            )
        })?;
    let expected_count = SOILGRIDS_TOPSOIL_PROPERTIES
        .len()
        .checked_mul(SOILGRIDS_TOPSOIL_QUANTILES.len())
        .and_then(|count| count.checked_mul(2))
        .context("SoilGrids expected artifact count overflow")?;
    if inventory.inventory_schema_version != SOILGRIDS_INVENTORY_SCHEMA_VERSION
        || inventory.release != SOILGRIDS_RELEASE
        || inventory.depth != SOILGRIDS_DEPTH
        || inventory.artifact_count != expected_count
        || inventory.artifacts.len() != expected_count
    {
        bail!("SoilGrids inventory has an unsupported or incomplete release contract");
    }

    let mut seen_paths = HashSet::with_capacity(expected_count);
    let mut actual_total = 0_u64;
    let mut rasters = Vec::with_capacity(expected_count / 2);
    let mut property_sources = Vec::with_capacity(SOILGRIDS_TOPSOIL_PROPERTIES.len());
    for (property_index, property_name) in SOILGRIDS_TOPSOIL_PROPERTIES.iter().enumerate() {
        let mut quantile_artifact_digests = [Digest::ZERO; 3];
        for (quantile_index, quantile) in SOILGRIDS_TOPSOIL_QUANTILES.iter().enumerate() {
            let vrt_filename = format!("{property_name}_0-5cm_{quantile}.vrt");
            let overview_filename = format!("{vrt_filename}.ovr");
            let vrt_relative_path =
                format!("soilgrids-2-0-topsoil-overviews/{property_name}/{vrt_filename}");
            let overview_relative_path =
                format!("soilgrids-2-0-topsoil-overviews/{property_name}/{overview_filename}");
            let base_url = format!("https://files.isric.org/soilgrids/latest/data/{property_name}");
            let vrt_artifact = inventory
                .artifacts
                .iter()
                .find(|artifact| artifact.artifact_path == vrt_relative_path)
                .with_context(|| format!("SoilGrids inventory is missing {vrt_relative_path}"))?;
            let overview_artifact = inventory
                .artifacts
                .iter()
                .find(|artifact| artifact.artifact_path == overview_relative_path)
                .with_context(|| {
                    format!("SoilGrids inventory is missing {overview_relative_path}")
                })?;
            validate_soilgrids_inventory_artifact(
                vrt_artifact,
                &vrt_relative_path,
                &format!("{base_url}/{vrt_filename}"),
                "geometry",
            )?;
            validate_soilgrids_inventory_artifact(
                overview_artifact,
                &overview_relative_path,
                &format!("{base_url}/{overview_filename}"),
                "data",
            )?;
            for artifact in [vrt_artifact, overview_artifact] {
                if !seen_paths.insert(artifact.artifact_path.clone()) {
                    bail!("SoilGrids inventory repeats an artifact path");
                }
                let path = artifact_root.join(&artifact.artifact_path);
                let (actual_length, actual_hash) = digest_file(&path).with_context(|| {
                    format!("verify retained SoilGrids artifact {}", path.display())
                })?;
                if actual_length != artifact.byte_length || actual_hash != artifact.content_hash {
                    bail!(
                        "retained SoilGrids artifact differs from its inventory: {}",
                        path.display()
                    );
                }
                actual_total = actual_total
                    .checked_add(actual_length)
                    .context("SoilGrids source byte total overflow")?;
            }

            let vrt_geometry =
                parse_soilgrids_vrt_geometry(&artifact_root.join(&vrt_relative_path))?;
            let expected_width = vrt_geometry
                .raster_width
                .div_ceil(SOILGRIDS_OVERVIEW_REDUCTION);
            let expected_height = vrt_geometry
                .raster_height
                .div_ceil(SOILGRIDS_OVERVIEW_REDUCTION);
            let overview_path = artifact_root.join(&overview_relative_path);
            let mut decoder = TiffDecoder::new(File::open(&overview_path)?)
                .with_context(|| format!("parse SoilGrids overview {}", overview_path.display()))?;
            let dimensions = decoder.dimensions()?;
            if dimensions != (expected_width, expected_height)
                || decoder.colortype()? != tiff::ColorType::Gray(16)
                || decoder.get_chunk_type() != TiffChunkType::Tile
                || decoder.chunk_dimensions()
                    != (
                        SOILGRIDS_OVERVIEW_CHUNK_WIDTH,
                        SOILGRIDS_OVERVIEW_CHUNK_HEIGHT,
                    )
                || decoder.tile_count()?
                    != expected_width.div_ceil(SOILGRIDS_OVERVIEW_CHUNK_WIDTH)
                        * expected_height.div_ceil(SOILGRIDS_OVERVIEW_CHUNK_HEIGHT)
            {
                bail!("SoilGrids overview has an unsupported first-image packing profile");
            }
            quantile_artifact_digests[quantile_index] = overview_artifact.content_hash;
            let coordinate_scale = 1_000_000_000_000_i128;
            let west_meters = geotransform_integer(vrt_geometry.geotransform[0], "west origin")?;
            let north_meters = geotransform_integer(vrt_geometry.geotransform[3], "north origin")?;
            let source_pixel_meters =
                geotransform_integer(vrt_geometry.geotransform[1], "pixel size")?;
            let overview_pixel_meters = source_pixel_meters
                .checked_mul(i128::from(SOILGRIDS_OVERVIEW_REDUCTION))
                .context("SoilGrids overview pixel size overflow")?;
            rasters.push(SoilgridsSourceRasterArtifact {
                artifact_path: overview_relative_path,
                byte_length: overview_artifact.byte_length,
                width: expected_width,
                height: expected_height,
                grid: SoilgridsOverviewGrid {
                    west_e12: west_meters
                        .checked_mul(coordinate_scale)
                        .context("SoilGrids west origin scaling overflow")?,
                    north_e12: north_meters
                        .checked_mul(coordinate_scale)
                        .context("SoilGrids north origin scaling overflow")?,
                    pixel_size_e12: overview_pixel_meters
                        .checked_mul(coordinate_scale)
                        .context("SoilGrids pixel size scaling overflow")?,
                    width: expected_width,
                    height: expected_height,
                },
            });
        }
        property_sources.push(SoilGridsPropertySource {
            property: soilgrids_property(property_index)?,
            quantile_artifact_digests,
        });
    }
    if seen_paths.len() != expected_count || actual_total != inventory.byte_length {
        bail!("SoilGrids inventory byte total or artifact membership is inconsistent");
    }
    Ok(SoilgridsSourceSet {
        inventory_digest: Digest::sha256(&inventory_bytes),
        property_sources,
        rasters,
    })
}

fn geotransform_integer(value: f64, field: &str) -> Result<i128> {
    if !value.is_finite() || value.fract() != 0.0 {
        bail!("SoilGrids {field} is not an exact integer-metre coordinate");
    }
    let integer = value as i128;
    if integer as f64 != value {
        bail!("SoilGrids {field} exceeds exact integer conversion");
    }
    Ok(integer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SoilgridsChunkSampleRequest {
    target_index: u32,
    row: u32,
    column: u32,
}

struct SoilgridsChunkGroups {
    requests: Vec<Vec<SoilgridsChunkSampleRequest>>,
}

fn group_soilgrids_projection_by_source_chunk(
    projection: &[SoilgridsProjectedCell],
    grid: SoilgridsOverviewGrid,
) -> Result<SoilgridsChunkGroups> {
    if grid.width == 0 || grid.height == 0 || grid.pixel_size_e12 <= 0 {
        bail!("SoilGrids source grid must be nonempty");
    }
    let chunks_across = grid.width.div_ceil(SOILGRIDS_OVERVIEW_CHUNK_WIDTH);
    let chunks_down = grid.height.div_ceil(SOILGRIDS_OVERVIEW_CHUNK_HEIGHT);
    let chunk_count = chunks_across
        .checked_mul(chunks_down)
        .context("SoilGrids chunk-group count overflow")?;
    let mut requests = (0..chunk_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (target_index, cell) in projection.iter().enumerate() {
        let Ok(row) =
            u32::try_from((grid.north_e12 - cell.northing_e12).div_euclid(grid.pixel_size_e12))
        else {
            continue;
        };
        let Ok(column) =
            u32::try_from((cell.easting_e12 - grid.west_e12).div_euclid(grid.pixel_size_e12))
        else {
            continue;
        };
        if row >= grid.height || column >= grid.width {
            continue;
        }
        let chunk_index = (row / SOILGRIDS_OVERVIEW_CHUNK_HEIGHT)
            .checked_mul(chunks_across)
            .and_then(|value| value.checked_add(column / SOILGRIDS_OVERVIEW_CHUNK_WIDTH))
            .context("SoilGrids source chunk index overflow")?;
        requests[usize::try_from(chunk_index)?].push(SoilgridsChunkSampleRequest {
            target_index: u32::try_from(target_index)?,
            row,
            column,
        });
    }
    Ok(SoilgridsChunkGroups { requests })
}

type SoilgridsRawCellValues = [[i16; 3]; 9];

struct SoilgridsDecodedChunk<'a> {
    decoded: &'a TiffDecodingResult,
    data_width: u32,
    data_height: u32,
    source_width: u32,
    source_height: u32,
    property_index: usize,
    quantile_index: usize,
}

fn apply_soilgrids_decoded_chunk(
    chunk: SoilgridsDecodedChunk<'_>,
    requests: &[SoilgridsChunkSampleRequest],
    values: &mut [SoilgridsRawCellValues],
) -> Result<()> {
    let TiffDecodingResult::I16(decoded) = chunk.decoded else {
        bail!("SoilGrids overview chunk is not signed i16");
    };
    if decoded.len()
        != usize::try_from(
            chunk
                .data_width
                .checked_mul(chunk.data_height)
                .context("SoilGrids decoded chunk area overflow")?,
        )?
    {
        bail!("SoilGrids decoded chunk length differs from its dimensions");
    }
    for request in requests {
        if request.row >= chunk.source_height || request.column >= chunk.source_width {
            continue;
        }
        let local_row = request.row % SOILGRIDS_OVERVIEW_CHUNK_HEIGHT;
        let local_column = request.column % SOILGRIDS_OVERVIEW_CHUNK_WIDTH;
        if local_row >= chunk.data_height || local_column >= chunk.data_width {
            bail!("SoilGrids sample lies outside its decoded source chunk");
        }
        let source_index = local_row
            .checked_mul(chunk.data_width)
            .and_then(|value| value.checked_add(local_column))
            .context("SoilGrids decoded sample index overflow")?;
        values
            .get_mut(usize::try_from(request.target_index)?)
            .context("SoilGrids sample target index exceeds the global field")?
            [chunk.property_index][chunk.quantile_index] = decoded[usize::try_from(source_index)?];
    }
    Ok(())
}

struct SoilgridsSampledField {
    values: Vec<SoilgridsRawCellValues>,
    decoded_source_chunks: u64,
}

fn sample_global_soilgrids_topsoil(
    artifact_root: &Path,
    source_set: &SoilgridsSourceSet,
    projection: &[SoilgridsProjectedCell],
) -> Result<SoilgridsSampledField> {
    if source_set.rasters.len()
        != SOILGRIDS_TOPSOIL_PROPERTIES.len() * SOILGRIDS_TOPSOIL_QUANTILES.len()
    {
        bail!("SoilGrids source raster set is incomplete");
    }
    let mut values = vec![[[SOILGRIDS_NO_DATA_VALUE; 3]; 9]; projection.len()];
    let mut decoded_source_chunks = 0_u64;
    let mut grids = Vec::new();
    for raster in &source_set.rasters {
        if !grids.contains(&raster.grid) {
            grids.push(raster.grid);
        }
    }
    let mut completed_rasters = 0_usize;
    for grid in grids {
        let groups = group_soilgrids_projection_by_source_chunk(projection, grid)?;
        for (raster_index, raster) in source_set
            .rasters
            .iter()
            .enumerate()
            .filter(|(_, raster)| raster.grid == grid)
        {
            let property_index = raster_index / SOILGRIDS_TOPSOIL_QUANTILES.len();
            let quantile_index = raster_index % SOILGRIDS_TOPSOIL_QUANTILES.len();
            let path = artifact_root.join(&raster.artifact_path);
            let metadata = fs::metadata(&path)?;
            if metadata.len() != raster.byte_length {
                bail!("SoilGrids overview changed after source verification");
            }
            let mut decoder = TiffDecoder::new(File::open(&path)?)
                .with_context(|| format!("open SoilGrids overview {}", path.display()))?;
            for (chunk_index, requests) in groups.requests.iter().enumerate() {
                if requests.is_empty() {
                    continue;
                }
                let chunk_index = u32::try_from(chunk_index)?;
                let decoded = decoder.read_chunk(chunk_index).with_context(|| {
                    format!(
                        "decode SoilGrids property {} quantile {} chunk {chunk_index}",
                        SOILGRIDS_TOPSOIL_PROPERTIES[property_index],
                        SOILGRIDS_TOPSOIL_QUANTILES[quantile_index]
                    )
                })?;
                let (data_width, data_height) = decoder.chunk_data_dimensions(chunk_index);
                apply_soilgrids_decoded_chunk(
                    SoilgridsDecodedChunk {
                        decoded: &decoded,
                        data_width,
                        data_height,
                        source_width: raster.width,
                        source_height: raster.height,
                        property_index,
                        quantile_index,
                    },
                    requests,
                    &mut values,
                )?;
                decoded_source_chunks = decoded_source_chunks
                    .checked_add(1)
                    .context("SoilGrids decoded source-chunk count overflow")?;
            }
            completed_rasters += 1;
            eprintln!(
                "SoilGrids grouped sampling progress: {completed_rasters}/27 property-quantile rasters"
            );
        }
    }
    Ok(SoilgridsSampledField {
        values,
        decoded_source_chunks,
    })
}

fn quantile_values(values: [i16; 3]) -> SoilGridsQuantileValues {
    SoilGridsQuantileValues {
        q0_05: values[0],
        q0_5: values[1],
        q0_95: values[2],
    }
}

fn validate_resumable_soilgrids_tile(
    bytes: &[u8],
    layer_id: &str,
    source_inventory_digest: Digest,
    source_set_digest: Digest,
    property_sources: &[SoilGridsPropertySource],
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<()> {
    let tile = PackedSoilGridsTopsoilTile::from_canonical_slice(bytes)
        .context("decode staged SoilGrids topsoil tile")?;
    if tile.layer_id != layer_id
        || tile.depth != SoilDepth::ZeroToFiveCentimeters
        || tile.source_snapshot_digest != source_inventory_digest
        || tile.source_set_digest != source_set_digest
        || tile.property_sources != property_sources
        || tile.sampling_reprojection_method != SOILGRIDS_SAMPLING_REPROJECTION_METHOD
        || tile.container_s2_cell_id != container_s2_cell_id
        || tile.target_s2_level != target_s2_level
        || tile.cells.iter().any(|cell| cell.support_samples != 1)
    {
        bail!("staged SoilGrids topsoil tile differs from requested derivation");
    }
    Ok(())
}

struct SoilgridsLayerPackingProfile<'a> {
    layer_id: &'a str,
    source_inventory_digest: Digest,
    source_set_digest: Digest,
    property_sources: &'a [SoilGridsPropertySource],
    container_s2_level: u8,
    target_s2_level: u8,
}

fn write_packed_soilgrids_topsoil_layer(
    output_directory: &Path,
    profile: SoilgridsLayerPackingProfile<'_>,
    projected_cells: &[SoilgridsProjectedCell],
    values: &[SoilgridsRawCellValues],
) -> Result<(String, Vec<u8>)> {
    if projected_cells.len() != values.len() {
        bail!("SoilGrids projected-cell and value counts differ");
    }
    let level_directory = format!("l{}", profile.container_s2_level);
    let tile_directory = output_directory
        .join("layers")
        .join(profile.layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let containers = global_s2_cells_at_level(profile.container_s2_level)?;
    let mut entries = Vec::with_capacity(containers.len());
    let mut target_cursor = 0_usize;
    for (position, container) in containers.into_iter().enumerate() {
        let relative_path = format!(
            "layers/{}/{level_directory}/{container}.tile",
            profile.layer_id
        );
        let artifact_path = output_directory.join(&relative_path);
        let expected_cells = enumerate_s2_descendants(container, profile.target_s2_level)?;
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_soilgrids_tile(
                    &existing,
                    profile.layer_id,
                    profile.source_inventory_digest,
                    profile.source_set_digest,
                    profile.property_sources,
                    container,
                    profile.target_s2_level,
                )?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let cells = expected_cells
                    .iter()
                    .enumerate()
                    .map(|(cell_offset, expected_cell)| {
                        let field_index = target_cursor
                            .checked_add(cell_offset)
                            .context("SoilGrids target index overflow")?;
                        let projected = projected_cells
                            .get(field_index)
                            .context("SoilGrids projected field ended early")?;
                        if projected.s2_cell_id != *expected_cell {
                            bail!("SoilGrids projected field is not in canonical S2 order");
                        }
                        let property_values = values
                            .get(field_index)
                            .context("SoilGrids value field ended early")?
                            .map(quantile_values);
                        Ok(SoilGridsTopsoilCell {
                            s2_cell_id: *expected_cell,
                            support_samples: 1,
                            property_values,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let tile = PackedSoilGridsTopsoilTile {
                    tile_schema_version: 1,
                    layer_id: profile.layer_id.to_owned(),
                    depth: SoilDepth::ZeroToFiveCentimeters,
                    source_snapshot_digest: profile.source_inventory_digest,
                    source_set_digest: profile.source_set_digest,
                    property_sources: profile.property_sources.to_vec(),
                    sampling_reprojection_method: SOILGRIDS_SAMPLING_REPROJECTION_METHOD.to_owned(),
                    container_s2_cell_id: container,
                    target_s2_level: profile.target_s2_level,
                    cells,
                };
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged SoilGrids topsoil tile"),
        };
        target_cursor = target_cursor
            .checked_add(expected_cells.len())
            .context("SoilGrids target cursor overflow")?;
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: profile.container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_SOILGRIDS_TOPSOIL_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 1_024 == 0 {
            eprintln!(
                "SoilGrids layer packing progress: {}/24576 containers",
                position + 1
            );
        }
    }
    if target_cursor != projected_cells.len() {
        bail!("SoilGrids projected field contains unconsumed target cells");
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: profile.layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{}/root.index", profile.layer_id);
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged SoilGrids root differs from requested derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged SoilGrids root"),
    }
    Ok((root_relative_path, root_bytes))
}

#[derive(Debug, Serialize)]
struct SoilgridsTopsoilLayerDerivation {
    derivation_schema_version: u16,
    status: &'static str,
    source_inventory_digest: Digest,
    source_set_digest: Digest,
    proj_version: String,
    sampling_reprojection_method: &'static str,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    target_cells: u64,
    decoded_source_chunks: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

fn derive_soilgrids_topsoil_layer(
    source_inventory: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!("provisional SoilGrids topsoil layer v1 requires L6 containers and L10 targets");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!("SoilGrids topsoil output directory already exists");
    }
    let source_set = load_verified_soilgrids_source_set(source_inventory, artifact_root)?;
    let source_set_digest = soilgrids_source_set_digest(&source_set.property_sources);
    let projection = project_global_s2_centres_to_soilgrids(target_s2_level)?;
    let sampled = sample_global_soilgrids_topsoil(artifact_root, &source_set, &projection.cells)?;
    let staging_directory =
        prepare_or_resume_layer_staging_directory(output_directory, "SoilGrids topsoil")?;
    let (root_index_path, root_bytes) = write_packed_soilgrids_topsoil_layer(
        &staging_directory,
        SoilgridsLayerPackingProfile {
            layer_id,
            source_inventory_digest: source_set.inventory_digest,
            source_set_digest,
            property_sources: &source_set.property_sources,
            container_s2_level,
            target_s2_level,
        },
        &projection.cells,
        &sampled.values,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish SoilGrids topsoil directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&SoilgridsTopsoilLayerDerivation {
            derivation_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            source_inventory_digest: source_set.inventory_digest,
            source_set_digest,
            proj_version: projection.proj_version,
            sampling_reprojection_method: SOILGRIDS_SAMPLING_REPROJECTION_METHOD,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            target_cells: u64::try_from(sampled.values.len())?,
            decoded_source_chunks: sampled.decoded_source_chunks,
            output_directory: output_directory.display().to_string(),
            root_index_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct SoilgridsQuantileInspection {
    no_data_cells: u64,
    minimum_source_value: Option<i16>,
    maximum_source_value: Option<i16>,
}

impl SoilgridsQuantileInspection {
    fn observe(&mut self, value: i16) -> Result<()> {
        if value == SOILGRIDS_NO_DATA_VALUE {
            self.no_data_cells = self
                .no_data_cells
                .checked_add(1)
                .context("SoilGrids no-data count overflow")?;
        } else {
            self.minimum_source_value = Some(
                self.minimum_source_value
                    .map_or(value, |minimum| minimum.min(value)),
            );
            self.maximum_source_value = Some(
                self.maximum_source_value
                    .map_or(value, |maximum| maximum.max(value)),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct SoilgridsPropertyInspection {
    property: &'static str,
    quantiles: Vec<SoilgridsNamedQuantileInspection>,
}

#[derive(Debug, Serialize)]
struct SoilgridsNamedQuantileInspection {
    quantile: &'static str,
    #[serde(flatten)]
    summary: SoilgridsQuantileInspection,
}

#[derive(Debug, Serialize)]
struct SoilgridsTopsoilLayerInspection {
    inspection_schema_version: u16,
    status: &'static str,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    depth: &'static str,
    sampling_reprojection_method: String,
    source_inventory_digest: Digest,
    source_set_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    tile_byte_length: u64,
    properties: Vec<SoilgridsPropertyInspection>,
}

fn inspect_soilgrids_topsoil_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical SoilGrids topsoil root")?;
    if root.layer_id != layer_id {
        bail!("SoilGrids root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("SoilGrids root does not cover every expected container");
    }
    let mut source_inventory_digest = None;
    let mut source_set_digest = None;
    let mut property_sources: Option<Vec<SoilGridsPropertySource>> = None;
    let mut sampling_reprojection_method = None;
    let mut summaries = [[SoilgridsQuantileInspection::default(); 3]; 9];
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.media_type != PACKED_SOILGRIDS_TOPSOIL_TILE_MEDIA_TYPE
        {
            bail!("SoilGrids root contains an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("SoilGrids tile fails its root reference");
        }
        let tile = PackedSoilGridsTopsoilTile::from_canonical_slice(&bytes)
            .context("decode canonical SoilGrids topsoil tile")?;
        if tile.layer_id != layer_id
            || tile.depth != SoilDepth::ZeroToFiveCentimeters
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
            || tile.sampling_reprojection_method != SOILGRIDS_SAMPLING_REPROJECTION_METHOD
            || tile.cells.iter().any(|cell| cell.support_samples != 1)
        {
            bail!("SoilGrids tile has inconsistent packing metadata");
        }
        if source_inventory_digest.is_some_and(|expected| expected != tile.source_snapshot_digest)
            || source_set_digest.is_some_and(|expected| expected != tile.source_set_digest)
            || property_sources
                .as_ref()
                .is_some_and(|expected| expected != &tile.property_sources)
            || sampling_reprojection_method
                .as_ref()
                .is_some_and(|expected| expected != &tile.sampling_reprojection_method)
        {
            bail!("SoilGrids tiles disagree on provenance or sampling metadata");
        }
        source_inventory_digest.get_or_insert(tile.source_snapshot_digest);
        source_set_digest.get_or_insert(tile.source_set_digest);
        property_sources.get_or_insert(tile.property_sources);
        sampling_reprojection_method.get_or_insert(tile.sampling_reprojection_method);
        for cell in &tile.cells {
            for (property_index, property) in cell.property_values.iter().enumerate() {
                for (quantile_index, value) in [property.q0_05, property.q0_5, property.q0_95]
                    .into_iter()
                    .enumerate()
                {
                    summaries[property_index][quantile_index].observe(value)?;
                }
            }
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("SoilGrids tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("SoilGrids target-cell total overflow")?;
    }
    let properties = SOILGRIDS_TOPSOIL_PROPERTIES
        .iter()
        .enumerate()
        .map(|(property_index, property)| SoilgridsPropertyInspection {
            property,
            quantiles: SOILGRIDS_TOPSOIL_QUANTILES
                .iter()
                .enumerate()
                .map(
                    |(quantile_index, quantile)| SoilgridsNamedQuantileInspection {
                        quantile,
                        summary: summaries[property_index][quantile_index],
                    },
                )
                .collect(),
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&SoilgridsTopsoilLayerInspection {
            inspection_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            depth: SOILGRIDS_DEPTH,
            sampling_reprojection_method: sampling_reprojection_method
                .context("SoilGrids root is empty")?,
            source_inventory_digest: source_inventory_digest.context("SoilGrids root is empty")?,
            source_set_digest: source_set_digest.context("SoilGrids root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            tile_byte_length,
            properties,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct GeographicRouteInspection {
    inspection_schema_version: u16,
    coordinate_frame: &'static str,
    latitude_e7: i32,
    longitude_e7: i32,
    s2_level: u8,
    s2_cell_id: String,
}

fn inspect_geographic_route(latitude_e7: i32, longitude_e7: i32, s2_level: u8) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)
        .context("validate WGS 84 geographic coordinate")?;
    let cell = route_geographic_to_s2(coordinate, s2_level)
        .context("route geographic coordinate to S2")?;
    println!(
        "{}",
        serde_json::to_string(&GeographicRouteInspection {
            inspection_schema_version: 1,
            coordinate_frame: "WGS 84 geodetic coordinates routed through a WGS 84 ECEF ray",
            latitude_e7,
            longitude_e7,
            s2_level,
            s2_cell_id: cell.to_string(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoCellRouteInspection {
    inspection_schema_version: u16,
    source_grid: &'static str,
    sample_support: &'static str,
    boundary_convention: &'static str,
    row: u32,
    column: u32,
    latitude_half_arcseconds: i32,
    longitude_half_arcseconds: i32,
    south_boundary_half_arcseconds: i32,
    north_boundary_half_arcseconds: i32,
    west_boundary_half_arcseconds: i32,
    east_boundary_half_arcseconds: i32,
    s2_level: u8,
    s2_cell_id: String,
}

fn inspect_etopo_cell_route(row: u32, column: u32, s2_level: u8) -> Result<()> {
    let support = etopo_cell_support(row, column)?;
    let cell = route_half_arcsecond_to_s2(support.centre, s2_level)
        .context("route ETOPO cell centre to S2")?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCellRouteInspection {
            inspection_schema_version: 1,
            source_grid: "NOAA ETOPO 2022 v1 60-arc-second WGS 84 / EGM2008",
            sample_support: "60-arc-second area cell center",
            boundary_convention: "south/west inclusive, north/east exclusive; +180 wraps to -180",
            row,
            column,
            latitude_half_arcseconds: support.centre.latitude_half_arcseconds(),
            longitude_half_arcseconds: support.centre.longitude_half_arcseconds(),
            south_boundary_half_arcseconds: support.south_boundary_half_arcseconds,
            north_boundary_half_arcseconds: support.north_boundary_half_arcseconds,
            west_boundary_half_arcseconds: support.west_boundary_half_arcseconds,
            east_boundary_half_arcseconds: support.east_boundary_half_arcseconds,
            s2_level,
            s2_cell_id: cell.to_string(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoCellQuadratureInspection {
    inspection_schema_version: u16,
    source_grid: &'static str,
    support_policy: &'static str,
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
    source_sample_count: u32,
    target_cells: Vec<EtopoQuadratureTargetCell>,
}

#[derive(Serialize)]
struct EtopoQuadratureTargetCell {
    s2_cell_id: String,
    equal_weight_samples: u32,
}

#[derive(Serialize)]
struct EtopoQuadratureThroughputInspection {
    inspection_schema_version: u16,
    start_row: u32,
    source_rows: u32,
    target_s2_level: u8,
    points_per_axis: u8,
    source_cells_routed: u64,
    interior_points_routed: u64,
    elapsed_milliseconds: u128,
}

/// Returns a deterministic, equal-point quadrature of an ETOPO source area cell.
///
/// This is deliberately an approximation to source-cell overlap, rather than an
/// assertion of exact spherical area intersection. Points are strictly interior, so
/// no source sample depends on an arbitrary north/east boundary ownership rule.
fn etopo_cell_quadrature(
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
) -> Result<std::collections::BTreeMap<S2CellId, u32>> {
    if points_per_axis == 0 || 60 % points_per_axis != 0 {
        bail!("points_per_axis must be a non-zero divisor of 60");
    }
    if s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let support = etopo_cell_support(row, column)?;
    let denominator = i32::from(points_per_axis)
        .checked_mul(2)
        .context("ETOPO quadrature denominator overflow")?;
    let spacing = ETOPO_CELL_STEP_HALF_ARCSECONDS / denominator;
    if spacing == 0 || ETOPO_CELL_STEP_HALF_ARCSECONDS % denominator != 0 {
        bail!("points_per_axis does not produce an exact ETOPO half-arcsecond quadrature");
    }
    let mut cells = std::collections::BTreeMap::new();
    for latitude_index in 0..i32::from(points_per_axis) {
        for longitude_index in 0..i32::from(points_per_axis) {
            let latitude_offset = latitude_index
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .context("ETOPO quadrature latitude index overflow")?;
            let longitude_offset = longitude_index
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .context("ETOPO quadrature longitude index overflow")?;
            let latitude = support
                .south_boundary_half_arcseconds
                .checked_add(
                    spacing
                        .checked_mul(latitude_offset)
                        .context("ETOPO quadrature latitude overflow")?,
                )
                .context("ETOPO quadrature latitude overflow")?;
            let longitude = support
                .west_boundary_half_arcseconds
                .checked_add(
                    spacing
                        .checked_mul(longitude_offset)
                        .context("ETOPO quadrature longitude overflow")?,
                )
                .context("ETOPO quadrature longitude overflow")?;
            let coordinate = GeographicCoordinateHalfArcsecond::new(latitude, longitude)
                .context("construct ETOPO quadrature coordinate")?;
            let target = route_half_arcsecond_to_s2(coordinate, s2_level)
                .context("route ETOPO quadrature point to S2")?;
            *cells.entry(target).or_insert(0) += 1;
        }
    }
    Ok(cells)
}

fn inspect_etopo_cell_quadrature(
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let cells = etopo_cell_quadrature(row, column, s2_level, points_per_axis)?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCellQuadratureInspection {
            inspection_schema_version: 1,
            source_grid: "NOAA ETOPO 2022 v1 60-arc-second WGS 84 / EGM2008",
            support_policy: "equal interior lattice points; approximate source-cell overlap, not exact spherical clipping",
            row,
            column,
            s2_level,
            points_per_axis,
            source_sample_count: u32::from(points_per_axis) * u32::from(points_per_axis),
            target_cells: cells
                .into_iter()
                .map(|(cell, equal_weight_samples)| EtopoQuadratureTargetCell {
                    s2_cell_id: cell.to_string(),
                    equal_weight_samples,
                })
                .collect(),
        })?
    );
    Ok(())
}

fn inspect_etopo_quadrature_throughput(
    start_row: u32,
    source_rows: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    if source_rows == 0
        || u64::from(start_row)
            .checked_add(u64::from(source_rows))
            .is_none_or(|end| end > ETOPO_LATITUDE_CELLS)
    {
        bail!("requested ETOPO throughput rows are outside the source grid");
    }
    let started = Instant::now();
    let mut interior_points_routed = 0_u64;
    for row in start_row..start_row + source_rows {
        for column in 0..u32::try_from(ETOPO_LONGITUDE_CELLS)? {
            let samples = etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?;
            interior_points_routed = interior_points_routed
                .checked_add(samples.values().map(|count| u64::from(*count)).sum::<u64>())
                .context("ETOPO throughput point count overflow")?;
        }
    }
    let source_cells_routed = u64::from(source_rows)
        .checked_mul(ETOPO_LONGITUDE_CELLS)
        .context("ETOPO throughput source-cell count overflow")?;
    println!(
        "{}",
        serde_json::to_string(&EtopoQuadratureThroughputInspection {
            inspection_schema_version: 1,
            start_row,
            source_rows,
            target_s2_level,
            points_per_axis,
            source_cells_routed,
            interior_points_routed,
            elapsed_milliseconds: started.elapsed().as_millis(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoTerrainLayerInspection {
    inspection_schema_version: u16,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    quadrature_points_per_axis: u8,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    tile_byte_length: u64,
}

/// Fully validate the flat L6→L10 terrain release before it is used as evidence.
///
/// This is intentionally independent from derivation: it rereads each canonical tile,
/// checks the root's content-addressed references, and requires uniform provenance and
/// packing parameters across the complete global layer.
fn inspect_etopo_terrain_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical ETOPO terrain-layer root index")?;
    if root.layer_id != layer_id {
        bail!(
            "ETOPO terrain root declares layer {:?}, expected {:?}",
            root.layer_id,
            layer_id
        );
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!(
            "ETOPO terrain root has {} tiles, expected {} at S2 level {container_s2_level}",
            root.entries.len(),
            expected_containers.len()
        );
    }

    let mut source_snapshot_digest = None;
    let mut source_artifact_digest = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
        {
            bail!(
                "ETOPO terrain root entry is not the expected L{container_s2_level} tile for {expected_container}"
            );
        }
        if entry.artifact.media_type != PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE {
            bail!(
                "ETOPO terrain tile {:?} has unexpected media type {:?}",
                entry.artifact.path,
                entry.artifact.media_type
            );
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!(
                "ETOPO terrain tile {:?} fails its root reference",
                entry.artifact.path
            );
        }
        let tile = PackedScalarTerrainTile::from_canonical_slice(&bytes).with_context(|| {
            format!(
                "decode canonical ETOPO terrain tile {:?}",
                entry.artifact.path
            )
        })?;
        if tile.layer_id != layer_id
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
            || tile.quadrature_points_per_axis != points_per_axis
        {
            bail!(
                "ETOPO terrain tile {:?} has inconsistent packing metadata",
                entry.artifact.path
            );
        }
        match source_snapshot_digest {
            Some(expected) if expected != tile.source_snapshot_digest => {
                bail!("ETOPO terrain tiles disagree on source snapshot digest")
            }
            None => source_snapshot_digest = Some(tile.source_snapshot_digest),
            _ => {}
        }
        match source_artifact_digest {
            Some(expected) if expected != tile.source_artifact_digest => {
                bail!("ETOPO terrain tiles disagree on source artifact digest")
            }
            None => source_artifact_digest = Some(tile.source_artifact_digest),
            _ => {}
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("ETOPO terrain tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("ETOPO terrain target cell total overflow")?;
    }

    println!(
        "{}",
        serde_json::to_string(&EtopoTerrainLayerInspection {
            inspection_schema_version: 1,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            quadrature_points_per_axis: points_per_axis,
            source_snapshot_digest: source_snapshot_digest
                .context("ETOPO terrain root is empty")?,
            source_artifact_digest: source_artifact_digest
                .context("ETOPO terrain root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            tile_byte_length,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandReferenceLayerInspection {
    inspection_schema_version: u16,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: String,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    tile_byte_length: u64,
}

/// Independently validate a complete generalized-land reference release.
fn inspect_natural_earth_land_reference_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical Natural Earth land-reference root index")?;
    if root.layer_id != layer_id {
        bail!("land-reference root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("land-reference root does not cover every expected container");
    }
    let mut source_snapshot_digest = None;
    let mut source_artifact_digest = None;
    let mut sample_policy = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.media_type != PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE
        {
            bail!("land-reference root has an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("land-reference tile fails its root reference");
        }
        let tile = PackedBooleanFieldTile::from_canonical_slice(&bytes)
            .context("decode canonical land-reference tile")?;
        if tile.layer_id != layer_id
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
        {
            bail!("land-reference tile has inconsistent packing metadata");
        }
        for (name, observed, expected) in [
            (
                "source snapshot",
                tile.source_snapshot_digest,
                source_snapshot_digest,
            ),
            (
                "source artifact",
                tile.source_artifact_digest,
                source_artifact_digest,
            ),
        ] {
            if let Some(expected) = expected
                && observed != expected
            {
                bail!("land-reference tiles disagree on {name} provenance");
            }
        }
        if let Some(expected) = sample_policy.as_ref()
            && &tile.sample_policy != expected
        {
            bail!("land-reference tiles disagree on sample policy");
        }
        source_snapshot_digest.get_or_insert(tile.source_snapshot_digest);
        source_artifact_digest.get_or_insert(tile.source_artifact_digest);
        sample_policy.get_or_insert(tile.sample_policy);
        tile_byte_length = tile_byte_length
            .checked_add(u64::try_from(bytes.len())?)
            .context("land-reference tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("land-reference target cell total overflow")?;
    }
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandReferenceLayerInspection {
            inspection_schema_version: 1,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: sample_policy.context("land-reference root is empty")?,
            source_snapshot_digest: source_snapshot_digest
                .context("land-reference root is empty")?,
            source_artifact_digest: source_artifact_digest
                .context("land-reference root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            tile_byte_length,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct CopernicusLandCoverLayerInspection {
    inspection_schema_version: u16,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: String,
    quadrature_points_per_axis: u8,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    target_support_samples: u64,
    tile_byte_length: u64,
    class_sample_counts: Vec<RasterValueCount>,
}

/// Independently validate every tile and root reference in observed land cover.
fn inspect_copernicus_land_cover_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical Copernicus land-cover root index")?;
    if root.layer_id != layer_id {
        bail!("Copernicus land-cover root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("Copernicus land-cover root does not cover every expected container");
    }
    let expected_policy = copernicus_land_cover_sample_policy(points_per_axis);
    let mut source_snapshot_digest = None;
    let mut source_artifact_digest = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    let mut target_support_samples = 0_u64;
    let mut class_sample_counts = [0_u64; 256];
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        let expected_path =
            format!("layers/{layer_id}/l{container_s2_level}/{expected_container}.tile");
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.path != expected_path
            || entry.artifact.media_type != PACKED_LAND_COVER_EVIDENCE_TILE_MEDIA_TYPE
        {
            bail!("Copernicus land-cover root has an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("Copernicus land-cover tile fails its root reference");
        }
        let tile = PackedLandCoverEvidenceTile::from_canonical_slice(&bytes)
            .context("decode canonical Copernicus land-cover tile")?;
        if tile.layer_id != layer_id
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
            || tile.quadrature_points_per_axis != points_per_axis
            || tile.sample_policy != expected_policy
        {
            bail!("Copernicus land-cover tile has inconsistent packing metadata");
        }
        match source_snapshot_digest {
            Some(expected) if expected != tile.source_snapshot_digest => {
                bail!("Copernicus land-cover tiles disagree on source snapshot digest")
            }
            None => source_snapshot_digest = Some(tile.source_snapshot_digest),
            _ => {}
        }
        match source_artifact_digest {
            Some(expected) if expected != tile.source_artifact_digest => {
                bail!("Copernicus land-cover tiles disagree on source artifact digest")
            }
            None => source_artifact_digest = Some(tile.source_artifact_digest),
            _ => {}
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("Copernicus land-cover tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("Copernicus land-cover target-cell total overflow")?;
        for cell in tile.cells {
            target_support_samples = target_support_samples
                .checked_add(cell.support_samples)
                .context("Copernicus target-support total overflow")?;
            for count in cell.class_counts {
                class_sample_counts[usize::from(count.class_value)] = class_sample_counts
                    [usize::from(count.class_value)]
                .checked_add(count.samples)
                .context("Copernicus class sample total overflow")?;
            }
        }
    }
    let expected_support = target_cell_count
        .checked_mul(u64::from(points_per_axis) * u64::from(points_per_axis))
        .context("Copernicus expected target-support total overflow")?;
    if target_support_samples != expected_support {
        bail!("Copernicus land-cover release lost target-support samples");
    }
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverLayerInspection {
            inspection_schema_version: 1,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: expected_policy,
            quadrature_points_per_axis: points_per_axis,
            source_snapshot_digest: source_snapshot_digest
                .context("Copernicus land-cover root is empty")?,
            source_artifact_digest: source_artifact_digest
                .context("Copernicus land-cover root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            target_support_samples,
            tile_byte_length,
            class_sample_counts: raster_value_counts_u8(&class_sample_counts),
        })?
    );
    Ok(())
}

fn read_release_file(root: &Path, relative_path: &str) -> Result<Vec<u8>> {
    if Path::new(relative_path)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("release artifact path {relative_path:?} is not portable");
    }
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve release directory {}", root.display()))?;
    let path = canonical_root.join(relative_path);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("inspect release artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("release artifact {} must be a regular file", path.display());
    }
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("resolve release artifact {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("release artifact {relative_path:?} escapes its release directory");
    }
    fs::read(canonical_path).with_context(|| format!("read release artifact {relative_path:?}"))
}

const ETOPO_GRID_MAGIC: &[u8; 8] = b"ATCETOP1";
const ETOPO_GRID_SCHEMA_VERSION: u16 = 1;
const ETOPO_CENTRE_INDEX_MAGIC: &[u8; 8] = b"ATCECI1\0";
const ETOPO_CENTRE_INDEX_SCHEMA_VERSION: u16 = 1;
const ETOPO_CENTRE_SUMMARY_MAGIC: &[u8; 8] = b"ATCECS1\0";
const ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION: u16 = 1;
const ETOPO_LATITUDE_CELLS: u64 = 10_800;
const ETOPO_LONGITUDE_CELLS: u64 = 21_600;
const ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS: i32 = -647_940;
const ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS: i32 = -1_295_940;
const ETOPO_CELL_STEP_HALF_ARCSECONDS: i32 = 120;
const ETOPO_GRID_HEADER_LENGTH: usize = 84;
const ETOPO_CENTRE_INDEX_HEADER_LENGTH: usize = 88;
const ETOPO_CENTRE_INDEX_RECORD_LENGTH: usize = 12;
const ETOPO_CENTRE_SUMMARY_HEADER_LENGTH: usize = 124;
const ETOPO_CENTRE_SUMMARY_RECORD_LENGTH: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EtopoCellSupport {
    centre: GeographicCoordinateHalfArcsecond,
    south_boundary_half_arcseconds: i32,
    north_boundary_half_arcseconds: i32,
    west_boundary_half_arcseconds: i32,
    east_boundary_half_arcseconds: i32,
}

fn etopo_cell_support(row: u32, column: u32) -> Result<EtopoCellSupport> {
    if u64::from(row) >= ETOPO_LATITUDE_CELLS || u64::from(column) >= ETOPO_LONGITUDE_CELLS {
        bail!("ETOPO row and column must be within its pinned 10800 by 21600 grid");
    }
    let south_boundary_half_arcseconds = (-648_000_i32)
        .checked_add(
            i32::try_from(row)?
                .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                .context("ETOPO row coordinate overflow")?,
        )
        .context("ETOPO latitude coordinate overflow")?;
    let west_boundary_half_arcseconds = (-1_296_000_i32)
        .checked_add(
            i32::try_from(column)?
                .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                .context("ETOPO column coordinate overflow")?,
        )
        .context("ETOPO longitude coordinate overflow")?;
    let north_boundary_half_arcseconds = south_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS)
        .context("ETOPO north boundary overflow")?;
    let east_boundary_half_arcseconds = west_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS)
        .context("ETOPO east boundary overflow")?;
    let latitude_half_arcseconds = south_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS / 2)
        .context("ETOPO latitude centre overflow")?;
    let longitude_half_arcseconds = west_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS / 2)
        .context("ETOPO longitude centre overflow")?;
    if latitude_half_arcseconds
        != ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS
            + i32::try_from(row)? * ETOPO_CELL_STEP_HALF_ARCSECONDS
        || longitude_half_arcseconds
            != ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS
                + i32::try_from(column)? * ETOPO_CELL_STEP_HALF_ARCSECONDS
    {
        bail!("ETOPO source-centre lattice disagrees with its declared area support");
    }
    Ok(EtopoCellSupport {
        centre: GeographicCoordinateHalfArcsecond::new(
            latitude_half_arcseconds,
            longitude_half_arcseconds,
        )
        .context("derive exact ETOPO area-cell centre")?,
        south_boundary_half_arcseconds,
        north_boundary_half_arcseconds,
        west_boundary_half_arcseconds,
        east_boundary_half_arcseconds,
    })
}

#[derive(Serialize)]
struct EtopoGridDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    sample_arc_minutes: u16,
    latitude_cells: u32,
    longitude_cells: u32,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoCentreIndexDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    sample_arc_minutes: u16,
    s2_level: u8,
    latitude_cells: u32,
    longitude_cells: u32,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoCentreSummaryDerivation {
    derivation_schema_version: u16,
    input_path: String,
    input_hash: Digest,
    source_snapshot_digest: Digest,
    source_artifact_hash: Digest,
    source_sample_arc_minutes: u16,
    source_s2_level: u8,
    summary_s2_level: u8,
    summary_cells: u32,
    source_samples: u64,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoQuadratureContributionDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    row: u32,
    column: u32,
    raw_elevation_ieee754_le_hex: String,
    target_s2_level: u8,
    points_per_axis: u8,
    target_cells: Vec<EtopoQuadratureContributionCell>,
}

#[derive(Serialize)]
struct EtopoQuadratureContributionCell {
    s2_cell_id: String,
    support_samples: u64,
    minimum_millimetres: i64,
    mean_millimetres: i64,
    maximum_millimetres: i64,
}

#[derive(Serialize)]
struct EtopoTerrainLayerDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
    target_cells: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

#[derive(Clone, Copy, Debug)]
struct EtopoCentreIndexHeader {
    sample_arc_minutes: u16,
    s2_level: u8,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
}

#[derive(Clone, Copy, Debug)]
struct EtopoCentreIndexRecord {
    cell: S2CellId,
    value_bits: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EtopoCentreSummaryStats {
    samples: u64,
    total_millimetres: i64,
    minimum_millimetres: i64,
    maximum_millimetres: i64,
}

impl EtopoCentreSummaryStats {
    fn add(&mut self, millimetres: i64) -> Result<()> {
        self.add_weighted(millimetres, 1)
    }

    fn add_weighted(&mut self, millimetres: i64, samples: u64) -> Result<()> {
        if samples == 0 {
            bail!("ETOPO summary cannot add zero support samples");
        }
        if self.samples == 0 {
            self.minimum_millimetres = millimetres;
            self.maximum_millimetres = millimetres;
        } else {
            self.minimum_millimetres = self.minimum_millimetres.min(millimetres);
            self.maximum_millimetres = self.maximum_millimetres.max(millimetres);
        }
        self.samples = self
            .samples
            .checked_add(samples)
            .context("ETOPO summary sample overflow")?;
        let weighted = millimetres
            .checked_mul(i64::try_from(samples).context("ETOPO support sample count exceeds i64")?)
            .context("ETOPO summary weighted total overflow")?;
        self.total_millimetres = self
            .total_millimetres
            .checked_add(weighted)
            .context("ETOPO summary total overflow")?;
        Ok(())
    }

    fn mean_millimetres(self) -> i64 {
        debug_assert!(self.samples > 0);
        round_divide_i64(self.total_millimetres, self.samples as i64)
    }
}

/// Add one raw ETOPO source cell to its target-level S2 quadrature summaries.
///
/// This is the common kernel for the eventual global streaming derivation. Values are
/// converted to integer millimetres before aggregation, so the emitted tile does not
/// depend on host floating-point accumulation order.
fn accumulate_etopo_cell_quadrature(
    summaries: &mut BTreeMap<S2CellId, EtopoCentreSummaryStats>,
    row: u32,
    column: u32,
    value_bits: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let millimetres = f32_bits_to_rounded_millimetres(value_bits)?;
    for (cell, support_samples) in
        etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?
    {
        summaries
            .entry(cell)
            .or_default()
            .add_weighted(millimetres, u64::from(support_samples))?;
    }
    Ok(())
}

/// Fast equivalent of [`accumulate_etopo_cell_quadrature`] for the long full-source
/// pass. Hash-table iteration never reaches canonical bytes: the completed summaries
/// are sorted into a `BTreeMap` before tile packing.
fn accumulate_etopo_cell_quadrature_unordered(
    summaries: &mut HashMap<S2CellId, EtopoCentreSummaryStats>,
    row: u32,
    column: u32,
    value_bits: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let millimetres = f32_bits_to_rounded_millimetres(value_bits)?;
    for (cell, support_samples) in
        etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?
    {
        summaries
            .entry(cell)
            .or_default()
            .add_weighted(millimetres, u64::from(support_samples))?;
    }
    Ok(())
}

fn derive_etopo_quadrature_contribution(
    manifest_path: &Path,
    artifact_root: &Path,
    row: u32,
    column: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    etopo_cell_support(row, column)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let source_snapshot_id = snapshot.snapshot_id.clone();
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Index(u64::from(row)),
            NcSliceInfoElem::Index(u64::from(column)),
        ],
    };
    let values = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read exact ETOPO source elevation cell")?;
    let value = values
        .as_slice()
        .context("ETOPO source-cell selection is not contiguous")?
        .first()
        .copied()
        .context("ETOPO source-cell selection is empty")?;
    let mut summaries = BTreeMap::new();
    accumulate_etopo_cell_quadrature(
        &mut summaries,
        row,
        column,
        value.to_bits(),
        target_s2_level,
        points_per_axis,
    )?;
    let target_cells = summaries
        .into_iter()
        .map(|(s2_cell_id, stats)| EtopoQuadratureContributionCell {
            s2_cell_id: s2_cell_id.to_string(),
            support_samples: stats.samples,
            minimum_millimetres: stats.minimum_millimetres,
            mean_millimetres: stats.mean_millimetres(),
            maximum_millimetres: stats.maximum_millimetres,
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&EtopoQuadratureContributionDerivation {
            derivation_schema_version: 1,
            source_snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            row,
            column,
            raw_elevation_ieee754_le_hex: format!("{:08x}", value.to_bits()),
            target_s2_level,
            points_per_axis,
            target_cells,
        })?
    );
    Ok(())
}

fn derive_etopo_terrain_layer(
    manifest_path: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!(
            "ETOPO terrain-layer v1 requires L6 containers and L10 target values; requested L{container_s2_level} to L{target_s2_level}"
        );
    }
    if points_per_axis == 0 || 60 % points_per_axis != 0 {
        bail!("points_per_axis must be a non-zero divisor of 60");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "ETOPO terrain output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .context("ETOPO terrain output directory has no parent")?;
    if !output_parent.is_dir() {
        bail!(
            "ETOPO terrain output parent {} is not a directory",
            output_parent.display()
        );
    }

    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let source_snapshot_id = snapshot.snapshot_id.clone();
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;

    let mut summaries = HashMap::new();
    for row in 0..ETOPO_LATITUDE_CELLS {
        let selection = NcSliceInfo {
            selections: vec![
                NcSliceInfoElem::Slice {
                    start: row,
                    end: row.checked_add(1).context("ETOPO row selection overflow")?,
                    step: 1,
                },
                NcSliceInfoElem::Slice {
                    start: 0,
                    end: ETOPO_LONGITUDE_CELLS,
                    step: 1,
                },
            ],
        };
        let values = file
            .read_variable_slice::<f32>("z", &selection)
            .with_context(|| format!("read ETOPO row {row}"))?;
        let values = values
            .as_slice()
            .context("ETOPO row selection is not contiguous")?;
        if values.len() != usize::try_from(ETOPO_LONGITUDE_CELLS)? {
            bail!("ETOPO row {row} has an unexpected sample count");
        }
        for (column, value) in values.iter().copied().enumerate() {
            accumulate_etopo_cell_quadrature_unordered(
                &mut summaries,
                u32::try_from(row)?,
                u32::try_from(column)?,
                value.to_bits(),
                target_s2_level,
                points_per_axis,
            )?;
        }
        // Progress is operational diagnostics only. It is written to stderr and has no
        // effect on emitted canonical bytes or the final derivation record.
        let completed_rows = row + 1;
        if completed_rows % 360 == 0 || completed_rows == ETOPO_LATITUDE_CELLS {
            eprintln!(
                "ETOPO terrain normalization progress: {completed_rows}/{ETOPO_LATITUDE_CELLS} source rows"
            );
        }
    }

    // Canonical packing performs ordered point lookups. Sorting only after every
    // source contribution is accumulated retains bit-for-bit output while avoiding a
    // tree traversal on each interior quadrature update.
    let summaries = summaries.into_iter().collect::<BTreeMap<_, _>>();

    let staging_directory =
        prepare_or_resume_layer_staging_directory(output_directory, "ETOPO terrain")?;
    let (root_relative_path, root_bytes) = write_packed_etopo_terrain_layer(
        &staging_directory,
        EtopoTerrainPackingProfile {
            layer_id,
            source_snapshot_digest,
            source_artifact_digest: artifact.content_hash,
            points_per_axis,
            container_s2_level,
            target_s2_level,
        },
        &summaries,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish ETOPO terrain directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&EtopoTerrainLayerDerivation {
            derivation_schema_version: 1,
            source_snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            points_per_axis,
            target_cells: u64::try_from(summaries.len())?,
            output_directory: output_directory.display().to_string(),
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandReferenceLayerDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: &'static str,
    target_cells: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

/// Create a full global, centre-classified Natural Earth land-reference layer.
///
/// This is deliberately distinct from a coastline layer: source vertices are
/// generalized cartography. The Boolean payload says only whether the exact centre
/// of each target S2 cell is inside the pinned polygon stream.
fn derive_natural_earth_land_reference_layer(
    manifest_path: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!("Natural Earth land-reference v1 requires L6 containers and L10 target values");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!("land-reference output directory already exists");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let prepared_land = PreparedNaturalEarthLand::from_shapefile(&bytes)
        .context("validate and prepare Natural Earth polygon stream")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let staging_directory = prepare_terrain_layer_staging_directory(output_directory)?;
    let (root_relative_path, root_bytes) = write_packed_natural_earth_land_reference_layer(
        &staging_directory,
        layer_id,
        source_snapshot_digest,
        artifact.content_hash,
        &prepared_land,
        container_s2_level,
        target_s2_level,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish land-reference directory {}",
            output_directory.display()
        )
    })?;
    let target_cells = u64::try_from(global_s2_cells_at_level(target_s2_level)?.len())?;
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandReferenceLayerDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: "s2-cell-centre-e7-v1",
            target_cells,
            output_directory: output_directory.display().to_string(),
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

fn write_packed_natural_earth_land_reference_layer(
    output_directory: &Path,
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    prepared_land: &PreparedNaturalEarthLand,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{container_s2_level}");
    let tile_directory = output_directory
        .join("layers")
        .join(layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let mut entries = Vec::new();
    for (position, container) in global_s2_cells_at_level(container_s2_level)?
        .into_iter()
        .enumerate()
    {
        let relative_path = format!("layers/{layer_id}/{level_directory}/{container}.tile");
        let artifact_path = output_directory.join(&relative_path);
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_natural_earth_land_tile(
                    &existing,
                    layer_id,
                    source_snapshot_digest,
                    source_artifact_digest,
                    container,
                    target_s2_level,
                )?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let tile = pack_natural_earth_land_reference_tile(
                    layer_id,
                    source_snapshot_digest,
                    source_artifact_digest,
                    prepared_land,
                    container,
                    target_s2_level,
                )?;
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged land-reference tile"),
        };
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 1_024 == 0 {
            eprintln!(
                "Natural Earth land-reference normalization progress: {}/24576 containers",
                position + 1
            );
        }
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged land-reference root does not match the requested derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged land-reference root"),
    }
    Ok((root_relative_path, root_bytes))
}

fn validate_resumable_natural_earth_land_tile(
    bytes: &[u8],
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<()> {
    let tile = PackedBooleanFieldTile::from_canonical_slice(bytes)
        .context("decode staged land-reference tile")?;
    if tile.canonical_bytes()? != bytes {
        bail!("staged land-reference tile is not canonical");
    }
    if tile.tile_schema_version != 1
        || tile.layer_id != layer_id
        || tile.source_snapshot_digest != source_snapshot_digest
        || tile.source_artifact_digest != source_artifact_digest
        || tile.sample_policy != "s2-cell-centre-e7-v1"
        || tile.container_s2_cell_id != container_s2_cell_id
        || tile.target_s2_level != target_s2_level
    {
        bail!("staged land-reference tile does not match the requested derivation");
    }
    tile.validate()
        .context("validate staged land-reference tile")
}

fn pack_natural_earth_land_reference_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    prepared_land: &PreparedNaturalEarthLand,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<PackedBooleanFieldTile> {
    let cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?
        .into_iter()
        .map(|s2_cell_id| {
            let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
                decode_s2_face_ij(s2_cell_id),
            )?)?)?;
            Ok(BooleanFieldCell {
                s2_cell_id,
                support_samples: 1,
                true_samples: u64::from(
                    prepared_land
                        .contains_point(coordinate.longitude_e7(), coordinate.latitude_e7()),
                ),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedBooleanFieldTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        source_snapshot_digest,
        source_artifact_digest,
        sample_policy: "s2-cell-centre-e7-v1".to_owned(),
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed land-reference tile is invalid")?;
    Ok(tile)
}

/// Reserve a same-parent hidden staging directory. The final rename is atomic on the
/// release filesystem, so readers can observe either no release or a complete root and
/// its referenced tiles, never the writer's intermediate tree.
fn prepare_terrain_layer_staging_directory(output_directory: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "ETOPO terrain output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .context("ETOPO terrain output directory has no parent")?;
    if !output_parent.is_dir() {
        bail!(
            "ETOPO terrain output parent {} is not a directory",
            output_parent.display()
        );
    }
    let output_name = output_directory
        .file_name()
        .and_then(OsStr::to_str)
        .context("ETOPO terrain output directory name is not UTF-8")?;
    let staging_directory = output_parent.join(format!(".{output_name}.staging"));
    if fs::symlink_metadata(&staging_directory).is_ok() {
        bail!(
            "ETOPO terrain staging directory {} already exists; inspect it before retrying",
            staging_directory.display()
        );
    }
    fs::create_dir(&staging_directory).with_context(|| {
        format!(
            "create hidden ETOPO terrain staging directory {}",
            staging_directory.display()
        )
    })?;
    Ok(staging_directory)
}

/// Resume only an unpublished, hidden layer staging tree. Every reused tile
/// is decoded, re-canonicalized, and checked against the current layer/source/profile
/// before it is admitted into the new root index. This makes an interrupted long global
/// derivation recoverable without ever exposing a partial release or replacing data.
fn prepare_or_resume_layer_staging_directory(
    output_directory: &Path,
    layer_label: &str,
) -> Result<PathBuf> {
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "{layer_label} output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .with_context(|| format!("{layer_label} output directory has no parent"))?;
    if !output_parent.is_dir() {
        bail!(
            "{layer_label} output parent {} is not a directory",
            output_parent.display()
        );
    }
    let output_name = output_directory
        .file_name()
        .and_then(OsStr::to_str)
        .with_context(|| format!("{layer_label} output directory name is not UTF-8"))?;
    let staging_directory = output_parent.join(format!(".{output_name}.staging"));
    match fs::symlink_metadata(&staging_directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => bail!(
            "{layer_label} staging path {} is not a real directory",
            staging_directory.display()
        ),
        Ok(_) => Ok(staging_directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&staging_directory)?;
            Ok(staging_directory)
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy)]
struct EtopoTerrainPackingProfile<'a> {
    layer_id: &'a str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    points_per_axis: u8,
    container_s2_level: u8,
    target_s2_level: u8,
}

fn write_packed_etopo_terrain_layer(
    output_directory: &Path,
    profile: EtopoTerrainPackingProfile<'_>,
    summaries: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{}", profile.container_s2_level);
    let layer_directory = output_directory.join("layers").join(profile.layer_id);
    let tile_directory = layer_directory.join(&level_directory);
    fs::create_dir_all(&tile_directory).with_context(|| {
        format!(
            "create ETOPO terrain tile directory {}",
            tile_directory.display()
        )
    })?;

    let mut entries = Vec::new();
    for container in global_s2_cells_at_level(profile.container_s2_level)? {
        let tile = pack_etopo_terrain_tile(
            profile.layer_id,
            profile.source_snapshot_digest,
            profile.source_artifact_digest,
            profile.points_per_axis,
            container,
            profile.target_s2_level,
            summaries,
        )?;
        let bytes = tile.canonical_bytes()?;
        let relative_path = format!(
            "layers/{}/{level_directory}/{container}.tile",
            profile.layer_id
        );
        write_new_artifact(&output_directory.join(&relative_path), &bytes)?;
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: profile.container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: profile.layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{}/root.index", profile.layer_id);
    write_new_artifact(&output_directory.join(&root_relative_path), &root_bytes)?;
    Ok((root_relative_path, root_bytes))
}

fn global_s2_cells_at_level(target_s2_level: u8) -> Result<Vec<S2CellId>> {
    if target_s2_level > MAX_S2_LEVEL {
        bail!("global S2 level must be within 0 through {MAX_S2_LEVEL}");
    }
    let mut cells = Vec::new();
    for face in 0_u64..6 {
        let root = S2CellId::new((face << 61) | (1_u64 << 60))?;
        if target_s2_level == 0 {
            cells.push(root);
        } else {
            cells.extend(enumerate_s2_descendants(root, target_s2_level)?);
        }
    }
    Ok(cells)
}

/// Pack all known target summaries in one coarse S2 container into a release tile.
fn pack_etopo_terrain_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    quadrature_points_per_axis: u8,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
    summaries: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
) -> Result<PackedScalarTerrainTile> {
    let cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?
        .into_iter()
        .map(|s2_cell_id| {
            let stats = summaries.get(&s2_cell_id).with_context(|| {
                format!("ETOPO quadrature has no support for expected target S2 cell {s2_cell_id}")
            })?;
            Ok(ScalarTerrainCell {
                s2_cell_id,
                support_samples: stats.samples,
                minimum_millimetres: stats.minimum_millimetres,
                mean_millimetres: stats.mean_millimetres(),
                maximum_millimetres: stats.maximum_millimetres,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedScalarTerrainTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        source_snapshot_digest,
        source_artifact_digest,
        quadrature_points_per_axis,
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed ETOPO terrain tile is invalid")?;
    Ok(tile)
}

fn enumerate_s2_descendants(root: S2CellId, target_s2_level: u8) -> Result<Vec<S2CellId>> {
    if target_s2_level <= root.level() || target_s2_level > MAX_S2_LEVEL {
        bail!(
            "target S2 level {target_s2_level} must be finer than container level {} and at most {MAX_S2_LEVEL}",
            root.level()
        );
    }
    let mut current = vec![root];
    while current
        .first()
        .is_some_and(|cell| cell.level() < target_s2_level)
    {
        let mut next = Vec::with_capacity(
            current
                .len()
                .checked_mul(4)
                .context("S2 descendant count overflow")?,
        );
        for cell in current {
            next.extend(cell.children().context("enumerate S2 children")?);
        }
        current = next;
    }
    Ok(current)
}

fn derive_etopo_grid(
    manifest_path: &Path,
    artifact_root: &Path,
    sample_arc_minutes: u16,
    output_path: &Path,
) -> Result<()> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LATITUDE_CELLS,
                step: stride,
            },
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LONGITUDE_CELLS,
                step: stride,
            },
        ],
    };
    let samples = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read selected ETOPO elevation cells")?;
    let latitude_cells = u32::try_from(ETOPO_LATITUDE_CELLS / stride)?;
    let longitude_cells = u32::try_from(ETOPO_LONGITUDE_CELLS / stride)?;
    let expected_samples = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    let values = samples
        .as_slice()
        .context("ETOPO selection is not contiguous")?;
    if values.len() != expected_samples {
        bail!("ETOPO selection has an unexpected number of cells");
    }
    let bytes = encode_etopo_grid(
        sample_arc_minutes,
        snapshot_digest,
        artifact.content_hash,
        latitude_cells,
        longitude_cells,
        values,
    )?;
    write_new_artifact(output_path, &bytes)?;
    let output_hash = Digest::sha256(&bytes);
    println!(
        "{}",
        serde_json::to_string(&EtopoGridDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id.clone(),
            source_snapshot_digest: snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            sample_arc_minutes,
            latitude_cells,
            longitude_cells,
            output_path: output_path.display().to_string(),
            output_hash,
            output_byte_length: u64::try_from(bytes.len())?,
        })?
    );
    Ok(())
}

fn derive_etopo_centre_index(
    manifest_path: &Path,
    artifact_root: &Path,
    sample_arc_minutes: u16,
    s2_level: u8,
    output_path: &Path,
) -> Result<()> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    if s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LATITUDE_CELLS,
                step: stride,
            },
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LONGITUDE_CELLS,
                step: stride,
            },
        ],
    };
    let samples = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read selected ETOPO elevation cells")?;
    let latitude_cells = u32::try_from(ETOPO_LATITUDE_CELLS / stride)?;
    let longitude_cells = u32::try_from(ETOPO_LONGITUDE_CELLS / stride)?;
    let expected_samples = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    let values = samples
        .as_slice()
        .context("ETOPO selection is not contiguous")?;
    if values.len() != expected_samples {
        bail!("ETOPO selection has an unexpected number of cells");
    }
    let bytes = encode_etopo_centre_index(
        sample_arc_minutes,
        s2_level,
        snapshot_digest,
        artifact.content_hash,
        latitude_cells,
        longitude_cells,
        values,
    )?;
    write_new_artifact(output_path, &bytes)?;
    let output_hash = Digest::sha256(&bytes);
    println!(
        "{}",
        serde_json::to_string(&EtopoCentreIndexDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id.clone(),
            source_snapshot_digest: snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            sample_arc_minutes,
            s2_level,
            latitude_cells,
            longitude_cells,
            output_path: output_path.display().to_string(),
            output_hash,
            output_byte_length: u64::try_from(bytes.len())?,
        })?
    );
    Ok(())
}

fn derive_etopo_centre_summary(
    input_path: &Path,
    summary_s2_level: u8,
    output_path: &Path,
) -> Result<()> {
    if summary_s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let bytes = fs::read(input_path)
        .with_context(|| format!("read ETOPO centre index {}", input_path.display()))?;
    let input_hash = Digest::sha256(&bytes);
    let (header, records) = decode_etopo_centre_index(&bytes)?;
    if summary_s2_level > header.s2_level {
        bail!(
            "summary s2_level {summary_s2_level} is finer than centre index level {}",
            header.s2_level
        );
    }

    let mut cells = std::collections::BTreeMap::<S2CellId, EtopoCentreSummaryStats>::new();
    for (sample_index, record) in records.iter().copied().enumerate() {
        let source_cell = record.cell;
        let expected = expected_etopo_index_cell(sample_index, header)?;
        if source_cell != expected {
            bail!("centre-index record {sample_index} disagrees with the pinned source lattice");
        }
        let target = source_cell
            .ancestor(summary_s2_level)
            .context("derive coarser S2 source-attribution address")?;
        let millimetres = f32_bits_to_rounded_millimetres(record.value_bits)?;
        cells.entry(target).or_default().add(millimetres)?;
    }
    let source_samples = u64::from(header.latitude_cells)
        .checked_mul(u64::from(header.longitude_cells))
        .context("ETOPO centre index sample count overflow")?;
    let output =
        encode_etopo_centre_summary(input_hash, header, summary_s2_level, &cells, source_samples)?;
    write_new_artifact(output_path, &output)?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCentreSummaryDerivation {
            derivation_schema_version: 1,
            input_path: input_path.display().to_string(),
            input_hash,
            source_snapshot_digest: header.snapshot_digest,
            source_artifact_hash: header.artifact_digest,
            source_sample_arc_minutes: header.sample_arc_minutes,
            source_s2_level: header.s2_level,
            summary_s2_level,
            summary_cells: u32::try_from(cells.len())?,
            source_samples,
            output_path: output_path.display().to_string(),
            output_hash: Digest::sha256(&output),
            output_byte_length: u64::try_from(output.len())?,
        })?
    );
    Ok(())
}

fn validate_etopo_sample_stride(sample_arc_minutes: u16) -> Result<u64> {
    let stride = u64::from(sample_arc_minutes);
    if sample_arc_minutes == 0
        || 60 % sample_arc_minutes != 0
        || !ETOPO_LATITUDE_CELLS.is_multiple_of(stride)
        || !ETOPO_LONGITUDE_CELLS.is_multiple_of(stride)
    {
        bail!("sample_arc_minutes must be a non-zero divisor of 60");
    }
    Ok(stride)
}

fn etopo_data_artifact(snapshot: &SourceSnapshotManifest) -> Result<&SourceSnapshotArtifact> {
    snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no ETOPO NetCDF data artifact")
}

fn validate_etopo_schema(file: &NcFile) -> Result<()> {
    let lat = file.variable("lat").context("ETOPO has no lat variable")?;
    let lon = file.variable("lon").context("ETOPO has no lon variable")?;
    let elevation = file.variable("z").context("ETOPO has no z variable")?;
    if lat.shape() != [ETOPO_LATITUDE_CELLS]
        || lon.shape() != [ETOPO_LONGITUDE_CELLS]
        || elevation.shape() != [ETOPO_LATITUDE_CELLS, ETOPO_LONGITUDE_CELLS]
    {
        bail!("ETOPO variables do not have the pinned global 60-arc-second shape");
    }
    validate_etopo_coordinate_axes(file)?;
    Ok(())
}

fn validate_etopo_coordinate_axes(file: &NcFile) -> Result<()> {
    validate_etopo_coordinate_axis(
        file,
        "lat",
        ETOPO_LATITUDE_CELLS,
        ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS,
        "latitude",
    )?;
    validate_etopo_coordinate_axis(
        file,
        "lon",
        ETOPO_LONGITUDE_CELLS,
        ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS,
        "longitude",
    )
}

fn validate_etopo_coordinate_axis(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
    first_half_arcseconds: i32,
    axis_name: &str,
) -> Result<()> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read ETOPO {axis_name} coordinate axis"))?;
    let values = values
        .as_slice()
        .with_context(|| format!("ETOPO {axis_name} coordinate axis is not contiguous"))?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("ETOPO {axis_name} coordinate axis has an unexpected length");
    }
    for (index, value) in values.iter().copied().enumerate() {
        let expected = first_half_arcseconds
            .checked_add(
                i32::try_from(index)?
                    .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                    .context("ETOPO axis index overflow")?,
            )
            .context("ETOPO coordinate axis overflow")?;
        let observed = f64_to_half_arcseconds(value).with_context(|| {
            format!("ETOPO {axis_name} coordinate {index} is not on the half-arcsecond lattice")
        })?;
        if observed != expected {
            bail!(
                "ETOPO {axis_name} coordinate {index} is {observed} half-arcseconds, expected {expected}"
            );
        }
    }
    Ok(())
}

fn f64_to_half_arcseconds(value: f64) -> Result<i32> {
    if !value.is_finite() {
        bail!("coordinate is not finite");
    }
    let scaled = value * 7_200.0;
    let rounded = scaled.round();
    if (scaled - rounded).abs() > 0.000_001 {
        bail!("coordinate is not within one millionth half-arcsecond of the source lattice");
    }
    i32::try_from(rounded as i64).context("coordinate is outside the half-arcsecond domain")
}

fn encode_etopo_grid(
    sample_arc_minutes: u16,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
    values: &[f32],
) -> Result<Vec<u8>> {
    let expected_values = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    if values.len() != expected_values {
        bail!("ETOPO grid value count disagrees with its declared dimensions");
    }
    let value_bytes = values
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .context("ETOPO grid byte length overflow")?;
    let total = ETOPO_GRID_HEADER_LENGTH
        .checked_add(value_bytes)
        .context("ETOPO grid total byte length overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_GRID_MAGIC);
    bytes.extend_from_slice(&ETOPO_GRID_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&sample_arc_minutes.to_le_bytes());
    bytes.extend_from_slice(snapshot_digest.as_bytes());
    bytes.extend_from_slice(artifact_digest.as_bytes());
    bytes.extend_from_slice(&latitude_cells.to_le_bytes());
    bytes.extend_from_slice(&longitude_cells.to_le_bytes());
    for value in values {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    Ok(bytes)
}

fn encode_etopo_centre_index(
    sample_arc_minutes: u16,
    s2_level: u8,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
    values: &[f32],
) -> Result<Vec<u8>> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    let expected_values = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    if values.len() != expected_values
        || u64::from(latitude_cells) * stride != ETOPO_LATITUDE_CELLS
        || u64::from(longitude_cells) * stride != ETOPO_LONGITUDE_CELLS
    {
        bail!("ETOPO centre index dimensions disagree with sampled source values");
    }
    let record_bytes = values
        .len()
        .checked_mul(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
        .context("ETOPO centre index byte length overflow")?;
    let total = ETOPO_CENTRE_INDEX_HEADER_LENGTH
        .checked_add(record_bytes)
        .context("ETOPO centre index total byte length overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_CENTRE_INDEX_MAGIC);
    bytes.extend_from_slice(&ETOPO_CENTRE_INDEX_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&sample_arc_minutes.to_le_bytes());
    bytes.push(s2_level);
    bytes.extend_from_slice(&[0_u8; 3]);
    bytes.extend_from_slice(snapshot_digest.as_bytes());
    bytes.extend_from_slice(artifact_digest.as_bytes());
    bytes.extend_from_slice(&latitude_cells.to_le_bytes());
    bytes.extend_from_slice(&longitude_cells.to_le_bytes());

    for row in 0..latitude_cells {
        let source_row = u32::try_from(u64::from(row) * stride)?;
        for column in 0..longitude_cells {
            let source_column = u32::try_from(u64::from(column) * stride)?;
            let support = etopo_cell_support(source_row, source_column)?;
            let cell = route_half_arcsecond_to_s2(support.centre, s2_level)
                .context("route exact ETOPO source centre to S2")?;
            let value_index =
                usize::try_from(u64::from(row) * u64::from(longitude_cells) + u64::from(column))?;
            bytes.extend_from_slice(&cell.get().to_be_bytes());
            bytes.extend_from_slice(&values[value_index].to_bits().to_le_bytes());
        }
    }
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
}

fn decode_etopo_centre_index(
    bytes: &[u8],
) -> Result<(EtopoCentreIndexHeader, Vec<EtopoCentreIndexRecord>)> {
    if bytes.len() < ETOPO_CENTRE_INDEX_HEADER_LENGTH {
        bail!("ETOPO centre index is shorter than its header");
    }
    if &bytes[..8] != ETOPO_CENTRE_INDEX_MAGIC {
        bail!("ETOPO centre index magic is invalid");
    }
    let schema_version = u16::from_le_bytes(bytes[8..10].try_into()?);
    if schema_version != ETOPO_CENTRE_INDEX_SCHEMA_VERSION {
        bail!("ETOPO centre index schema version {schema_version} is unsupported");
    }
    let sample_arc_minutes = u16::from_le_bytes(bytes[10..12].try_into()?);
    let s2_level = bytes[12];
    if bytes[13..16] != [0_u8; 3] {
        bail!("ETOPO centre index reserved bytes must be zero");
    }
    let header = EtopoCentreIndexHeader {
        sample_arc_minutes,
        s2_level,
        snapshot_digest: Digest::from_bytes(bytes[16..48].try_into()?),
        artifact_digest: Digest::from_bytes(bytes[48..80].try_into()?),
        latitude_cells: u32::from_le_bytes(bytes[80..84].try_into()?),
        longitude_cells: u32::from_le_bytes(bytes[84..88].try_into()?),
    };
    let stride = validate_etopo_sample_stride(header.sample_arc_minutes)?;
    if header.s2_level > MAX_S2_LEVEL
        || u64::from(header.latitude_cells) * stride != ETOPO_LATITUDE_CELLS
        || u64::from(header.longitude_cells) * stride != ETOPO_LONGITUDE_CELLS
    {
        bail!("ETOPO centre index header dimensions or S2 level are invalid");
    }
    let record_count = usize::try_from(
        u64::from(header.latitude_cells)
            .checked_mul(u64::from(header.longitude_cells))
            .context("ETOPO centre index record count overflow")?,
    )?;
    let expected_length = ETOPO_CENTRE_INDEX_HEADER_LENGTH
        .checked_add(
            record_count
                .checked_mul(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
                .context("ETOPO centre index byte length overflow")?,
        )
        .context("ETOPO centre index total byte length overflow")?;
    if bytes.len() != expected_length {
        bail!("ETOPO centre index byte length disagrees with its header");
    }
    let records = bytes[ETOPO_CENTRE_INDEX_HEADER_LENGTH..]
        .chunks_exact(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
        .map(|record| {
            Ok(EtopoCentreIndexRecord {
                cell: S2CellId::new(u64::from_be_bytes(
                    record[..8].try_into().expect("fixed record cell"),
                ))?,
                value_bits: u32::from_le_bytes(
                    record[8..12].try_into().expect("fixed record value"),
                ),
            })
        });
    Ok((header, records.collect::<Result<Vec<_>>>()?))
}

fn expected_etopo_index_cell(
    sample_index: usize,
    header: EtopoCentreIndexHeader,
) -> Result<S2CellId> {
    let columns = usize::try_from(header.longitude_cells)?;
    let row = u32::try_from(sample_index / columns)?;
    let column = u32::try_from(sample_index % columns)?;
    let stride = validate_etopo_sample_stride(header.sample_arc_minutes)?;
    let source_row = u32::try_from(u64::from(row) * stride)?;
    let source_column = u32::try_from(u64::from(column) * stride)?;
    route_half_arcsecond_to_s2(
        etopo_cell_support(source_row, source_column)?.centre,
        header.s2_level,
    )
    .context("route expected ETOPO centre-index source cell")
}

fn f32_bits_to_rounded_millimetres(bits: u32) -> Result<i64> {
    let sign = if bits >> 31 == 0 { 1_i128 } else { -1_i128 };
    let exponent = ((bits >> 23) & 0xff) as i32;
    let fraction = bits & 0x007f_ffff;
    if exponent == 0xff {
        bail!("ETOPO value is not finite");
    }
    let (significand, power) = if exponent == 0 {
        (i128::from(fraction), -149)
    } else {
        (i128::from((1_u32 << 23) | fraction), exponent - 150)
    };
    let numerator = sign
        .checked_mul(significand)
        .and_then(|value| value.checked_mul(1_000))
        .context("ETOPO millimetre conversion overflow")?;
    let millimetres = if power >= 0 {
        numerator
            .checked_shl(u32::try_from(power)?)
            .context("ETOPO millimetre conversion overflow")?
    } else {
        let divisor_shift = u32::try_from(-power)?;
        // A finite f32 significand times 1,000 is below 2^34. Any divisor at
        // 2^127 or greater therefore rounds to zero at millimetre precision and
        // cannot be a nearest-even tie. Avoid constructing an out-of-range i128.
        if divisor_shift >= 127 {
            0
        } else {
            round_divide_i128(numerator, 1_i128 << divisor_shift)
        }
    };
    i64::try_from(millimetres).context("ETOPO value is outside signed millimetre range")
}

/// Convert a finite IEEE-754 binary64 source value to a nearest-even fixed integer
/// without performing host floating-point multiplication or casting at the boundary.
fn f64_bits_to_rounded_scaled_integer(bits: u64, scale: i128) -> Result<i128> {
    if scale <= 0 {
        bail!("fixed-point scale must be positive");
    }
    let sign = if bits >> 63 == 0 { 1_i128 } else { -1_i128 };
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0x7ff {
        bail!("binary64 source value is not finite");
    }
    let (significand, power) = if exponent == 0 {
        (i128::from(fraction), -1074)
    } else {
        (i128::from((1_u64 << 52) | fraction), exponent - 1075)
    };
    let numerator = sign
        .checked_mul(significand)
        .and_then(|value| value.checked_mul(scale))
        .context("binary64 fixed-point numerator overflow")?;
    if power >= 0 {
        numerator
            .checked_shl(u32::try_from(power)?)
            .context("binary64 fixed-point conversion overflow")
    } else {
        let divisor_shift = u32::try_from(-power)?;
        if divisor_shift > 127 {
            Ok(0)
        } else if divisor_shift == 127 {
            let half = 1_u128 << 126;
            Ok(if numerator.unsigned_abs() > half {
                numerator.signum()
            } else {
                // Exactly one half ties to the even integer zero.
                0
            })
        } else {
            Ok(round_divide_i128(numerator, 1_i128 << divisor_shift))
        }
    }
}

fn round_divide_i128(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice_remainder = remainder.unsigned_abs() * 2;
    let denominator = denominator as u128;
    if twice_remainder > denominator || (twice_remainder == denominator && quotient % 2 != 0) {
        quotient + numerator.signum()
    } else {
        quotient
    }
}

fn round_divide_i64(numerator: i64, denominator: i64) -> i64 {
    i64::try_from(round_divide_i128(
        i128::from(numerator),
        i128::from(denominator),
    ))
    .expect("i64 division result remains within i64")
}

fn encode_etopo_centre_summary(
    input_hash: Digest,
    header: EtopoCentreIndexHeader,
    summary_s2_level: u8,
    cells: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
    source_samples: u64,
) -> Result<Vec<u8>> {
    let record_bytes = cells
        .len()
        .checked_mul(ETOPO_CENTRE_SUMMARY_RECORD_LENGTH)
        .context("ETOPO centre summary record bytes overflow")?;
    let total = ETOPO_CENTRE_SUMMARY_HEADER_LENGTH
        .checked_add(record_bytes)
        .context("ETOPO centre summary total bytes overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_CENTRE_SUMMARY_MAGIC);
    bytes.extend_from_slice(&ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&header.sample_arc_minutes.to_le_bytes());
    bytes.push(header.s2_level);
    bytes.push(summary_s2_level);
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes.extend_from_slice(input_hash.as_bytes());
    bytes.extend_from_slice(header.snapshot_digest.as_bytes());
    bytes.extend_from_slice(header.artifact_digest.as_bytes());
    bytes.extend_from_slice(&u32::try_from(cells.len())?.to_le_bytes());
    bytes.extend_from_slice(&source_samples.to_le_bytes());
    for (cell, stats) in cells {
        bytes.extend_from_slice(&cell.get().to_be_bytes());
        bytes.extend_from_slice(&stats.samples.to_le_bytes());
        bytes.extend_from_slice(&stats.minimum_millimetres.to_le_bytes());
        bytes.extend_from_slice(&stats.mean_millimetres().to_le_bytes());
        bytes.extend_from_slice(&stats.maximum_millimetres.to_le_bytes());
    }
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
}

fn write_new_artifact(output_path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output_path
        .parent()
        .context("output path has no parent directory")?;
    let parent = parent
        .canonicalize()
        .with_context(|| format!("failed to resolve output directory {}", parent.display()))?;
    let file_name = output_path
        .file_name()
        .and_then(OsStr::to_str)
        .context("output filename is not UTF-8")?;
    let destination = parent.join(file_name);
    if fs::symlink_metadata(&destination).is_ok() {
        bail!("derived artifact {} already exists", destination.display());
    }
    let mut partial = PartialDownload::create(&parent, file_name)?;
    partial
        .file
        .write_all(bytes)
        .with_context(|| format!("failed to write {}", partial.path.display()))?;
    partial
        .file
        .sync_all()
        .with_context(|| format!("failed to sync {}", partial.path.display()))?;
    partial.persist_without_replacement(&destination)
}

#[derive(Serialize)]
struct EtopoInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    variables: Vec<EtopoVariableInspection>,
}

#[derive(Serialize)]
struct ChelsaAttributeInspection {
    name: String,
    string_value: Option<String>,
    first_numeric_value: Option<String>,
}

#[derive(Serialize)]
struct EtopoVariableInspection {
    name: String,
    shape: Vec<u64>,
}

const CHELSA_LATITUDE_CELLS: u64 = 20_880;
const CHELSA_LONGITUDE_CELLS: u64 = 43_200;
const CHELSA_SOURCE_CHUNK_CELLS: u32 = 500;
const CHELSA_MISSING_MILLICELSIUS: i64 = i64::MIN;

#[derive(Serialize)]
struct ChelsaJanuaryTemperatureInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    data_variable: String,
    data_shape: Vec<u64>,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    data_attributes: Vec<ChelsaAttributeInspection>,
    variables: Vec<EtopoVariableInspection>,
}

#[derive(Serialize)]
struct ChelsaAnnualTemperatureInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    monthly_normals: Vec<ChelsaMonthlyTemperatureInspection>,
    shared_latitude_endpoint_ieee754_le_hex: [String; 2],
    shared_longitude_endpoint_ieee754_le_hex: [String; 2],
    documented_raw_to_millicelsius: &'static str,
}

#[derive(Serialize)]
struct ChelsaMonthlyTemperatureInspection {
    month: u8,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
}

const ERA5_ARCHIVE_MEMBERS: [&str; 2] = [
    "data_stream-moda_stepType-avgua.nc",
    "data_stream-moda_stepType-avgad.nc",
];

#[derive(Serialize)]
struct Era5AnnualArchiveInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    year: u16,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    members: Vec<Era5MemberInspection>,
}

#[derive(Serialize)]
struct Era5MemberInspection {
    name: String,
    uncompressed_byte_length: u64,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    variables: Vec<EtopoVariableInspection>,
}

const ERA5_MONTHS_PER_YEAR: u64 = 12;
const ERA5_LATITUDE_CELLS: u64 = 721;
const ERA5_LONGITUDE_CELLS: u64 = 1_440;

fn expected_era5_member_variables(member_name: &str) -> Result<BTreeMap<&'static str, Vec<u64>>> {
    let spatial = vec![
        ERA5_MONTHS_PER_YEAR,
        ERA5_LATITUDE_CELLS,
        ERA5_LONGITUDE_CELLS,
    ];
    let mut expected = BTreeMap::from([
        ("expver", vec![ERA5_MONTHS_PER_YEAR]),
        ("latitude", vec![ERA5_LATITUDE_CELLS]),
        ("longitude", vec![ERA5_LONGITUDE_CELLS]),
        ("number", Vec::new()),
        ("valid_time", vec![ERA5_MONTHS_PER_YEAR]),
    ]);
    match member_name {
        "data_stream-moda_stepType-avgua.nc" => {
            for name in ["siconc", "sst", "t2m", "u10", "v10"] {
                expected.insert(name, spatial.clone());
            }
        }
        "data_stream-moda_stepType-avgad.nc" => {
            expected.insert("tp", spatial);
        }
        _ => bail!("unrecognized ERA5 archive member {member_name}"),
    }
    Ok(expected)
}

fn validate_era5_member_schema(file: &NcFile, member_name: &str) -> Result<()> {
    let expected = expected_era5_member_variables(member_name)?;
    let observed = file
        .variables()
        .with_context(|| format!("enumerate variables in ERA5 member {member_name}"))?
        .iter()
        .map(|variable| (variable.name(), variable.shape().to_vec()))
        .collect::<BTreeMap<_, _>>();
    if observed != expected {
        bail!("ERA5 member {member_name} variable schema changed");
    }
    Ok(())
}

/// Inspect one verified ERA5 archive in memory. ZIP member bytes never become a
/// source artifact or a durable intermediate: this is a parser-boundary probe before
/// climate semantics and spatial normalization are specified.
fn inspect_era5_annual_archive(
    manifest_path: &Path,
    artifact_root: &Path,
    year: u16,
) -> Result<()> {
    if !(1981..=2010).contains(&year) {
        bail!("ERA5 inspection year must be inside the pinned 1981-2010 normal period");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let expected_name = format!("-{:04}.zip", year);
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(&expected_name)
        })
        .with_context(|| format!("source snapshot has no ERA5 archive for {year}"))?;
    let archive_path = artifact_root.join(&artifact.artifact_path);
    let archive_file = File::open(&archive_path)
        .with_context(|| format!("open verified ERA5 archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .context("open verified ERA5 ZIP archive through the portable reader")?;
    if archive.len() != ERA5_ARCHIVE_MEMBERS.len() {
        bail!("ERA5 ZIP archive has an unexpected member count");
    }
    let mut members = Vec::with_capacity(ERA5_ARCHIVE_MEMBERS.len());
    for member_name in ERA5_ARCHIVE_MEMBERS {
        let mut member = archive
            .by_name(member_name)
            .with_context(|| format!("ERA5 ZIP archive is missing member {member_name}"))?;
        let uncompressed_byte_length = member.size();
        if uncompressed_byte_length == 0 {
            bail!("ERA5 ZIP member {member_name} is empty");
        }
        let capacity = usize::try_from(uncompressed_byte_length)
            .context("ERA5 ZIP member is too large for this platform")?;
        let mut bytes = Vec::with_capacity(capacity);
        member
            .read_to_end(&mut bytes)
            .with_context(|| format!("read ERA5 ZIP member {member_name}"))?;
        if u64::try_from(bytes.len())? != uncompressed_byte_length {
            bail!("ERA5 ZIP member {member_name} did not yield its declared byte length");
        }
        let file = NcFile::from_bytes(&bytes)
            .with_context(|| format!("parse ERA5 ZIP member {member_name} as NetCDF"))?;
        validate_era5_member_schema(&file, member_name)?;
        let latitude_endpoint_ieee754_le_hex =
            inspect_etopo_axis_endpoints(&file, "latitude", ERA5_LATITUDE_CELLS, "ERA5 latitude")?;
        let longitude_endpoint_ieee754_le_hex = inspect_etopo_axis_endpoints(
            &file,
            "longitude",
            ERA5_LONGITUDE_CELLS,
            "ERA5 longitude",
        )?;
        let mut variables = file
            .variables()
            .with_context(|| format!("enumerate variables in ERA5 member {member_name}"))?
            .iter()
            .map(|variable| EtopoVariableInspection {
                name: variable.name().to_owned(),
                shape: variable.shape().to_vec(),
            })
            .collect::<Vec<_>>();
        variables.sort_by(|left, right| left.name.cmp(&right.name));
        members.push(Era5MemberInspection {
            name: member_name.to_owned(),
            uncompressed_byte_length,
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            variables,
        });
    }
    println!(
        "{}",
        serde_json::to_string(&Era5AnnualArchiveInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            year,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            members,
        })?
    );
    Ok(())
}

const COPERNICUS_LAND_COVER_MEMBER: &str = "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1.nc";
const COPERNICUS_LAND_COVER_SNAPSHOT_ID: &str = "copernicus-satellite-land-cover-v2-1-1-2022";
const COPERNICUS_LAND_COVER_ARTIFACT_PATH: &str =
    "copernicus-land-cover-2022/copernicus-satellite-land-cover-v2.1.1-2022.zip";
const COPERNICUS_LAND_COVER_ARTIFACT_HASH: &str =
    "993500e18307b5ea0811394355199937b8305081d08b6a7f6909d73a3eadbac7";
const COPERNICUS_LAND_COVER_ARTIFACT_BYTES: u64 = 2_352_123_142;
const COPERNICUS_LAND_COVER_MEMBER_BYTES: u64 = 2_351_763_989;
const COPERNICUS_LAND_COVER_MEMBER_CRC32: u32 = 3_844_043_699;
const COPERNICUS_LAND_COVER_MEMBER_HASH: &str =
    "38149d655e27c0d353dac61eb8e5997cf951566cb52071a3fc4a63b260063e42";
const COPERNICUS_LAND_COVER_LATITUDE_CELLS: u64 = 64_800;
const COPERNICUS_LAND_COVER_LONGITUDE_CELLS: u64 = 129_600;
const COPERNICUS_LAND_COVER_CHUNK_CELLS: u64 = 2_025;
const COPERNICUS_LAND_COVER_LATITUDE_ENDPOINT_BITS: [u64; 2] =
    [0x4056_7fe9_3e93_e940, 0xc056_7fe9_3e93_e93f];
const COPERNICUS_LAND_COVER_LONGITUDE_ENDPOINT_BITS: [u64; 2] =
    [0xc066_7ff4_9f49_f49f, 0x4066_7ff4_9f49_f4a0];

#[derive(Serialize)]
struct CopernicusLandCoverInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    archive_member: String,
    archive_member_byte_length: u64,
    archive_member_crc32: u32,
    archive_member_hash: Digest,
    latitude_endpoint_ieee754_bits_hex: [String; 2],
    longitude_endpoint_ieee754_bits_hex: [String; 2],
    global_attributes: BTreeMap<String, String>,
    variables: Vec<LandCoverVariableInspection>,
}

#[derive(Serialize)]
struct LandCoverVariableInspection {
    name: String,
    data_type: String,
    shape: Vec<u64>,
}

struct VerifiedCopernicusLandCover {
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    archive_member_crc32: u32,
    archive_member_hash: Digest,
    latitude_endpoint_bits: [u64; 2],
    longitude_endpoint_bits: [u64; 2],
    global_attributes: BTreeMap<String, String>,
    file: NcFile,
    _extracted: PartialDownload,
}

#[derive(Serialize)]
struct CopernicusLandCoverCensus {
    census_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    archive_member_hash: Digest,
    latitude_cells: u64,
    longitude_cells: u64,
    raster_cells: u64,
    source_chunk_shape: [u64; 2],
    chunks_scanned: u64,
    lccs_classes: Vec<LccsClassCensus>,
    processed_flag_counts: Vec<RasterValueCount>,
    current_pixel_state_counts: Vec<RasterValueCount>,
    observation_count_counts: Vec<RasterValueCount>,
    change_count_counts: Vec<RasterValueCount>,
}

#[derive(Serialize)]
struct LccsClassCensus {
    value: u8,
    meaning: &'static str,
    cells: u64,
}

#[derive(Serialize)]
struct RasterValueCount {
    value: i64,
    cells: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct CopernicusLandCoverSourceSample {
    latitude_e7: i32,
    longitude_e7: i32,
    source_row: u32,
    source_column: u32,
}

#[derive(Serialize)]
struct CopernicusLandCoverTargetSupportInspection {
    inspection_schema_version: u16,
    target_s2_cell_id: S2CellId,
    target_s2_level: u8,
    sample_policy: String,
    points_per_axis: u8,
    support_samples: u64,
    distinct_source_cells: u64,
    sample_fingerprint: Digest,
    first_sample: CopernicusLandCoverSourceSample,
    last_sample: CopernicusLandCoverSourceSample,
}

fn expected_copernicus_land_cover_variables() -> BTreeMap<&'static str, (NcType, Vec<u64>)> {
    let raster = vec![
        1,
        COPERNICUS_LAND_COVER_LATITUDE_CELLS,
        COPERNICUS_LAND_COVER_LONGITUDE_CELLS,
    ];
    BTreeMap::from([
        ("change_count", (NcType::UByte, raster.clone())),
        ("crs", (NcType::Int, Vec::new())),
        ("current_pixel_state", (NcType::Byte, raster.clone())),
        (
            "lat",
            (NcType::Double, vec![COPERNICUS_LAND_COVER_LATITUDE_CELLS]),
        ),
        (
            "lat_bounds",
            (
                NcType::Double,
                vec![COPERNICUS_LAND_COVER_LATITUDE_CELLS, 2],
            ),
        ),
        ("lccs_class", (NcType::UByte, raster.clone())),
        (
            "lon",
            (NcType::Double, vec![COPERNICUS_LAND_COVER_LONGITUDE_CELLS]),
        ),
        (
            "lon_bounds",
            (
                NcType::Double,
                vec![COPERNICUS_LAND_COVER_LONGITUDE_CELLS, 2],
            ),
        ),
        ("observation_count", (NcType::UShort, raster.clone())),
        ("processed_flag", (NcType::Byte, raster)),
        ("time", (NcType::Double, vec![1])),
        ("time_bounds", (NcType::Double, vec![1, 2])),
    ])
}

fn expected_copernicus_land_cover_global_attributes() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("id", "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1"),
        ("license", "EC C3S Land cover Data Policy"),
        ("product_version", "2.1.1"),
        ("source", "Sentinel-3 OLCI"),
        ("spatial_resolution", "300m"),
        ("time_coverage_end", "20221231"),
        ("time_coverage_start", "20220101"),
        ("tracking_id", "cbc0983e-a0fd-4277-9023-2e618c0c2067"),
    ])
}

fn required_variable_attribute<'a>(
    file: &'a NcFile,
    variable_name: &str,
    attribute_name: &str,
) -> Result<&'a NcAttrValue> {
    file.variable(variable_name)
        .with_context(|| format!("find Copernicus land-cover variable {variable_name}"))?
        .attribute(attribute_name)
        .with_context(|| {
            format!(
                "Copernicus land-cover variable {variable_name} is missing attribute {attribute_name}"
            )
        })
        .map(|attribute| &attribute.value)
}

fn validate_copernicus_land_cover_value_semantics(file: &NcFile) -> Result<()> {
    let expected_class_values = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(value, _)| *value)
        .collect::<Vec<_>>();
    let expected_class_meanings = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(_, meaning)| *meaning)
        .collect::<Vec<_>>()
        .join(" ");
    let expectations = [
        (
            "lccs_class",
            "flag_values",
            NcAttrValue::UBytes(expected_class_values),
        ),
        (
            "lccs_class",
            "flag_meanings",
            NcAttrValue::Strings(vec![expected_class_meanings]),
        ),
        (
            "lccs_class",
            "ancillary_variables",
            NcAttrValue::Strings(vec![
                "processed_flag current_pixel_state observation_count change_count".to_owned(),
            ]),
        ),
        (
            "processed_flag",
            "flag_values",
            NcAttrValue::Bytes(vec![0, 1]),
        ),
        (
            "processed_flag",
            "flag_meanings",
            NcAttrValue::Strings(vec!["not_processed processed".to_owned()]),
        ),
        ("processed_flag", "_FillValue", NcAttrValue::Bytes(vec![-1])),
        (
            "current_pixel_state",
            "flag_values",
            NcAttrValue::Bytes(vec![0, 1, 2, 3, 4, 5]),
        ),
        (
            "current_pixel_state",
            "flag_meanings",
            NcAttrValue::Strings(vec![
                "invalid clear_land clear_water clear_snow_ice cloud cloud_shadow".to_owned(),
            ]),
        ),
        (
            "current_pixel_state",
            "_FillValue",
            NcAttrValue::Bytes(vec![-1]),
        ),
    ];
    for (variable_name, attribute_name, expected) in expectations {
        let observed = required_variable_attribute(file, variable_name, attribute_name)?;
        if observed != &expected {
            bail!(
                "Copernicus land-cover {variable_name}:{attribute_name} changed: expected {expected:?}, observed {observed:?}"
            );
        }
    }
    Ok(())
}

fn validate_copernicus_land_cover_schema(file: &NcFile) -> Result<()> {
    let expected = expected_copernicus_land_cover_variables();
    let observed = file
        .variables()
        .context("enumerate Copernicus land-cover variables")?
        .iter()
        .map(|variable| {
            (
                variable.name(),
                (variable.dtype().clone(), variable.shape().to_vec()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if observed != expected {
        bail!(
            "Copernicus land-cover variable schema changed: expected {expected:?}, observed {observed:?}"
        );
    }
    Ok(())
}

fn netcdf_type_name(value: &NcType) -> Result<&'static str> {
    match value {
        NcType::Byte => Ok("i8"),
        NcType::Char => Ok("char"),
        NcType::Short => Ok("i16"),
        NcType::Int => Ok("i32"),
        NcType::Float => Ok("f32"),
        NcType::Double => Ok("f64"),
        NcType::UByte => Ok("u8"),
        NcType::UShort => Ok("u16"),
        NcType::UInt => Ok("u32"),
        NcType::Int64 => Ok("i64"),
        NcType::UInt64 => Ok("u64"),
        NcType::String => Ok("string"),
        _ => bail!("land-cover schema contains an unsupported compound NetCDF type"),
    }
}

fn required_global_attribute(file: &NcFile, name: &str) -> Result<String> {
    file.global_attributes()
        .context("enumerate Copernicus land-cover global attributes")?
        .iter()
        .find(|attribute| attribute.name == name)
        .with_context(|| format!("Copernicus land-cover file is missing global attribute {name}"))?
        .value
        .as_string()
        .with_context(|| format!("Copernicus land-cover global attribute {name} is not text"))
}

fn inspect_land_cover_axis_endpoints(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
) -> Result<[u64; 2]> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read Copernicus land-cover {variable_name} coordinate axis"))?;
    let values = values.as_slice().with_context(|| {
        format!("Copernicus land-cover {variable_name} coordinate axis is not contiguous")
    })?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("Copernicus land-cover {variable_name} axis has an unexpected length");
    }
    let first = values
        .first()
        .context("Copernicus land-cover coordinate axis is empty")?;
    let last = values
        .last()
        .context("Copernicus land-cover coordinate axis is empty")?;
    Ok([first.to_bits(), last.to_bits()])
}

fn open_verified_copernicus_land_cover(
    manifest_path: &Path,
    artifact_root: &Path,
) -> Result<VerifiedCopernicusLandCover> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    if snapshot.snapshot_id != COPERNICUS_LAND_COVER_SNAPSHOT_ID {
        bail!(
            "expected Copernicus land-cover snapshot {COPERNICUS_LAND_COVER_SNAPSHOT_ID}, observed {}",
            snapshot.snapshot_id
        );
    }
    let source_snapshot_digest = snapshot.content_digest()?;
    let mut data_artifacts = snapshot.artifacts.iter().filter(|artifact| {
        artifact.role == world_data::SourceSnapshotArtifactRole::Data
            && artifact.artifact_path.ends_with(".zip")
    });
    let artifact = data_artifacts
        .next()
        .context("source snapshot has no Copernicus land-cover ZIP response")?;
    if data_artifacts.next().is_some() {
        bail!("Copernicus land-cover source snapshot has multiple data responses");
    }
    let expected_artifact_hash = COPERNICUS_LAND_COVER_ARTIFACT_HASH
        .parse::<Digest>()
        .context("parse pinned Copernicus land-cover artifact digest")?;
    if artifact.artifact_path != COPERNICUS_LAND_COVER_ARTIFACT_PATH
        || artifact.content_hash != expected_artifact_hash
        || artifact.byte_length != COPERNICUS_LAND_COVER_ARTIFACT_BYTES
    {
        bail!("Copernicus land-cover data artifact identity changed");
    }
    let archive_path = artifact_root.join(&artifact.artifact_path);
    let archive_file = File::open(&archive_path).with_context(|| {
        format!(
            "open verified Copernicus land-cover archive {}",
            archive_path.display()
        )
    })?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .context("open verified Copernicus land-cover ZIP archive")?;
    if archive.len() != 1 {
        bail!("Copernicus land-cover ZIP archive has an unexpected member count");
    }
    let mut member = archive
        .by_index(0)
        .context("open Copernicus land-cover ZIP member")?;
    if member.name() != COPERNICUS_LAND_COVER_MEMBER {
        bail!("Copernicus land-cover ZIP member name changed");
    }
    if member.size() != COPERNICUS_LAND_COVER_MEMBER_BYTES {
        bail!("Copernicus land-cover ZIP member byte length changed");
    }
    let archive_member_crc32 = member.crc32();
    if archive_member_crc32 != COPERNICUS_LAND_COVER_MEMBER_CRC32 {
        bail!("Copernicus land-cover ZIP member CRC-32 changed");
    }
    let mut extracted = PartialDownload::create(
        &std::env::temp_dir(),
        "a-tiny-civilization-copernicus-land-cover-inspection.nc",
    )?;
    let mut member_hasher = Sha256::new();
    let mut member_bytes = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = member
            .read(&mut buffer)
            .context("read Copernicus land-cover ZIP member")?;
        if read == 0 {
            break;
        }
        member_bytes = member_bytes
            .checked_add(u64::try_from(read)?)
            .context("Copernicus land-cover member length overflow")?;
        member_hasher.update(&buffer[..read]);
        extracted
            .file
            .write_all(&buffer[..read])
            .context("write temporary Copernicus land-cover NetCDF")?;
    }
    if member_bytes != COPERNICUS_LAND_COVER_MEMBER_BYTES {
        bail!("Copernicus land-cover ZIP member yielded an unexpected byte length");
    }
    extracted
        .file
        .sync_all()
        .context("sync temporary Copernicus land-cover NetCDF")?;
    let archive_member_hash = Digest::from_bytes(member_hasher.finalize().into());
    let expected_member_hash = COPERNICUS_LAND_COVER_MEMBER_HASH
        .parse::<Digest>()
        .context("parse pinned Copernicus land-cover member digest")?;
    if archive_member_hash != expected_member_hash {
        bail!("Copernicus land-cover ZIP member SHA-256 changed");
    }
    drop(member);
    drop(archive);

    let file = NcFile::open(&extracted.path)
        .context("parse Copernicus land-cover NetCDF through the pure-Rust reader")?;
    validate_copernicus_land_cover_schema(&file)?;
    validate_copernicus_land_cover_value_semantics(&file)?;
    let latitude_endpoint_bits =
        inspect_land_cover_axis_endpoints(&file, "lat", COPERNICUS_LAND_COVER_LATITUDE_CELLS)?;
    if latitude_endpoint_bits != COPERNICUS_LAND_COVER_LATITUDE_ENDPOINT_BITS {
        bail!("Copernicus land-cover latitude endpoints changed");
    }
    let longitude_endpoint_bits =
        inspect_land_cover_axis_endpoints(&file, "lon", COPERNICUS_LAND_COVER_LONGITUDE_CELLS)?;
    if longitude_endpoint_bits != COPERNICUS_LAND_COVER_LONGITUDE_ENDPOINT_BITS {
        bail!("Copernicus land-cover longitude endpoints changed");
    }
    let expected_global_attributes = expected_copernicus_land_cover_global_attributes();
    let global_attributes = expected_global_attributes
        .keys()
        .map(|name| Ok(((*name).to_owned(), required_global_attribute(&file, name)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let observed_global_attributes = global_attributes
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<BTreeMap<_, _>>();
    if observed_global_attributes != expected_global_attributes {
        bail!(
            "Copernicus land-cover global attributes changed: expected {expected_global_attributes:?}, observed {observed_global_attributes:?}"
        );
    }
    Ok(VerifiedCopernicusLandCover {
        source_snapshot_id: snapshot.snapshot_id,
        source_snapshot_digest,
        artifact_path: artifact.artifact_path.clone(),
        artifact_hash: artifact.content_hash,
        artifact_byte_length: artifact.byte_length,
        archive_member_crc32,
        archive_member_hash,
        latitude_endpoint_bits,
        longitude_endpoint_bits,
        global_attributes,
        file,
        _extracted: extracted,
    })
}

fn inspect_copernicus_land_cover(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let mut variables = source
        .file
        .variables()
        .context("enumerate Copernicus land-cover variables")?
        .iter()
        .map(|variable| {
            Ok(LandCoverVariableInspection {
                name: variable.name().to_owned(),
                data_type: netcdf_type_name(variable.dtype())?.to_owned(),
                shape: variable.shape().to_vec(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverInspection {
            inspection_schema_version: 1,
            source_snapshot_id: source.source_snapshot_id.clone(),
            source_snapshot_digest: source.source_snapshot_digest,
            artifact_path: source.artifact_path.clone(),
            artifact_hash: source.artifact_hash,
            artifact_byte_length: source.artifact_byte_length,
            archive_member: COPERNICUS_LAND_COVER_MEMBER.to_owned(),
            archive_member_byte_length: COPERNICUS_LAND_COVER_MEMBER_BYTES,
            archive_member_crc32: source.archive_member_crc32,
            archive_member_hash: source.archive_member_hash,
            latitude_endpoint_ieee754_bits_hex: source
                .latitude_endpoint_bits
                .map(|value| format!("{value:016x}")),
            longitude_endpoint_ieee754_bits_hex: source
                .longitude_endpoint_bits
                .map(|value| format!("{value:016x}")),
            global_attributes: source.global_attributes.clone(),
            variables,
        })?
    );
    Ok(())
}

fn copernicus_land_cover_chunk_selection(chunk_row: u64, chunk_column: u64) -> NcSliceInfo {
    let latitude_start = chunk_row * COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let longitude_start = chunk_column * COPERNICUS_LAND_COVER_CHUNK_CELLS;
    NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Index(0),
            NcSliceInfoElem::Slice {
                start: latitude_start,
                end: latitude_start + COPERNICUS_LAND_COVER_CHUNK_CELLS,
                step: 1,
            },
            NcSliceInfoElem::Slice {
                start: longitude_start,
                end: longitude_start + COPERNICUS_LAND_COVER_CHUNK_CELLS,
                step: 1,
            },
        ],
    }
}

fn accumulate_u8_counts(values: &[u8], counts: &mut [u64; 256]) {
    for value in values {
        counts[usize::from(*value)] += 1;
    }
}

fn accumulate_i8_counts(values: &[i8], counts: &mut [u64; 256]) {
    for value in values {
        let index = usize::try_from(i16::from(*value) + 128).expect("i8 offset fits usize");
        counts[index] += 1;
    }
}

fn accumulate_u16_counts(values: &[u16], counts: &mut [u64]) {
    debug_assert_eq!(counts.len(), usize::from(u16::MAX) + 1);
    for value in values {
        counts[usize::from(*value)] += 1;
    }
}

fn raster_value_counts_u8(counts: &[u64; 256]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(value, cells)| RasterValueCount {
            value: i64::try_from(value).expect("u8 index fits i64"),
            cells: *cells,
        })
        .collect()
}

fn raster_value_counts_i8(counts: &[u64; 256]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(index, cells)| RasterValueCount {
            value: i64::try_from(index).expect("i8 count index fits i64") - 128,
            cells: *cells,
        })
        .collect()
}

fn raster_value_counts_u16(counts: &[u64]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(value, cells)| RasterValueCount {
            value: i64::try_from(value).expect("u16 index fits i64"),
            cells: *cells,
        })
        .collect()
}

fn require_census_total(label: &str, counts: &[u64], expected: u64) -> Result<()> {
    let observed = counts.iter().try_fold(0_u64, |total, count| {
        total.checked_add(*count).context("census total overflow")
    })?;
    if observed != expected {
        bail!("{label} census covered {observed} cells instead of {expected}");
    }
    Ok(())
}

fn inspect_copernicus_land_cover_census(
    manifest_path: &Path,
    artifact_root: &Path,
    output_path: Option<&Path>,
) -> Result<()> {
    if !COPERNICUS_LAND_COVER_LATITUDE_CELLS.is_multiple_of(COPERNICUS_LAND_COVER_CHUNK_CELLS)
        || !COPERNICUS_LAND_COVER_LONGITUDE_CELLS.is_multiple_of(COPERNICUS_LAND_COVER_CHUNK_CELLS)
    {
        bail!("pinned Copernicus land-cover grid is not divisible by its source chunk shape");
    }
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let raster_cells = COPERNICUS_LAND_COVER_LATITUDE_CELLS
        .checked_mul(COPERNICUS_LAND_COVER_LONGITUDE_CELLS)
        .context("Copernicus land-cover raster cell count overflow")?;
    let chunk_rows = COPERNICUS_LAND_COVER_LATITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let chunk_columns = COPERNICUS_LAND_COVER_LONGITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let chunks_scanned = chunk_rows
        .checked_mul(chunk_columns)
        .context("Copernicus land-cover chunk count overflow")?;
    let expected_chunk_cells =
        usize::try_from(COPERNICUS_LAND_COVER_CHUNK_CELLS * COPERNICUS_LAND_COVER_CHUNK_CELLS)?;
    let mut lccs_counts = [0_u64; 256];
    let mut processed_counts = [0_u64; 256];
    let mut state_counts = [0_u64; 256];
    let mut observation_counts = vec![0_u64; usize::from(u16::MAX) + 1];
    let mut change_counts = [0_u64; 256];

    for chunk_row in 0..chunk_rows {
        for chunk_column in 0..chunk_columns {
            let selection = copernicus_land_cover_chunk_selection(chunk_row, chunk_column);
            let classes = source
                .file
                .read_variable_slice::<u8>("lccs_class", &selection)
                .context("read Copernicus lccs_class source chunk")?;
            let processed = source
                .file
                .read_variable_slice::<i8>("processed_flag", &selection)
                .context("read Copernicus processed_flag source chunk")?;
            let states = source
                .file
                .read_variable_slice::<i8>("current_pixel_state", &selection)
                .context("read Copernicus current_pixel_state source chunk")?;
            let observations = source
                .file
                .read_variable_slice::<u16>("observation_count", &selection)
                .context("read Copernicus observation_count source chunk")?;
            let changes = source
                .file
                .read_variable_slice::<u8>("change_count", &selection)
                .context("read Copernicus change_count source chunk")?;
            let classes = classes
                .as_slice()
                .context("Copernicus lccs_class chunk is not contiguous")?;
            let processed = processed
                .as_slice()
                .context("Copernicus processed_flag chunk is not contiguous")?;
            let states = states
                .as_slice()
                .context("Copernicus current_pixel_state chunk is not contiguous")?;
            let observations = observations
                .as_slice()
                .context("Copernicus observation_count chunk is not contiguous")?;
            let changes = changes
                .as_slice()
                .context("Copernicus change_count chunk is not contiguous")?;
            for (name, length) in [
                ("lccs_class", classes.len()),
                ("processed_flag", processed.len()),
                ("current_pixel_state", states.len()),
                ("observation_count", observations.len()),
                ("change_count", changes.len()),
            ] {
                if length != expected_chunk_cells {
                    bail!(
                        "Copernicus {name} chunk yielded {length} cells instead of {expected_chunk_cells}"
                    );
                }
            }
            accumulate_u8_counts(classes, &mut lccs_counts);
            accumulate_i8_counts(processed, &mut processed_counts);
            accumulate_i8_counts(states, &mut state_counts);
            accumulate_u16_counts(observations, &mut observation_counts);
            accumulate_u8_counts(changes, &mut change_counts);
        }
        eprintln!(
            "censused Copernicus land-cover source chunk row {}/{}",
            chunk_row + 1,
            chunk_rows
        );
    }

    require_census_total("lccs_class", &lccs_counts, raster_cells)?;
    require_census_total("processed_flag", &processed_counts, raster_cells)?;
    require_census_total("current_pixel_state", &state_counts, raster_cells)?;
    require_census_total("observation_count", &observation_counts, raster_cells)?;
    require_census_total("change_count", &change_counts, raster_cells)?;
    let allowed_classes = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(value, _)| *value)
        .collect::<std::collections::BTreeSet<_>>();
    for (value, cells) in lccs_counts.iter().enumerate() {
        if *cells != 0 && !allowed_classes.contains(&u8::try_from(value)?) {
            bail!("Copernicus lccs_class contains unsupported value {value}");
        }
    }
    for value in -128_i16..=127_i16 {
        let cells = processed_counts[usize::try_from(value + 128)?];
        if cells != 0 && ![-1_i16, 0, 1].contains(&value) {
            bail!("Copernicus processed_flag contains unsupported value {value}");
        }
        let cells = state_counts[usize::try_from(value + 128)?];
        if cells != 0 && ![-1_i16, 0, 1, 2, 3, 4, 5].contains(&value) {
            bail!("Copernicus current_pixel_state contains unsupported value {value}");
        }
    }
    if let Some((value, _)) = observation_counts
        .iter()
        .enumerate()
        .find(|(value, cells)| **cells != 0 && *value > 32_767)
    {
        bail!("Copernicus observation_count contains unsupported value {value}");
    }
    if let Some((value, _)) = change_counts
        .iter()
        .enumerate()
        .find(|(value, cells)| **cells != 0 && *value > 100)
    {
        bail!("Copernicus change_count contains unsupported value {value}");
    }

    let census = CopernicusLandCoverCensus {
        census_schema_version: 1,
        source_snapshot_id: source.source_snapshot_id.clone(),
        source_snapshot_digest: source.source_snapshot_digest,
        archive_member_hash: source.archive_member_hash,
        latitude_cells: COPERNICUS_LAND_COVER_LATITUDE_CELLS,
        longitude_cells: COPERNICUS_LAND_COVER_LONGITUDE_CELLS,
        raster_cells,
        source_chunk_shape: [
            COPERNICUS_LAND_COVER_CHUNK_CELLS,
            COPERNICUS_LAND_COVER_CHUNK_CELLS,
        ],
        chunks_scanned,
        lccs_classes: COPERNICUS_LCCS_CLASSES
            .iter()
            .map(|(value, meaning)| LccsClassCensus {
                value: *value,
                meaning,
                cells: lccs_counts[usize::from(*value)],
            })
            .collect(),
        processed_flag_counts: raster_value_counts_i8(&processed_counts),
        current_pixel_state_counts: raster_value_counts_i8(&state_counts),
        observation_count_counts: raster_value_counts_u16(&observation_counts),
        change_count_counts: raster_value_counts_u8(&change_counts),
    };
    if let Some(output_path) = output_path {
        let mut pretty = serde_json::to_vec_pretty(&census)?;
        pretty.push(b'\n');
        write_new_artifact(output_path, &pretty)?;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "census_schema_version": 1,
                "output_path": output_path,
                "output_hash": Digest::sha256(&pretty),
                "raster_cells": raster_cells,
                "chunks_scanned": chunks_scanned,
            }))?
        );
    } else {
        println!("{}", serde_json::to_string(&census)?);
    }
    Ok(())
}

fn interpolate_s2_face_uv_midpoint(
    lower: S2FaceUv,
    upper: S2FaceUv,
    index: u8,
    points_per_axis: u8,
    use_u_axis: bool,
) -> Result<(i128, i128)> {
    if points_per_axis == 0 || points_per_axis > 64 || index >= points_per_axis {
        bail!("Copernicus target-support quadrature must use 1..=64 points per axis");
    }
    if lower.face != upper.face || lower.denominator != upper.denominator {
        bail!("S2 target-support vertices do not share one face projection");
    }
    let high_weight = i128::from(index)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .context("target-support midpoint weight overflow")?;
    let denominator_weight = i128::from(points_per_axis)
        .checked_mul(2)
        .context("target-support denominator weight overflow")?;
    let low_weight = denominator_weight
        .checked_sub(high_weight)
        .context("target-support low weight underflow")?;
    let (low, high) = if use_u_axis {
        (lower.u_numerator, upper.u_numerator)
    } else {
        (lower.v_numerator, upper.v_numerator)
    };
    let numerator = low
        .checked_mul(low_weight)
        .and_then(|value| {
            high.checked_mul(high_weight)
                .and_then(|high| value.checked_add(high))
        })
        .context("target-support interpolation overflow")?;
    let denominator = lower
        .denominator
        .checked_mul(denominator_weight)
        .context("target-support denominator overflow")?;
    Ok((numerator, denominator))
}

fn copernicus_source_area_cell(coordinate: GeographicCoordinateE7) -> Result<(u32, u32)> {
    const E7_PER_DEGREE: i64 = 10_000_000;
    const SOURCE_CELLS_PER_DEGREE: i64 = 360;
    let latitude_distance_from_north = 900_000_000_i64
        .checked_sub(i64::from(coordinate.latitude_e7()))
        .context("Copernicus latitude lookup overflow")?;
    let mut row = latitude_distance_from_north
        .checked_mul(SOURCE_CELLS_PER_DEGREE)
        .context("Copernicus latitude lookup overflow")?
        / E7_PER_DEGREE;
    if row == i64::try_from(COPERNICUS_LAND_COVER_LATITUDE_CELLS)? {
        row -= 1;
    }
    let longitude_distance_from_west = i64::from(coordinate.longitude_e7())
        .checked_add(1_800_000_000)
        .context("Copernicus longitude lookup overflow")?;
    let column = longitude_distance_from_west
        .checked_mul(SOURCE_CELLS_PER_DEGREE)
        .context("Copernicus longitude lookup overflow")?
        / E7_PER_DEGREE;
    if row < 0
        || row >= i64::try_from(COPERNICUS_LAND_COVER_LATITUDE_CELLS)?
        || column < 0
        || column >= i64::try_from(COPERNICUS_LAND_COVER_LONGITUDE_CELLS)?
    {
        bail!("geographic coordinate does not select a Copernicus source area cell");
    }
    Ok((u32::try_from(row)?, u32::try_from(column)?))
}

fn copernicus_land_cover_target_support_samples(
    target: S2CellId,
    points_per_axis: u8,
) -> Result<Vec<CopernicusLandCoverSourceSample>> {
    if points_per_axis == 0 || points_per_axis > 64 {
        bail!("Copernicus target-support quadrature must use 1..=64 points per axis");
    }
    let ij = decode_s2_face_ij(target);
    let upper_i = ij.i.checked_add(1).context("S2 target i overflow")?;
    let upper_j = ij.j.checked_add(1).context("S2 target j overflow")?;
    let lower = s2_face_ij_vertex_uv(ij, ij.i, ij.j)?;
    let upper = s2_face_ij_vertex_uv(ij, upper_i, upper_j)?;
    let sample_count = usize::from(points_per_axis)
        .checked_mul(usize::from(points_per_axis))
        .context("Copernicus target-support sample count overflow")?;
    let mut samples = Vec::with_capacity(sample_count);
    let u_midpoints = (0..points_per_axis)
        .map(|index| interpolate_s2_face_uv_midpoint(lower, upper, index, points_per_axis, true))
        .collect::<Result<Vec<_>>>()?;
    let v_midpoints = (0..points_per_axis)
        .map(|index| interpolate_s2_face_uv_midpoint(lower, upper, index, points_per_axis, false))
        .collect::<Result<Vec<_>>>()?;
    for (v_numerator, denominator) in v_midpoints {
        for (u_numerator, u_denominator) in &u_midpoints {
            if *u_denominator != denominator {
                bail!("target-support axes produced different denominators");
            }
            let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(S2FaceUv {
                face: ij.face,
                u_numerator: *u_numerator,
                v_numerator,
                denominator,
            })?)?;
            let (source_row, source_column) = copernicus_source_area_cell(coordinate)?;
            samples.push(CopernicusLandCoverSourceSample {
                latitude_e7: coordinate.latitude_e7(),
                longitude_e7: coordinate.longitude_e7(),
                source_row,
                source_column,
            });
        }
    }
    Ok(samples)
}

fn copernicus_target_support_fingerprint(samples: &[CopernicusLandCoverSourceSample]) -> Digest {
    let mut bytes = Vec::with_capacity(samples.len() * 16);
    for sample in samples {
        bytes.extend_from_slice(&sample.latitude_e7.to_le_bytes());
        bytes.extend_from_slice(&sample.longitude_e7.to_le_bytes());
        bytes.extend_from_slice(&sample.source_row.to_le_bytes());
        bytes.extend_from_slice(&sample.source_column.to_le_bytes());
    }
    Digest::sha256(&bytes)
}

fn inspect_copernicus_land_cover_target_support(
    target: S2CellId,
    points_per_axis: u8,
) -> Result<()> {
    let samples = copernicus_land_cover_target_support_samples(target, points_per_axis)?;
    let distinct_source_cells = samples
        .iter()
        .map(|sample| (sample.source_row, sample.source_column))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverTargetSupportInspection {
            inspection_schema_version: 1,
            target_s2_cell_id: target,
            target_s2_level: target.level(),
            sample_policy: format!("s2-face-uv-q{points_per_axis}-e7-source-area-v1"),
            points_per_axis,
            support_samples: u64::try_from(samples.len())?,
            distinct_source_cells: u64::try_from(distinct_source_cells)?,
            sample_fingerprint: copernicus_target_support_fingerprint(&samples),
            first_sample: *samples.first().context("target support is empty")?,
            last_sample: *samples.last().context("target support is empty")?,
        })?
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct CopernicusLandCoverSourceValue {
    class_value: u8,
    processed_flag: i8,
    current_pixel_state: i8,
    observation_count: u16,
    change_count: u8,
}

struct CopernicusLandCoverSourceChunk {
    last_used: u64,
    classes: Vec<u8>,
    processed_flags: Vec<i8>,
    current_pixel_states: Vec<i8>,
    observation_counts: Vec<u16>,
    change_counts: Vec<u8>,
}

trait CopernicusLandCoverLookup {
    fn lookup(&mut self, row: u32, column: u32) -> Result<CopernicusLandCoverSourceValue>;
}

struct CopernicusLandCoverChunkCache<'a> {
    file: &'a NcFile,
    capacity: usize,
    access_clock: u64,
    chunks_loaded: u64,
    cache_hits: u64,
    chunks: HashMap<(u32, u32), CopernicusLandCoverSourceChunk>,
}

impl<'a> CopernicusLandCoverChunkCache<'a> {
    fn new(file: &'a NcFile, capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > 64 {
            bail!("Copernicus source chunk cache must retain 1 through 64 chunks");
        }
        Ok(Self {
            file,
            capacity,
            access_clock: 0,
            chunks_loaded: 0,
            cache_hits: 0,
            chunks: HashMap::new(),
        })
    }

    fn load_chunk(
        &self,
        chunk_row: u32,
        chunk_column: u32,
    ) -> Result<CopernicusLandCoverSourceChunk> {
        Ok(CopernicusLandCoverSourceChunk {
            last_used: 0,
            classes: read_copernicus_source_chunk::<u8>(
                self.file,
                "lccs_class",
                chunk_row,
                chunk_column,
            )?,
            processed_flags: read_copernicus_source_chunk::<i8>(
                self.file,
                "processed_flag",
                chunk_row,
                chunk_column,
            )?,
            current_pixel_states: read_copernicus_source_chunk::<i8>(
                self.file,
                "current_pixel_state",
                chunk_row,
                chunk_column,
            )?,
            observation_counts: read_copernicus_source_chunk::<u16>(
                self.file,
                "observation_count",
                chunk_row,
                chunk_column,
            )?,
            change_counts: read_copernicus_source_chunk::<u8>(
                self.file,
                "change_count",
                chunk_row,
                chunk_column,
            )?,
        })
    }
}

impl CopernicusLandCoverLookup for CopernicusLandCoverChunkCache<'_> {
    fn lookup(&mut self, row: u32, column: u32) -> Result<CopernicusLandCoverSourceValue> {
        if u64::from(row) >= COPERNICUS_LAND_COVER_LATITUDE_CELLS
            || u64::from(column) >= COPERNICUS_LAND_COVER_LONGITUDE_CELLS
        {
            bail!("Copernicus source lookup is outside the global raster");
        }
        let chunk_size = u32::try_from(COPERNICUS_LAND_COVER_CHUNK_CELLS)?;
        let key = (row / chunk_size, column / chunk_size);
        let cached = self.chunks.contains_key(&key);
        if !cached {
            if self.chunks.len() == self.capacity {
                let oldest = self
                    .chunks
                    .iter()
                    .min_by_key(|(_, chunk)| chunk.last_used)
                    .map(|(key, _)| *key)
                    .context("nonempty Copernicus source cache has no oldest chunk")?;
                self.chunks.remove(&oldest);
            }
            let chunk = self.load_chunk(key.0, key.1)?;
            self.chunks.insert(key, chunk);
            self.chunks_loaded = self
                .chunks_loaded
                .checked_add(1)
                .context("Copernicus source chunk load count overflow")?;
        } else {
            self.cache_hits = self
                .cache_hits
                .checked_add(1)
                .context("Copernicus source cache hit count overflow")?;
        }
        self.access_clock = self
            .access_clock
            .checked_add(1)
            .context("Copernicus source cache clock overflow")?;
        let chunk = self
            .chunks
            .get_mut(&key)
            .context("Copernicus source chunk disappeared from cache")?;
        chunk.last_used = self.access_clock;
        let local_row = usize::try_from(row % chunk_size)?;
        let local_column = usize::try_from(column % chunk_size)?;
        let index = local_row
            .checked_mul(usize::try_from(chunk_size)?)
            .and_then(|value| value.checked_add(local_column))
            .context("Copernicus source chunk index overflow")?;
        Ok(CopernicusLandCoverSourceValue {
            class_value: *chunk
                .classes
                .get(index)
                .context("Copernicus class chunk index is missing")?,
            processed_flag: *chunk
                .processed_flags
                .get(index)
                .context("Copernicus processed chunk index is missing")?,
            current_pixel_state: *chunk
                .current_pixel_states
                .get(index)
                .context("Copernicus pixel-state chunk index is missing")?,
            observation_count: *chunk
                .observation_counts
                .get(index)
                .context("Copernicus observation chunk index is missing")?,
            change_count: *chunk
                .change_counts
                .get(index)
                .context("Copernicus change chunk index is missing")?,
        })
    }
}

fn read_copernicus_source_chunk<T>(
    file: &NcFile,
    variable_name: &str,
    chunk_row: u32,
    chunk_column: u32,
) -> Result<Vec<T>>
where
    T: netcdf_reader::NcReadable + Clone,
{
    let chunk_size = COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let latitude_start = u64::from(chunk_row)
        .checked_mul(chunk_size)
        .context("Copernicus source chunk latitude overflow")?;
    let longitude_start = u64::from(chunk_column)
        .checked_mul(chunk_size)
        .context("Copernicus source chunk longitude overflow")?;
    if latitude_start + chunk_size > COPERNICUS_LAND_COVER_LATITUDE_CELLS
        || longitude_start + chunk_size > COPERNICUS_LAND_COVER_LONGITUDE_CELLS
    {
        bail!("Copernicus source chunk address is outside the pinned raster");
    }
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Index(0),
            NcSliceInfoElem::Slice {
                start: latitude_start,
                end: latitude_start + chunk_size,
                step: 1,
            },
            NcSliceInfoElem::Slice {
                start: longitude_start,
                end: longitude_start + chunk_size,
                step: 1,
            },
        ],
    };
    let values = file
        .read_variable_slice::<T>(variable_name, &selection)
        .with_context(|| {
            format!("read Copernicus {variable_name} source chunk {chunk_row},{chunk_column}")
        })?;
    let values = values
        .as_slice()
        .with_context(|| format!("Copernicus {variable_name} source chunk is not contiguous"))?;
    let expected = usize::try_from(chunk_size * chunk_size)?;
    if values.len() != expected {
        bail!("Copernicus {variable_name} source chunk has an unexpected cell count");
    }
    Ok(values.to_vec())
}

struct CopernicusLandCoverCellAccumulator {
    support_samples: u64,
    class_counts: [u64; 256],
    processed_flag_counts: [u64; 3],
    current_pixel_state_counts: [u64; 7],
    observation_count_minimum: u16,
    observation_count_sum: u64,
    observation_count_maximum: u16,
    change_count_minimum: u8,
    change_count_sum: u64,
    change_count_maximum: u8,
}

impl Default for CopernicusLandCoverCellAccumulator {
    fn default() -> Self {
        Self {
            support_samples: 0,
            class_counts: [0; 256],
            processed_flag_counts: [0; 3],
            current_pixel_state_counts: [0; 7],
            observation_count_minimum: u16::MAX,
            observation_count_sum: 0,
            observation_count_maximum: 0,
            change_count_minimum: u8::MAX,
            change_count_sum: 0,
            change_count_maximum: 0,
        }
    }
}

impl CopernicusLandCoverCellAccumulator {
    fn add(&mut self, value: CopernicusLandCoverSourceValue) -> Result<()> {
        if !COPERNICUS_LCCS_CLASSES
            .iter()
            .any(|(class_value, _)| *class_value == value.class_value)
        {
            bail!(
                "sampled unsupported Copernicus LCCS value {}",
                value.class_value
            );
        }
        if !(-1..=1).contains(&value.processed_flag)
            || !(-1..=5).contains(&value.current_pixel_state)
            || value.observation_count > 32_767
            || value.change_count > 100
        {
            bail!("sampled Copernicus classification quality is outside its pinned domain");
        }
        self.support_samples = self
            .support_samples
            .checked_add(1)
            .context("land-cover target support overflow")?;
        self.class_counts[usize::from(value.class_value)] += 1;
        self.processed_flag_counts[usize::try_from(i16::from(value.processed_flag) + 1)?] += 1;
        self.current_pixel_state_counts
            [usize::try_from(i16::from(value.current_pixel_state) + 1)?] += 1;
        self.observation_count_minimum =
            self.observation_count_minimum.min(value.observation_count);
        self.observation_count_sum = self
            .observation_count_sum
            .checked_add(u64::from(value.observation_count))
            .context("land-cover observation-count sum overflow")?;
        self.observation_count_maximum =
            self.observation_count_maximum.max(value.observation_count);
        self.change_count_minimum = self.change_count_minimum.min(value.change_count);
        self.change_count_sum = self
            .change_count_sum
            .checked_add(u64::from(value.change_count))
            .context("land-cover change-count sum overflow")?;
        self.change_count_maximum = self.change_count_maximum.max(value.change_count);
        Ok(())
    }

    fn finish(self, s2_cell_id: S2CellId) -> Result<LandCoverEvidenceCell> {
        if self.support_samples == 0 {
            bail!("land-cover target cell has no support samples");
        }
        Ok(LandCoverEvidenceCell {
            s2_cell_id,
            support_samples: self.support_samples,
            class_counts: COPERNICUS_LCCS_CLASSES
                .iter()
                .filter_map(|(class_value, _)| {
                    let samples = self.class_counts[usize::from(*class_value)];
                    (samples != 0).then_some(LandCoverClassCount {
                        class_value: *class_value,
                        samples,
                    })
                })
                .collect(),
            processed_flag_counts: self
                .processed_flag_counts
                .iter()
                .enumerate()
                .filter_map(|(index, samples)| {
                    (*samples != 0).then_some(LandCoverSignedValueCount {
                        value: i8::try_from(index).expect("processed index fits i8") - 1,
                        samples: *samples,
                    })
                })
                .collect(),
            current_pixel_state_counts: self
                .current_pixel_state_counts
                .iter()
                .enumerate()
                .filter_map(|(index, samples)| {
                    (*samples != 0).then_some(LandCoverSignedValueCount {
                        value: i8::try_from(index).expect("pixel-state index fits i8") - 1,
                        samples: *samples,
                    })
                })
                .collect(),
            observation_count_minimum: self.observation_count_minimum,
            observation_count_sum: self.observation_count_sum,
            observation_count_maximum: self.observation_count_maximum,
            change_count_minimum: self.change_count_minimum,
            change_count_sum: self.change_count_sum,
            change_count_maximum: self.change_count_maximum,
        })
    }
}

#[derive(Serialize)]
struct CopernicusLandCoverCellEvidenceInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    sample_policy: String,
    cell_fingerprint: Digest,
    source_chunks_loaded: u64,
    source_chunk_cache_hits: u64,
    cell: LandCoverEvidenceCell,
}

fn inspect_copernicus_land_cover_cell_evidence(
    manifest_path: &Path,
    artifact_root: &Path,
    target: S2CellId,
    points_per_axis: u8,
) -> Result<()> {
    if target.level() != 10 || points_per_axis != 32 {
        bail!("Copernicus observed-land-cover v1 cell evidence requires an L10 cell and q32");
    }
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let mut cache = CopernicusLandCoverChunkCache::new(&source.file, 4)?;
    let mut accumulator = CopernicusLandCoverCellAccumulator::default();
    for sample in copernicus_land_cover_target_support_samples(target, points_per_axis)? {
        accumulator.add(cache.lookup(sample.source_row, sample.source_column)?)?;
    }
    let cell = accumulator.finish(target)?;
    let cell_fingerprint = Digest::canonical(&cell)?;
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverCellEvidenceInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: source.source_snapshot_digest,
            source_artifact_digest: source.artifact_hash,
            sample_policy: copernicus_land_cover_sample_policy(points_per_axis),
            cell_fingerprint,
            source_chunks_loaded: cache.chunks_loaded,
            source_chunk_cache_hits: cache.cache_hits,
            cell,
        })?
    );
    Ok(())
}

fn copernicus_land_cover_sample_policy(points_per_axis: u8) -> String {
    format!("s2-face-uv-q{points_per_axis}-e7-source-area-v1")
}

fn pack_copernicus_land_cover_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
    points_per_axis: u8,
    lookup: &mut impl CopernicusLandCoverLookup,
) -> Result<PackedLandCoverEvidenceTile> {
    let target_cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?;
    let parallel_samples = target_cells
        .par_iter()
        .map(|target| copernicus_land_cover_target_support_samples(*target, points_per_axis))
        .collect::<Vec<_>>();
    let sample_sets = parallel_samples.into_iter().collect::<Result<Vec<_>>>()?;
    let mut cells = Vec::with_capacity(target_cells.len());
    for (target, samples) in target_cells.into_iter().zip(sample_sets) {
        let mut accumulator = CopernicusLandCoverCellAccumulator::default();
        for sample in samples {
            accumulator.add(lookup.lookup(sample.source_row, sample.source_column)?)?;
        }
        cells.push(accumulator.finish(target)?);
    }
    let tile = PackedLandCoverEvidenceTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        source_snapshot_digest,
        source_artifact_digest,
        sample_policy: copernicus_land_cover_sample_policy(points_per_axis),
        quadrature_points_per_axis: points_per_axis,
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed Copernicus land-cover tile is invalid")?;
    Ok(tile)
}

#[derive(Clone, Copy)]
struct CopernicusLandCoverPackingProfile<'a> {
    layer_id: &'a str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
}

fn validate_resumable_copernicus_land_cover_tile(
    bytes: &[u8],
    profile: CopernicusLandCoverPackingProfile<'_>,
    container: S2CellId,
) -> Result<()> {
    let tile = PackedLandCoverEvidenceTile::from_canonical_slice(bytes)
        .context("decode staged Copernicus land-cover tile")?;
    if tile.layer_id != profile.layer_id
        || tile.source_snapshot_digest != profile.source_snapshot_digest
        || tile.source_artifact_digest != profile.source_artifact_digest
        || tile.sample_policy != copernicus_land_cover_sample_policy(profile.points_per_axis)
        || tile.quadrature_points_per_axis != profile.points_per_axis
        || tile.container_s2_cell_id != container
        || tile.target_s2_level != profile.target_s2_level
    {
        bail!("staged Copernicus land-cover tile does not match the requested derivation");
    }
    Ok(())
}

fn write_packed_copernicus_land_cover_layer(
    output_directory: &Path,
    profile: CopernicusLandCoverPackingProfile<'_>,
    lookup: &mut impl CopernicusLandCoverLookup,
) -> Result<(String, Vec<u8>, u64)> {
    let level_directory = format!("l{}", profile.container_s2_level);
    let tile_directory = output_directory
        .join("layers")
        .join(profile.layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let containers = global_s2_cells_at_level(profile.container_s2_level)?;
    let mut entries = Vec::with_capacity(containers.len());
    let mut target_cells = 0_u64;
    for (position, container) in containers.into_iter().enumerate() {
        let relative_path = format!(
            "layers/{}/{level_directory}/{container}.tile",
            profile.layer_id
        );
        let artifact_path = output_directory.join(&relative_path);
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_copernicus_land_cover_tile(&existing, profile, container)?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let tile = pack_copernicus_land_cover_tile(
                    profile.layer_id,
                    profile.source_snapshot_digest,
                    profile.source_artifact_digest,
                    container,
                    profile.target_s2_level,
                    profile.points_per_axis,
                    lookup,
                )?;
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged Copernicus land-cover tile"),
        };
        let tile = PackedLandCoverEvidenceTile::from_canonical_slice(&bytes)?;
        target_cells = target_cells
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("Copernicus land-cover target-cell count overflow")?;
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: profile.container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_LAND_COVER_EVIDENCE_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 256 == 0 {
            eprintln!(
                "Copernicus land-cover normalization progress: {}/24576 containers",
                position + 1
            );
        }
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: profile.layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{}/root.index", profile.layer_id);
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged Copernicus land-cover root does not match this derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged Copernicus land-cover root"),
    }
    Ok((root_relative_path, root_bytes, target_cells))
}

#[derive(Serialize)]
struct CopernicusLandCoverLayerDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: String,
    points_per_axis: u8,
    target_cells: u64,
    target_support_samples: u64,
    source_chunks_loaded: u64,
    source_chunk_cache_hits: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

struct CopernicusLandCoverDerivationOptions<'a> {
    manifest_path: &'a Path,
    artifact_root: &'a Path,
    layer_id: &'a str,
    output_directory: &'a Path,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
    source_chunk_cache: usize,
}

fn derive_copernicus_land_cover_layer(
    options: CopernicusLandCoverDerivationOptions<'_>,
) -> Result<()> {
    let CopernicusLandCoverDerivationOptions {
        manifest_path,
        artifact_root,
        layer_id,
        output_directory,
        container_s2_level,
        target_s2_level,
        points_per_axis,
        source_chunk_cache,
    } = options;
    if container_s2_level != 6 || target_s2_level != 10 || points_per_axis != 32 {
        bail!("Copernicus observed-land-cover v1 requires L6→L10 packing and q32 support");
    }
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let staging_directory =
        prepare_or_resume_layer_staging_directory(output_directory, "Copernicus land-cover")?;
    let profile = CopernicusLandCoverPackingProfile {
        layer_id,
        source_snapshot_digest: source.source_snapshot_digest,
        source_artifact_digest: source.artifact_hash,
        container_s2_level,
        target_s2_level,
        points_per_axis,
    };
    let mut cache = CopernicusLandCoverChunkCache::new(&source.file, source_chunk_cache)?;
    let (root_relative_path, root_bytes, target_cells) =
        write_packed_copernicus_land_cover_layer(&staging_directory, profile, &mut cache)?;
    let target_support_samples = target_cells
        .checked_mul(u64::from(points_per_axis) * u64::from(points_per_axis))
        .context("Copernicus target-support total overflow")?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish Copernicus land-cover directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverLayerDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: source.source_snapshot_id.clone(),
            source_snapshot_digest: source.source_snapshot_digest,
            source_artifact_path: source.artifact_path.clone(),
            source_artifact_hash: source.artifact_hash,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: copernicus_land_cover_sample_policy(points_per_axis),
            points_per_axis,
            target_cells,
            target_support_samples,
            source_chunks_loaded: cache.chunks_loaded,
            source_chunk_cache_hits: cache.cache_hits,
            output_directory: output_directory.display().to_string(),
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct ChelsaJanuaryCellInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    row: u64,
    column: u64,
    latitude_ieee754_le_hex: String,
    longitude_ieee754_le_hex: String,
    raw_band1_ieee754_le_hex: String,
    documented_temperature_millicelsius: i64,
}

#[derive(Serialize)]
struct ChelsaNearestCellInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    requested_latitude_e7: i32,
    requested_longitude_e7: i32,
    row: u64,
    column: u64,
    source_latitude_e7: i32,
    source_longitude_e7: i32,
}

#[derive(Serialize)]
struct ChelsaAnnualCellInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    requested_latitude_e7: i32,
    requested_longitude_e7: i32,
    row: u64,
    column: u64,
    source_latitude_e7: i32,
    source_longitude_e7: i32,
    monthly_raw_ieee754_le_hex: Vec<String>,
    monthly_millicelsius: Vec<i64>,
    monthly_artifact_digests: Vec<Digest>,
}

#[derive(Debug)]
struct ChelsaGridAxes {
    latitudes_e7: Vec<i32>,
    longitudes_e7: Vec<i32>,
}

impl ChelsaGridAxes {
    fn nearest_cell(&self, coordinate: GeographicCoordinateE7) -> Result<(u64, u64)> {
        let row = nearest_sorted_e7_index(&self.latitudes_e7, coordinate.latitude_e7())?;
        let column = nearest_sorted_e7_index(&self.longitudes_e7, coordinate.longitude_e7())?;
        Ok((u64::try_from(row)?, u64::try_from(column)?))
    }

    fn nearest_cell_if_covered(
        &self,
        coordinate: GeographicCoordinateE7,
    ) -> Result<Option<(u32, u32)>> {
        let latitude = coordinate.latitude_e7();
        let longitude = coordinate.longitude_e7();
        if self
            .latitudes_e7
            .first()
            .is_none_or(|minimum| latitude < *minimum)
            || self
                .latitudes_e7
                .last()
                .is_none_or(|maximum| latitude > *maximum)
            || self
                .longitudes_e7
                .first()
                .is_none_or(|minimum| longitude < *minimum)
            || self
                .longitudes_e7
                .last()
                .is_none_or(|maximum| longitude > *maximum)
        {
            return Ok(None);
        }
        let (row, column) = self.nearest_cell(coordinate)?;
        Ok(Some((u32::try_from(row)?, u32::try_from(column)?)))
    }
}

fn inspect_chelsa_january_temperature(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    validate_chelsa_january_temperature_schema(&file)?;
    let data_attributes = file
        .variable("Band1")
        .context("CHELSA has no January-temperature data variable")?
        .attributes()
        .iter()
        .map(|attribute| ChelsaAttributeInspection {
            name: attribute.name.clone(),
            string_value: attribute.value.as_string(),
            first_numeric_value: attribute.value.as_f64().map(|value| value.to_string()),
        })
        .collect::<Vec<_>>();
    let latitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lat", CHELSA_LATITUDE_CELLS, "CHELSA latitude")?;
    let longitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lon", CHELSA_LONGITUDE_CELLS, "CHELSA longitude")?;
    let mut variables = file
        .variables()
        .context("enumerate CHELSA variables")?
        .iter()
        .map(|variable| EtopoVariableInspection {
            name: variable.name().to_owned(),
            shape: variable.shape().to_vec(),
        })
        .collect::<Vec<_>>();
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&ChelsaJanuaryTemperatureInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            data_variable: "Band1".to_owned(),
            data_shape: vec![CHELSA_LATITUDE_CELLS, CHELSA_LONGITUDE_CELLS],
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            data_attributes,
            variables,
        })?
    );
    Ok(())
}

/// Verify that every retained monthly normal has the same declared CHELSA grid and
/// exact coordinate axes before a climate normalizer can treat them as one annual
/// cycle. This reads metadata and coordinate endpoints, not the full raster payload.
fn inspect_chelsa_annual_temperature(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let artifacts = chelsa_annual_temperature_artifacts(&snapshot)?;
    let mut shared_latitude = None;
    let mut shared_longitude = None;
    let mut monthly_normals = Vec::with_capacity(12);
    for (offset, artifact) in artifacts.into_iter().enumerate() {
        let expected_month = u8::try_from(offset + 1)?;
        let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
            .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
        validate_chelsa_january_temperature_schema(&file)?;
        let latitude =
            inspect_etopo_axis_endpoints(&file, "lat", CHELSA_LATITUDE_CELLS, "CHELSA latitude")?;
        let longitude =
            inspect_etopo_axis_endpoints(&file, "lon", CHELSA_LONGITUDE_CELLS, "CHELSA longitude")?;
        if let Some(expected) = shared_latitude.as_ref()
            && &latitude != expected
        {
            bail!("CHELSA monthly normals disagree on the latitude coordinate axis");
        }
        if let Some(expected) = shared_longitude.as_ref()
            && &longitude != expected
        {
            bail!("CHELSA monthly normals disagree on the longitude coordinate axis");
        }
        shared_latitude.get_or_insert(latitude);
        shared_longitude.get_or_insert(longitude);
        monthly_normals.push(ChelsaMonthlyTemperatureInspection {
            month: expected_month,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
        });
    }
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualTemperatureInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            monthly_normals,
            shared_latitude_endpoint_ieee754_le_hex: shared_latitude
                .context("annual CHELSA snapshot is empty")?,
            shared_longitude_endpoint_ieee754_le_hex: shared_longitude
                .context("annual CHELSA snapshot is empty")?,
            documented_raw_to_millicelsius: "stored value * 100 - 273150",
        })?
    );
    Ok(())
}

fn validate_chelsa_january_temperature_schema(file: &NcFile) -> Result<()> {
    let latitude = file.variable("lat").context("CHELSA has no lat variable")?;
    let longitude = file.variable("lon").context("CHELSA has no lon variable")?;
    let temperature = file
        .variable("Band1")
        .context("CHELSA has no January-temperature data variable")?;
    if latitude.shape() != [CHELSA_LATITUDE_CELLS]
        || longitude.shape() != [CHELSA_LONGITUDE_CELLS]
        || temperature.shape() != [CHELSA_LATITUDE_CELLS, CHELSA_LONGITUDE_CELLS]
    {
        bail!("CHELSA variables do not have the pinned January-temperature grid shape");
    }
    Ok(())
}

fn inspect_chelsa_january_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    row: u64,
    column: u64,
) -> Result<()> {
    validate_chelsa_cell_address(row, column)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    validate_chelsa_january_temperature_schema(&file)?;
    let latitude = read_chelsa_coordinate(&file, "lat", row, "latitude")?;
    let longitude = read_chelsa_coordinate(&file, "lon", column, "longitude")?;
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(row), NcSliceInfoElem::Index(column)],
    };
    let values = file
        .read_variable_slice::<f32>("Band1", &selection)
        .context("read one raw CHELSA January-temperature sample")?;
    let value = values
        .as_slice()
        .context("CHELSA sample selection is not contiguous")?
        .first()
        .copied()
        .context("CHELSA sample selection is empty")?;
    let documented_temperature_millicelsius = chelsa_raw_tas_to_millicelsius(value)?;
    println!(
        "{}",
        serde_json::to_string(&ChelsaJanuaryCellInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            row,
            column,
            latitude_ieee754_le_hex: format!("{:016x}", latitude.to_bits()),
            longitude_ieee754_le_hex: format!("{:016x}", longitude.to_bits()),
            raw_band1_ieee754_le_hex: format!("{:08x}", value.to_bits()),
            documented_temperature_millicelsius,
        })?
    );
    Ok(())
}

/// Resolve a geography coordinate through the retained axis coordinates themselves.
/// No assumed origin or floating-point increment is permitted: the NetCDF coordinate
/// vectors are the source contract, converted from their IEEE bits to the E7 lattice.
fn inspect_chelsa_nearest_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    let axes = read_chelsa_grid_axes(&file)?;
    let (row, column) = axes.nearest_cell(coordinate)?;
    println!(
        "{}",
        serde_json::to_string(&ChelsaNearestCellInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            requested_latitude_e7: latitude_e7,
            requested_longitude_e7: longitude_e7,
            row,
            column,
            source_latitude_e7: axes.latitudes_e7[usize::try_from(row)?],
            source_longitude_e7: axes.longitudes_e7[usize::try_from(column)?],
        })?
    );
    Ok(())
}

/// Read the retained January-through-December normal vector for one mapped source
/// location. This is an auditable source inspection, not a climate-layer derivation:
/// it deliberately exposes exact stored samples and phase provenance before any
/// spatial aggregation can summarize them.
fn inspect_chelsa_annual_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifacts = chelsa_annual_temperature_artifacts(&snapshot)?;
    let first = artifacts
        .first()
        .context("annual CHELSA snapshot is empty")?;
    let first_file = NcFile::open(artifact_root.join(&first.artifact_path))
        .context("parse first verified CHELSA monthly NetCDF")?;
    let axes = read_chelsa_grid_axes(&first_file)?;
    let (row, column) = axes.nearest_cell(coordinate)?;
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(row), NcSliceInfoElem::Index(column)],
    };
    let mut monthly_raw_ieee754_le_hex = Vec::with_capacity(12);
    let mut monthly_millicelsius = Vec::with_capacity(12);
    let mut monthly_artifact_digests = Vec::with_capacity(12);
    for artifact in artifacts {
        let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
            .context("parse verified CHELSA monthly NetCDF")?;
        validate_chelsa_january_temperature_schema(&file)?;
        let values = file
            .read_variable_slice::<f32>("Band1", &selection)
            .context("read CHELSA monthly sample")?;
        let value = values
            .as_slice()
            .context("CHELSA sample selection is not contiguous")?
            .first()
            .copied()
            .context("CHELSA sample selection is empty")?;
        monthly_raw_ieee754_le_hex.push(format!("{:08x}", value.to_bits()));
        monthly_millicelsius.push(chelsa_raw_tas_to_millicelsius(value)?);
        monthly_artifact_digests.push(artifact.content_hash);
    }
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualCellInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            requested_latitude_e7: latitude_e7,
            requested_longitude_e7: longitude_e7,
            row,
            column,
            source_latitude_e7: axes.latitudes_e7[usize::try_from(row)?],
            source_longitude_e7: axes.longitudes_e7[usize::try_from(column)?],
            monthly_raw_ieee754_le_hex,
            monthly_millicelsius,
            monthly_artifact_digests,
        })?
    );
    Ok(())
}

fn chelsa_annual_temperature_artifacts(
    snapshot: &SourceSnapshotManifest,
) -> Result<Vec<&SourceSnapshotArtifact>> {
    let mut artifacts = snapshot
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.artifact_path.cmp(&right.artifact_path));
    if artifacts.len() != 12 {
        bail!("annual CHELSA snapshot must retain exactly twelve monthly NetCDF artifacts");
    }
    for (offset, artifact) in artifacts.iter().enumerate() {
        let expected_month = u8::try_from(offset + 1)?;
        let expected_suffix = format!("tas_{expected_month:02}_1981-2010_v.2.1.nc");
        if !artifact.artifact_path.ends_with(&expected_suffix) {
            bail!("annual CHELSA artifacts are not canonical January-through-December order");
        }
    }
    Ok(artifacts)
}

struct VerifiedChelsaAnnualTemperature {
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_set_digest: Digest,
    source_artifacts: Vec<SeasonalSourceArtifact>,
    axes: ChelsaGridAxes,
    files: Vec<NcFile>,
}

fn open_verified_chelsa_annual_temperature(
    manifest_path: &Path,
    artifact_root: &Path,
) -> Result<VerifiedChelsaAnnualTemperature> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifacts = chelsa_annual_temperature_artifacts(&snapshot)?;
    let mut artifact_set_hasher = Sha256::new();
    artifact_set_hasher.update(b"a-tiny-civilization:chelsa-monthly-tas-artifact-set:v1\0");
    let mut files = Vec::with_capacity(12);
    let mut source_artifacts = Vec::with_capacity(12);
    for (month, artifact) in artifacts.into_iter().enumerate() {
        artifact_set_hasher.update(u64::try_from(artifact.artifact_path.len())?.to_le_bytes());
        artifact_set_hasher.update(artifact.artifact_path.as_bytes());
        artifact_set_hasher.update(artifact.byte_length.to_le_bytes());
        artifact_set_hasher.update(artifact.content_hash.as_bytes());
        let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
            .with_context(|| format!("open CHELSA monthly normal {}", artifact.artifact_path))?;
        validate_chelsa_january_temperature_schema(&file)?;
        files.push(file);
        source_artifacts.push(SeasonalSourceArtifact {
            digest: artifact.content_hash,
            phase_mask: 1_u16 << month,
        });
    }
    source_artifacts.sort_by_key(|artifact| artifact.digest);
    let axes = read_chelsa_grid_axes(files.first().context("CHELSA annual source has no files")?)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    Ok(VerifiedChelsaAnnualTemperature {
        source_snapshot_id: snapshot.snapshot_id,
        source_snapshot_digest,
        source_artifact_set_digest: Digest::from_bytes(artifact_set_hasher.finalize().into()),
        source_artifacts,
        axes,
        files,
    })
}

struct ChelsaMonthlyTemperatureChunk {
    last_used: u64,
    width: u32,
    height: u32,
    monthly_raw: Vec<Vec<f32>>,
}

struct ChelsaMonthlyTemperatureChunkCache<'a> {
    files: &'a [NcFile],
    capacity: usize,
    access_clock: u64,
    chunks_loaded: u64,
    cache_hits: u64,
    chunks: HashMap<(u32, u32), ChelsaMonthlyTemperatureChunk>,
}

impl<'a> ChelsaMonthlyTemperatureChunkCache<'a> {
    fn new(files: &'a [NcFile], capacity: usize) -> Result<Self> {
        if files.len() != 12 {
            bail!("CHELSA monthly temperature cache requires twelve source files");
        }
        if !(1..=32).contains(&capacity) {
            bail!("CHELSA source chunk cache must retain 1 through 32 monthly chunks");
        }
        Ok(Self {
            files,
            capacity,
            access_clock: 0,
            chunks_loaded: 0,
            cache_hits: 0,
            chunks: HashMap::new(),
        })
    }

    fn load_chunk(
        &self,
        chunk_row: u32,
        chunk_column: u32,
    ) -> Result<ChelsaMonthlyTemperatureChunk> {
        let latitude_start = u64::from(chunk_row) * u64::from(CHELSA_SOURCE_CHUNK_CELLS);
        let longitude_start = u64::from(chunk_column) * u64::from(CHELSA_SOURCE_CHUNK_CELLS);
        if latitude_start >= CHELSA_LATITUDE_CELLS || longitude_start >= CHELSA_LONGITUDE_CELLS {
            bail!("CHELSA chunk address lies outside the source raster");
        }
        let latitude_end = latitude_start
            .checked_add(u64::from(CHELSA_SOURCE_CHUNK_CELLS))
            .context("CHELSA latitude chunk overflow")?
            .min(CHELSA_LATITUDE_CELLS);
        let longitude_end = longitude_start
            .checked_add(u64::from(CHELSA_SOURCE_CHUNK_CELLS))
            .context("CHELSA longitude chunk overflow")?
            .min(CHELSA_LONGITUDE_CELLS);
        let width = u32::try_from(longitude_end - longitude_start)?;
        let height = u32::try_from(latitude_end - latitude_start)?;
        let expected_values = usize::try_from(u64::from(width) * u64::from(height))?;
        let selection = NcSliceInfo {
            selections: vec![
                NcSliceInfoElem::Slice {
                    start: latitude_start,
                    end: latitude_end,
                    step: 1,
                },
                NcSliceInfoElem::Slice {
                    start: longitude_start,
                    end: longitude_end,
                    step: 1,
                },
            ],
        };
        let mut monthly_raw = Vec::with_capacity(12);
        for (offset, file) in self.files.iter().enumerate() {
            let values = file
                .read_variable_slice::<f32>("Band1", &selection)
                .with_context(|| {
                    format!(
                        "read CHELSA month {} source chunk {chunk_row},{chunk_column}",
                        offset + 1
                    )
                })?;
            let values = values
                .as_slice()
                .context("CHELSA monthly source chunk is not contiguous")?;
            if values.len() != expected_values {
                bail!("CHELSA monthly source chunk has an unexpected sample count");
            }
            monthly_raw.push(values.to_vec());
        }
        Ok(ChelsaMonthlyTemperatureChunk {
            last_used: 0,
            width,
            height,
            monthly_raw,
        })
    }

    fn lookup(&mut self, row: u32, column: u32) -> Result<[i64; 12]> {
        if u64::from(row) >= CHELSA_LATITUDE_CELLS || u64::from(column) >= CHELSA_LONGITUDE_CELLS {
            bail!("CHELSA source lookup lies outside its raster");
        }
        let key = (
            row / CHELSA_SOURCE_CHUNK_CELLS,
            column / CHELSA_SOURCE_CHUNK_CELLS,
        );
        if !self.chunks.contains_key(&key) {
            if self.chunks.len() == self.capacity {
                let oldest = self
                    .chunks
                    .iter()
                    .min_by_key(|(_, chunk)| chunk.last_used)
                    .map(|(key, _)| *key)
                    .context("nonempty CHELSA cache has no oldest chunk")?;
                self.chunks.remove(&oldest);
            }
            let chunk = self.load_chunk(key.0, key.1)?;
            self.chunks.insert(key, chunk);
            self.chunks_loaded = self
                .chunks_loaded
                .checked_add(1)
                .context("CHELSA source chunk load count overflow")?;
        } else {
            self.cache_hits = self
                .cache_hits
                .checked_add(1)
                .context("CHELSA source cache hit count overflow")?;
        }
        self.access_clock = self
            .access_clock
            .checked_add(1)
            .context("CHELSA source cache clock overflow")?;
        let chunk = self
            .chunks
            .get_mut(&key)
            .context("CHELSA source chunk disappeared from cache")?;
        chunk.last_used = self.access_clock;
        let local_row = row % CHELSA_SOURCE_CHUNK_CELLS;
        let local_column = column % CHELSA_SOURCE_CHUNK_CELLS;
        if local_row >= chunk.height || local_column >= chunk.width {
            bail!("CHELSA local source address exceeds an edge chunk");
        }
        let index = usize::try_from(
            u64::from(local_row)
                .checked_mul(u64::from(chunk.width))
                .and_then(|value| value.checked_add(u64::from(local_column)))
                .context("CHELSA chunk index overflow")?,
        )?;
        let monthly = chunk
            .monthly_raw
            .iter()
            .map(|values| {
                let raw = *values.get(index).context("CHELSA chunk sample is absent")?;
                if raw.to_bits() == (-2_147_483_648_f32).to_bits() {
                    Ok(CHELSA_MISSING_MILLICELSIUS)
                } else {
                    chelsa_raw_tas_to_millicelsius(raw)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        monthly
            .try_into()
            .map_err(|_| anyhow::anyhow!("CHELSA monthly vector does not contain twelve values"))
    }
}

#[derive(Debug, Serialize)]
struct ChelsaAnnualTemperatureLayerDerivation {
    derivation_schema_version: u16,
    status: &'static str,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_set_digest: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: &'static str,
    missing_value: String,
    target_cells: u64,
    source_chunks_loaded: u64,
    source_chunk_cache_hits: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

fn derive_chelsa_annual_temperature_layer(
    source_snapshot: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
    source_chunk_cache: usize,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!(
            "provisional CHELSA annual-temperature layer v1 requires L6 containers and L10 targets"
        );
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!("CHELSA annual-temperature output directory already exists");
    }
    let source = open_verified_chelsa_annual_temperature(source_snapshot, artifact_root)?;
    let mut cache = ChelsaMonthlyTemperatureChunkCache::new(&source.files, source_chunk_cache)?;
    let staging_directory =
        prepare_or_resume_layer_staging_directory(output_directory, "CHELSA annual temperature")?;
    let (root_index_path, root_bytes) = write_packed_chelsa_annual_temperature_layer(
        &staging_directory,
        layer_id,
        source.source_snapshot_digest,
        &source.source_artifacts,
        &source.axes,
        container_s2_level,
        target_s2_level,
        &mut cache,
    )?;
    let source_chunks_loaded = cache.chunks_loaded;
    let source_chunk_cache_hits = cache.cache_hits;
    drop(cache);
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish CHELSA annual-temperature directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualTemperatureLayerDerivation {
            derivation_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            source_snapshot_id: source.source_snapshot_id,
            source_snapshot_digest: source.source_snapshot_digest,
            source_artifact_set_digest: source.source_artifact_set_digest,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: "s2-cell-centre-nearest-chelsa-axis-cell-v1",
            missing_value: CHELSA_MISSING_MILLICELSIUS.to_string(),
            target_cells: u64::try_from(global_s2_cells_at_level(target_s2_level)?.len())?,
            source_chunks_loaded,
            source_chunk_cache_hits,
            output_directory: output_directory.display().to_string(),
            root_index_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_packed_chelsa_annual_temperature_layer(
    output_directory: &Path,
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifacts: &[SeasonalSourceArtifact],
    axes: &ChelsaGridAxes,
    container_s2_level: u8,
    target_s2_level: u8,
    cache: &mut ChelsaMonthlyTemperatureChunkCache<'_>,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{container_s2_level}");
    let tile_directory = output_directory
        .join("layers")
        .join(layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let containers = global_s2_cells_at_level(container_s2_level)?;
    let mut entries = Vec::with_capacity(containers.len());
    for (position, container) in containers.into_iter().enumerate() {
        let relative_path = format!("layers/{layer_id}/{level_directory}/{container}.tile");
        let artifact_path = output_directory.join(&relative_path);
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_chelsa_annual_temperature_tile(
                    &existing,
                    layer_id,
                    source_snapshot_digest,
                    source_artifacts,
                    container,
                    target_s2_level,
                )?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let tile = pack_chelsa_annual_temperature_tile(
                    layer_id,
                    source_snapshot_digest,
                    source_artifacts,
                    axes,
                    container,
                    target_s2_level,
                    cache,
                )?;
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged CHELSA temperature tile"),
        };
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_SEASONAL_FIELD_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 1_024 == 0 {
            eprintln!(
                "CHELSA annual-temperature normalization progress: {}/24576 containers",
                position + 1
            );
        }
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged CHELSA annual-temperature root differs from requested derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged CHELSA annual-temperature root"),
    }
    Ok((root_relative_path, root_bytes))
}

fn validate_resumable_chelsa_annual_temperature_tile(
    bytes: &[u8],
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifacts: &[SeasonalSourceArtifact],
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<()> {
    let tile = PackedSeasonalScalarFieldTile::from_canonical_slice(bytes)
        .context("decode staged CHELSA annual-temperature tile")?;
    if tile.layer_id != layer_id
        || tile.unit != "degC"
        || tile.decimal_places != 3
        || tile.phases_per_cycle != 12
        || tile.source_snapshot_digest != source_snapshot_digest
        || tile.source_artifacts != source_artifacts
        || tile.quadrature_points_per_axis != 1
        || tile.container_s2_cell_id != container_s2_cell_id
        || tile.target_s2_level != target_s2_level
    {
        bail!("staged CHELSA annual-temperature tile differs from requested derivation");
    }
    Ok(())
}

fn pack_chelsa_annual_temperature_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifacts: &[SeasonalSourceArtifact],
    axes: &ChelsaGridAxes,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
    cache: &mut ChelsaMonthlyTemperatureChunkCache<'_>,
) -> Result<PackedSeasonalScalarFieldTile> {
    let cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?
        .into_iter()
        .map(|s2_cell_id| {
            let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
                decode_s2_face_ij(s2_cell_id),
            )?)?)?;
            let monthly = if let Some((row, column)) = axes.nearest_cell_if_covered(coordinate)? {
                cache.lookup(row, column)?
            } else {
                [CHELSA_MISSING_MILLICELSIUS; 12]
            };
            let values = monthly.to_vec();
            Ok(SeasonalScalarFieldCell {
                s2_cell_id,
                support_samples_per_phase: 1,
                minimum_values: values.clone(),
                mean_values: values.clone(),
                maximum_values: values,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedSeasonalScalarFieldTile {
        tile_schema_version: 2,
        layer_id: layer_id.to_owned(),
        unit: "degC".to_owned(),
        decimal_places: 3,
        phases_per_cycle: 12,
        source_snapshot_digest,
        source_artifacts: source_artifacts.to_vec(),
        quadrature_points_per_axis: 1,
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed CHELSA annual-temperature tile is invalid")?;
    Ok(tile)
}

#[derive(Debug, Serialize)]
struct ChelsaAnnualTemperatureLayerInspection {
    inspection_schema_version: u16,
    status: &'static str,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    source_snapshot_digest: Digest,
    source_artifacts: Vec<SeasonalSourceArtifact>,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    missing_source_cells: u64,
    monthly_minimum_millicelsius: Vec<i64>,
    monthly_maximum_millicelsius: Vec<i64>,
    tile_byte_length: u64,
}

fn inspect_chelsa_annual_temperature_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical CHELSA annual-temperature root")?;
    if root.layer_id != layer_id {
        bail!("CHELSA annual-temperature root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("CHELSA annual-temperature root does not cover every expected container");
    }
    let mut source_snapshot_digest = None;
    let mut source_artifacts = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    let mut missing_source_cells = 0_u64;
    let mut monthly_minimum = [i64::MAX; 12];
    let mut monthly_maximum = [i64::MIN; 12];
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.media_type != PACKED_SEASONAL_FIELD_TILE_MEDIA_TYPE
        {
            bail!("CHELSA annual-temperature root contains an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("CHELSA annual-temperature tile fails its root reference");
        }
        let tile = PackedSeasonalScalarFieldTile::from_canonical_slice(&bytes)
            .context("decode canonical CHELSA annual-temperature tile")?;
        if tile.layer_id != layer_id
            || tile.unit != "degC"
            || tile.decimal_places != 3
            || tile.phases_per_cycle != 12
            || tile.quadrature_points_per_axis != 1
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
        {
            bail!("CHELSA annual-temperature tile has inconsistent packing metadata");
        }
        match source_snapshot_digest {
            Some(expected) if expected != tile.source_snapshot_digest => {
                bail!("CHELSA annual-temperature tiles disagree on source snapshot")
            }
            None => source_snapshot_digest = Some(tile.source_snapshot_digest),
            _ => {}
        }
        match source_artifacts.as_ref() {
            Some(expected) if expected != &tile.source_artifacts => {
                bail!("CHELSA annual-temperature tiles disagree on source artifacts")
            }
            None => source_artifacts = Some(tile.source_artifacts.clone()),
            _ => {}
        }
        for cell in &tile.cells {
            if cell.minimum_values != cell.mean_values || cell.mean_values != cell.maximum_values {
                bail!("CHELSA centre-sampled temperature cell contains a non-point range");
            }
            if cell
                .mean_values
                .iter()
                .all(|value| *value == CHELSA_MISSING_MILLICELSIUS)
            {
                missing_source_cells = missing_source_cells
                    .checked_add(1)
                    .context("CHELSA missing-cell count overflow")?;
                continue;
            }
            for (month, value) in cell.mean_values.iter().copied().enumerate() {
                if value != CHELSA_MISSING_MILLICELSIUS {
                    monthly_minimum[month] = monthly_minimum[month].min(value);
                    monthly_maximum[month] = monthly_maximum[month].max(value);
                }
            }
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("CHELSA annual-temperature tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("CHELSA annual-temperature target cell total overflow")?;
    }
    if monthly_minimum.contains(&i64::MAX) || monthly_maximum.contains(&i64::MIN) {
        bail!("CHELSA annual-temperature release contains no observed source values");
    }
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualTemperatureLayerInspection {
            inspection_schema_version: 1,
            status: "provisional-not-scientifically-admitted",
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            source_snapshot_digest: source_snapshot_digest
                .context("CHELSA annual-temperature root contains no tiles")?,
            source_artifacts: source_artifacts
                .context("CHELSA annual-temperature root contains no tiles")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            missing_source_cells,
            monthly_minimum_millicelsius: monthly_minimum.to_vec(),
            monthly_maximum_millicelsius: monthly_maximum.to_vec(),
            tile_byte_length,
        })?
    );
    Ok(())
}

fn read_chelsa_grid_axes(file: &NcFile) -> Result<ChelsaGridAxes> {
    validate_chelsa_january_temperature_schema(file)?;
    let read_axis = |variable: &str, expected: u64| -> Result<Vec<i32>> {
        let values = file
            .read_variable_slice::<f64>(variable, &NcSliceInfo::all(1))
            .with_context(|| format!("read complete CHELSA {variable} axis"))?;
        let values = values
            .as_slice()
            .context("CHELSA axis selection is not contiguous")?;
        if u64::try_from(values.len())? != expected {
            bail!("CHELSA axis length disagrees with declared shape");
        }
        values
            .iter()
            .map(|value| {
                i32::try_from(ieee754_degrees_bits_to_e7(value.to_bits())?)
                    .context("CHELSA E7 coordinate does not fit i32")
            })
            .collect()
    };
    let latitudes_e7 = read_axis("lat", CHELSA_LATITUDE_CELLS)?;
    let longitudes_e7 = read_axis("lon", CHELSA_LONGITUDE_CELLS)?;
    if !latitudes_e7.windows(2).all(|pair| pair[0] < pair[1])
        || !longitudes_e7.windows(2).all(|pair| pair[0] < pair[1])
    {
        bail!("CHELSA coordinate axes are not strictly increasing on the E7 lattice");
    }
    Ok(ChelsaGridAxes {
        latitudes_e7,
        longitudes_e7,
    })
}

fn nearest_sorted_e7_index(axis: &[i32], value: i32) -> Result<usize> {
    if axis.is_empty() || value < axis[0] || value > *axis.last().context("empty CHELSA axis")? {
        bail!("requested coordinate lies outside retained CHELSA source coverage");
    }
    match axis.binary_search(&value) {
        Ok(index) => Ok(index),
        Err(upper) => {
            let lower = upper
                .checked_sub(1)
                .context("CHELSA axis lower bound underflow")?;
            let lower_distance = i64::from(value) - i64::from(axis[lower]);
            let upper_distance = i64::from(axis[upper]) - i64::from(value);
            // Ties choose the smaller raw index, a declared deterministic policy.
            Ok(if lower_distance <= upper_distance {
                lower
            } else {
                upper
            })
        }
    }
}

/// The retained CHELSA technical specification defines `tas_01` through `tas_12`
/// as degrees Celsius after multiplying stored values by 0.1 and adding -273.15.
fn chelsa_raw_tas_to_millicelsius(raw: f32) -> Result<i64> {
    if !raw.is_finite() || raw.fract() != 0.0 {
        bail!("CHELSA tas sample must be a finite integral stored value");
    }
    let stored = raw as i64;
    stored
        .checked_mul(100)
        .and_then(|value| value.checked_sub(273_150))
        .context("CHELSA tas millidegree conversion overflow")
}

fn validate_chelsa_cell_address(row: u64, column: u64) -> Result<()> {
    if row >= CHELSA_LATITUDE_CELLS || column >= CHELSA_LONGITUDE_CELLS {
        bail!(
            "CHELSA cell ({row}, {column}) is outside the pinned {} by {} source grid",
            CHELSA_LATITUDE_CELLS,
            CHELSA_LONGITUDE_CELLS
        );
    }
    Ok(())
}

fn read_chelsa_coordinate(file: &NcFile, variable: &str, index: u64, axis: &str) -> Result<f64> {
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(index)],
    };
    let coordinates = file
        .read_variable_slice::<f64>(variable, &selection)
        .with_context(|| format!("read CHELSA {axis} coordinate"))?;
    coordinates
        .as_slice()
        .with_context(|| format!("CHELSA {axis} coordinate selection is not contiguous"))?
        .first()
        .copied()
        .with_context(|| format!("CHELSA {axis} coordinate selection is empty"))
}

fn inspect_etopo(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no ETOPO NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let latitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lat", ETOPO_LATITUDE_CELLS, "latitude")?;
    let longitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lon", ETOPO_LONGITUDE_CELLS, "longitude")?;
    let mut variables = file
        .variables()
        .context("enumerate ETOPO variables")?
        .iter()
        .map(|variable| EtopoVariableInspection {
            name: variable.name().to_owned(),
            shape: variable.shape().to_vec(),
        })
        .collect::<Vec<_>>();
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&EtopoInspection {
            inspection_schema_version: 2,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            variables,
        })?
    );
    Ok(())
}

fn inspect_etopo_axis_endpoints(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
    axis_name: &str,
) -> Result<[String; 2]> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read ETOPO {axis_name} coordinate axis"))?;
    let values = values
        .as_slice()
        .with_context(|| format!("ETOPO {axis_name} coordinate axis is not contiguous"))?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("ETOPO {axis_name} coordinate axis has an unexpected length");
    }
    let first = values.first().context("ETOPO coordinate axis is empty")?;
    let last = values.last().context("ETOPO coordinate axis is empty")?;
    Ok([
        format!("{:016x}", first.to_bits()),
        format!("{:016x}", last.to_bits()),
    ])
}

#[derive(Serialize)]
struct NaturalEarthLandInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    shapefile_version: u32,
    declared_shape_type: u32,
    bounding_box_ieee754_le_hex: [String; 4],
    record_count: u64,
    polygon_record_count: u64,
    part_count: u64,
    point_count: u64,
}

fn inspect_natural_earth_land(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let parsed = parse_polygon_shapefile(&bytes)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let inspection = NaturalEarthLandInspection {
        inspection_schema_version: 1,
        source_snapshot_id: snapshot.snapshot_id,
        source_snapshot_digest,
        artifact_path: artifact.artifact_path.clone(),
        artifact_hash: artifact.content_hash,
        artifact_byte_length: artifact.byte_length,
        shapefile_version: parsed.version,
        declared_shape_type: parsed.shape_type,
        bounding_box_ieee754_le_hex: parsed.bounding_box.map(|bits| format!("{bits:016x}")),
        record_count: parsed.records,
        polygon_record_count: parsed.polygons,
        part_count: parsed.parts,
        point_count: parsed.points,
    };
    println!("{}", serde_json::to_string(&inspection)?);
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandPointInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    latitude_e7: i32,
    longitude_e7: i32,
    inside_land_polygon: bool,
}

fn inspect_natural_earth_land_point(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    if !(-900_000_000..=900_000_000).contains(&latitude_e7)
        || !(-1_800_000_000..1_800_000_000).contains(&longitude_e7)
    {
        bail!("Natural Earth point is outside exact WGS 84 E7 bounds");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let inside_land_polygon = natural_earth_contains_point(&bytes, longitude_e7, latitude_e7)?;
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandPointInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            latitude_e7,
            longitude_e7,
            inside_land_polygon,
        })?
    );
    Ok(())
}

/// Classify an exact E7 coordinate using the generalized Natural Earth polygons.
///
/// Shapefiles store vertices as IEEE-754 doubles.  The classifier converts each
/// source bit pattern once to its nearest E7 lattice point with integer arithmetic,
/// then performs the even-odd test using integer cross-products. This prevents host
/// floating-point behavior from becoming a normalization input.
fn natural_earth_contains_point(bytes: &[u8], longitude_e7: i32, latitude_e7: i32) -> Result<bool> {
    parse_polygon_shapefile(bytes)?;
    natural_earth_contains_point_unvalidated(bytes, longitude_e7, latitude_e7)
}

// A one-degree latitude bucket is deliberately only an execution index.  It has no
// representation in a derived tile: the classifier below performs exactly the same
// half-open horizontal-ray test as `point_in_shapefile_ring`, using the same E7
// conversion.  It prevents the full source stream (and its IEEE-754 decoding) from
// being traversed once per L10 target centre.
const NATURAL_EARTH_LATITUDE_BUCKETS: usize = 180;
const NATURAL_EARTH_E7_PER_DEGREE: i128 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NaturalEarthEdge {
    current_x: i128,
    current_y: i128,
    previous_x: i128,
    previous_y: i128,
}

#[derive(Debug)]
struct PreparedNaturalEarthLand {
    // Edges are present in every degree band that can satisfy
    // min_y <= query_y < max_y. Order is source order, though parity makes the
    // result independent of that order.
    edges_by_latitude_bucket: Vec<Vec<NaturalEarthEdge>>,
}

impl PreparedNaturalEarthLand {
    fn from_shapefile(bytes: &[u8]) -> Result<Self> {
        parse_polygon_shapefile(bytes)?;
        let mut edges_by_latitude_bucket = vec![Vec::new(); NATURAL_EARTH_LATITUDE_BUCKETS];
        let mut offset = 100_usize;
        while offset < bytes.len() {
            let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
                .checked_mul(2)
                .context("record length overflow")?;
            let body = &bytes[offset + 8..offset + 8 + content_length];
            if le_u32(&body[..4])? == 5 {
                let parts = usize::try_from(le_u32(&body[36..40])?)?;
                let points = usize::try_from(le_u32(&body[40..44])?)?;
                let part_start = 44;
                let point_start = part_start + parts * 4;
                let source_points = &body[point_start..];
                for part in 0..parts {
                    let start = usize::try_from(le_u32(
                        &body[part_start + part * 4..part_start + part * 4 + 4],
                    )?)?;
                    let end = if part + 1 == parts {
                        points
                    } else {
                        usize::try_from(le_u32(
                            &body[part_start + (part + 1) * 4..part_start + (part + 1) * 4 + 4],
                        )?)?
                    };
                    if end <= start + 2 {
                        continue;
                    }
                    let point = |index: usize| -> Result<(i128, i128)> {
                        let point_offset =
                            index.checked_mul(16).context("point offset overflow")?;
                        Ok((
                            ieee754_degrees_bits_to_e7(le_u64(
                                &source_points[point_offset..point_offset + 8],
                            )?)?,
                            ieee754_degrees_bits_to_e7(le_u64(
                                &source_points[point_offset + 8..point_offset + 16],
                            )?)?,
                        ))
                    };
                    let mut previous = point(end - 1)?;
                    for index in start..end {
                        let current = point(index)?;
                        let edge = NaturalEarthEdge {
                            current_x: current.0,
                            current_y: current.1,
                            previous_x: previous.0,
                            previous_y: previous.1,
                        };
                        if edge.current_y != edge.previous_y {
                            let lower = edge.current_y.min(edge.previous_y);
                            let upper = edge.current_y.max(edge.previous_y) - 1;
                            let first = natural_earth_latitude_bucket(lower)?;
                            let last = natural_earth_latitude_bucket(upper)?;
                            for bucket_edges in edges_by_latitude_bucket
                                .iter_mut()
                                .take(last + 1)
                                .skip(first)
                            {
                                bucket_edges.push(edge);
                            }
                        }
                        previous = current;
                    }
                }
            }
            offset += 8 + content_length;
        }
        Ok(Self {
            edges_by_latitude_bucket,
        })
    }

    fn contains_point(&self, longitude_e7: i32, latitude_e7: i32) -> bool {
        let x = i128::from(longitude_e7);
        let y = i128::from(latitude_e7);
        let bucket = natural_earth_latitude_bucket(y)
            .expect("validated geographic E7 coordinate must select a latitude bucket");
        self.edges_by_latitude_bucket[bucket]
            .iter()
            .filter(|edge| (edge.current_y > y) != (edge.previous_y > y))
            .fold(false, |inside, edge| {
                let denominator = edge.previous_y - edge.current_y;
                let left = (x - edge.current_x) * denominator;
                let right = (edge.previous_x - edge.current_x) * (y - edge.current_y);
                if if denominator > 0 {
                    left < right
                } else {
                    left > right
                } {
                    !inside
                } else {
                    inside
                }
            })
    }
}

fn natural_earth_latitude_bucket(latitude_e7: i128) -> Result<usize> {
    if !(-900_000_000..=900_000_000).contains(&latitude_e7) {
        bail!("Natural Earth latitude is outside exact WGS 84 E7 bounds");
    }
    // 90° has no non-horizontal crossing edge, but route it to the final bucket
    // so direct classification remains total at the closed geographic pole.
    let bucket = ((latitude_e7 + 900_000_000) / NATURAL_EARTH_E7_PER_DEGREE)
        .min(i128::try_from(NATURAL_EARTH_LATITUDE_BUCKETS - 1)?);
    usize::try_from(bucket).context("latitude bucket does not fit usize")
}

/// The caller has already validated the complete polygon stream with
/// `parse_polygon_shapefile`; this avoids a full source traversal per target cell.
fn natural_earth_contains_point_unvalidated(
    bytes: &[u8],
    longitude_e7: i32,
    latitude_e7: i32,
) -> Result<bool> {
    let mut inside = false;
    let mut offset = 100_usize;
    while offset < bytes.len() {
        let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
            .checked_mul(2)
            .context("record length overflow")?;
        let body = &bytes[offset + 8..offset + 8 + content_length];
        if le_u32(&body[..4])? == 5 {
            let parts = usize::try_from(le_u32(&body[36..40])?)?;
            let points = usize::try_from(le_u32(&body[40..44])?)?;
            let part_start = 44;
            let point_start = part_start + parts * 4;
            for part in 0..parts {
                let start = usize::try_from(le_u32(
                    &body[part_start + part * 4..part_start + part * 4 + 4],
                )?)?;
                let end = if part + 1 == parts {
                    points
                } else {
                    usize::try_from(le_u32(
                        &body[part_start + (part + 1) * 4..part_start + (part + 1) * 4 + 4],
                    )?)?
                };
                if point_in_shapefile_ring(
                    &body[point_start..],
                    start,
                    end,
                    i128::from(longitude_e7),
                    i128::from(latitude_e7),
                )? {
                    inside = !inside;
                }
            }
        }
        offset += 8 + content_length;
    }
    Ok(inside)
}

fn point_in_shapefile_ring(
    points: &[u8],
    start: usize,
    end: usize,
    x: i128,
    y: i128,
) -> Result<bool> {
    if end <= start + 2 {
        return Ok(false);
    }
    let point = |index: usize| -> Result<(i128, i128)> {
        let offset = index.checked_mul(16).context("point offset overflow")?;
        Ok((
            ieee754_degrees_bits_to_e7(le_u64(&points[offset..offset + 8])?)?,
            ieee754_degrees_bits_to_e7(le_u64(&points[offset + 8..offset + 16])?)?,
        ))
    };
    let mut crossed = false;
    let mut previous = point(end - 1)?;
    for index in start..end {
        let current = point(index)?;
        let crosses_horizontal_ray = (current.1 > y) != (previous.1 > y);
        let denominator = previous.1 - current.1;
        let left = (x - current.0) * denominator;
        let right = (previous.0 - current.0) * (y - current.1);
        if crosses_horizontal_ray
            && if denominator > 0 {
                left < right
            } else {
                left > right
            }
        {
            crossed = !crossed;
        }
        previous = current;
    }
    Ok(crossed)
}

fn ieee754_degrees_bits_to_e7(bits: u64) -> Result<i128> {
    let sign = if bits >> 63 == 0 { 1_i128 } else { -1_i128 };
    let exponent = i32::try_from((bits >> 52) & 0x7ff)?;
    let fraction = bits & ((1_u64 << 52) - 1);
    if exponent == 0x7ff {
        bail!("Natural Earth coordinate is not finite");
    }
    if exponent == 0 && fraction == 0 {
        return Ok(0);
    }
    let (mantissa, binary_exponent) = if exponent == 0 {
        (i128::from(fraction), -1074_i32)
    } else {
        (i128::from((1_u64 << 52) | fraction), exponent - 1075)
    };
    let scaled = mantissa
        .checked_mul(10_000_000)
        .context("Natural Earth E7 mantissa overflow")?;
    let rounded = if binary_exponent >= 0 {
        scaled
            .checked_shl(u32::try_from(binary_exponent)?)
            .context("Natural Earth E7 exponent overflow")?
    } else {
        let divisor = 1_i128
            .checked_shl(u32::try_from(-binary_exponent)?)
            .context("Natural Earth E7 divisor overflow")?;
        let quotient = scaled / divisor;
        let remainder = scaled % divisor;
        quotient + i128::from(remainder * 2 >= divisor)
    };
    sign.checked_mul(rounded)
        .context("Natural Earth E7 sign overflow")
}

#[derive(Debug, Eq, PartialEq)]
struct PolygonShapefileSummary {
    version: u32,
    shape_type: u32,
    bounding_box: [u64; 4],
    records: u64,
    polygons: u64,
    parts: u64,
    points: u64,
}

fn parse_polygon_shapefile(bytes: &[u8]) -> Result<PolygonShapefileSummary> {
    if bytes.len() < 100 {
        bail!("shapefile is shorter than its header");
    }
    if be_u32(&bytes[0..4])? != 9994 {
        bail!("unexpected shapefile code");
    }
    let words = usize::try_from(be_u32(&bytes[24..28])?)?;
    if words.checked_mul(2) != Some(bytes.len()) {
        bail!("shapefile header length disagrees with file");
    }
    let version = le_u32(&bytes[28..32])?;
    if version != 1000 {
        bail!("unsupported shapefile version {version}");
    }
    let shape_type = le_u32(&bytes[32..36])?;
    if shape_type != 5 {
        bail!("expected polygon shapefile type 5");
    }
    let mut bounding_box = [0_u64; 4];
    for (index, value) in bounding_box.iter_mut().enumerate() {
        *value = le_u64(&bytes[36 + index * 8..44 + index * 8])?;
    }
    let (mut offset, mut records, mut polygons, mut parts, mut points) =
        (100_usize, 0_u64, 0_u64, 0_u64, 0_u64);
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            bail!("truncated shapefile record header");
        }
        let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
            .checked_mul(2)
            .context("shapefile record length overflow")?;
        let start = offset.checked_add(8).context("record offset overflow")?;
        let end = start
            .checked_add(content_length)
            .context("record end overflow")?;
        if end > bytes.len() {
            bail!("truncated shapefile record body");
        }
        records += 1;
        let body = &bytes[start..end];
        if body.len() < 4 {
            bail!("empty shapefile record");
        }
        match le_u32(&body[..4])? {
            0 => {}
            5 => {
                if body.len() < 44 {
                    bail!("truncated polygon record");
                }
                let record_parts = usize::try_from(le_u32(&body[36..40])?)?;
                let record_points = usize::try_from(le_u32(&body[40..44])?)?;
                let expected = 44_usize
                    .checked_add(record_parts.checked_mul(4).context("part overflow")?)
                    .and_then(|value| value.checked_add(record_points.checked_mul(16)?))
                    .context("polygon record length overflow")?;
                if expected != body.len() {
                    bail!("polygon record length disagrees with counts");
                }
                polygons += 1;
                parts += u64::try_from(record_parts)?;
                points += u64::try_from(record_points)?;
            }
            _ => bail!("polygon shapefile contains an unexpected record type"),
        }
        offset = end;
    }
    Ok(PolygonShapefileSummary {
        version,
        shape_type,
        bounding_box,
        records,
        polygons,
        parts,
        points,
    })
}

fn be_u32(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes.try_into().context("expected four bytes")?,
    ))
}
fn le_u32(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes.try_into().context("expected four bytes")?,
    ))
}
fn le_u64(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes.try_into().context("expected eight bytes")?,
    ))
}

fn validate(bundle_path: PathBuf, configuration_path: Option<&PathBuf>) -> Result<()> {
    let bytes = fs::read(&bundle_path)
        .with_context(|| format!("failed to read bundle {}", bundle_path.display()))?;
    let bundle = WorldDataBundle::from_canonical_slice(&bytes)
        .with_context(|| format!("bundle {} is invalid", bundle_path.display()))?;
    let digest = bundle.content_digest()?;
    let artifact_root = bundle_path
        .parent()
        .context("bundle path has no parent directory")?;
    let stats = verify_release_artifacts(&bundle, artifact_root)?;

    if let Some(path) = configuration_path {
        let config_bytes = fs::read(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        let configuration: WorldConfiguration = serde_json::from_slice(&config_bytes)
            .with_context(|| format!("failed to decode configuration {}", path.display()))?;
        bundle
            .validate_for_configuration(&configuration)
            .with_context(|| format!("bundle does not match configuration {}", path.display()))?;
        println!("configuration: matched {}", path.display());
    }

    println!("bundle: {}@{}", bundle.bundle_id, bundle.bundle_version);
    println!("schema: {}", bundle.bundle_schema_version);
    println!("sha256: {digest}");
    println!("sources: {}", bundle.sources.len());
    println!("entities: {}", bundle.entities.len());
    println!("parameters: {}", bundle.parameters.len());
    println!("layers: {}", bundle.layers.len());
    println!("tile indexes: {}", stats.tile_indexes);
    println!("tiles: {}", stats.tiles);
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}

fn validate_provisional_world(composition_path: &Path, artifact_root: &Path) -> Result<()> {
    let composition = load_provisional_world_composition(composition_path)?;
    let stats = verify_provisional_world_artifacts(&composition, artifact_root)?;

    println!(
        "composition: {}@{}",
        composition.composition_id, composition.composition_version
    );
    println!("status: provisional-not-scientifically-admitted");
    println!("sha256: {}", composition.content_digest()?);
    println!("earth layers: {}", composition.earth_layers.len());
    println!("world components: {}", composition.world_components.len());
    println!(
        "validation gaps: {}",
        composition.coupled_validation_gaps.len()
    );
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}

fn load_source_manifest(path: &Path) -> Result<SourceSnapshotManifest> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read source manifest {}", path.display()))?;
    SourceSnapshotManifest::from_canonical_slice(&bytes)
        .with_context(|| format!("source manifest {} is invalid", path.display()))
}

fn validate_source(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    let digest = snapshot.content_digest()?;
    let stats = verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    println!("source snapshot: {}", snapshot.snapshot_id);
    println!("upstream release: {}", snapshot.upstream_release);
    println!("dataset version: {}", snapshot.dataset_version);
    println!("sha256: {digest}");
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}

async fn fetch_source(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    fs::create_dir_all(artifact_root).with_context(|| {
        format!(
            "failed to create source artifact root {}",
            artifact_root.display()
        )
    })?;
    let canonical_root = artifact_root.canonicalize().with_context(|| {
        format!(
            "failed to resolve source artifact root {}",
            artifact_root.display()
        )
    })?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(8))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .user_agent("a-tiny-civilization-source-acquisition/0.1")
        .build()
        .context("failed to construct HTTPS source client")?;

    for artifact in &snapshot.artifacts {
        let destination = prepare_destination(&canonical_root, &artifact.artifact_path)?;
        match fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!(
                        "source destination {} exists but is not a regular file",
                        destination.display()
                    );
                }
                verify_source_snapshot_artifact(artifact, &canonical_root).with_context(|| {
                    format!(
                        "existing source artifact {} differs; refusing to replace it",
                        destination.display()
                    )
                })?;
                println!("verified existing {}", artifact.artifact_path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                download_artifact(&client, artifact, &destination).await?;
                println!("fetched {}", artifact.artifact_path);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect source destination {}",
                        destination.display()
                    )
                });
            }
        }
    }

    validate_source(manifest_path, &canonical_root)
}

fn prepare_destination(canonical_root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    let parent = relative
        .parent()
        .context("source artifact path has no parent")?;
    let mut current = canonical_root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(part) = component else {
            bail!("source artifact path {relative_path:?} is not portable");
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!(
                        "source artifact parent {} is not a regular directory",
                        current.display()
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!("failed to create source directory {}", current.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect source directory {}", current.display())
                });
            }
        }
    }
    Ok(canonical_root.join(relative))
}

async fn download_artifact(
    client: &reqwest::Client,
    artifact: &SourceSnapshotArtifact,
    destination: &Path,
) -> Result<()> {
    let mut response = client
        .get(&artifact.download_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch {}", artifact.download_url))?
        .error_for_status()
        .with_context(|| format!("source returned an error for {}", artifact.download_url))?;
    if let Some(advertised) = response.content_length()
        && advertised != artifact.byte_length
    {
        bail!(
            "source {} advertised {} bytes, expected {}",
            artifact.download_url,
            advertised,
            artifact.byte_length
        );
    }

    let parent = destination
        .parent()
        .context("source destination has no parent directory")?;
    let file_name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .context("source destination filename is not UTF-8")?;
    let mut partial = PartialDownload::create(parent, file_name)?;
    let mut hasher = Sha256::new();
    let mut actual_length = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed while reading {}", artifact.download_url))?
    {
        actual_length = actual_length
            .checked_add(u64::try_from(chunk.len()).context("response chunk length overflow")?)
            .context("downloaded source length overflow")?;
        if actual_length > artifact.byte_length {
            bail!(
                "source {} exceeded expected length {}",
                artifact.download_url,
                artifact.byte_length
            );
        }
        hasher.update(&chunk);
        partial
            .file
            .write_all(&chunk)
            .with_context(|| format!("failed to write {}", partial.path.display()))?;
    }
    let actual_digest = Digest::from_bytes(hasher.finalize().into());
    artifact
        .expected_artifact()
        .verify_observation(actual_length, actual_digest)
        .with_context(|| format!("downloaded source {:?} is invalid", artifact.artifact_path))?;
    partial
        .file
        .sync_all()
        .with_context(|| format!("failed to sync {}", partial.path.display()))?;
    partial.persist_without_replacement(destination)
}

struct PartialDownload {
    path: PathBuf,
    file: File,
    persisted: bool,
}

impl PartialDownload {
    fn create(parent: &Path, file_name: &str) -> Result<Self> {
        for attempt in 0..100_u16 {
            let path = parent.join(format!(
                ".{file_name}.atc-partial-{}-{attempt}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file,
                        persisted: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to create partial source file {}", path.display())
                    });
                }
            }
        }
        bail!("could not allocate a unique partial source filename")
    }

    fn persist_without_replacement(&mut self, destination: &Path) -> Result<()> {
        fs::hard_link(&self.path, destination).with_context(|| {
            format!(
                "failed to publish {} without replacing {}",
                self.path.display(),
                destination.display()
            )
        })?;
        fs::remove_file(&self.path)
            .with_context(|| format!("failed to remove partial file {}", self.path.display()))?;
        sync_parent_directory(destination)?;
        self.persisted = true;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("published source destination has no parent")?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync source directory {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_destination: &Path) -> Result<()> {
    Ok(())
}

impl Drop for PartialDownload {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct ConstantLandCoverLookup {
        calls: u64,
    }

    impl CopernicusLandCoverLookup for ConstantLandCoverLookup {
        fn lookup(&mut self, _row: u32, _column: u32) -> Result<CopernicusLandCoverSourceValue> {
            self.calls += 1;
            Ok(CopernicusLandCoverSourceValue {
                class_value: 130,
                processed_flag: 1,
                current_pixel_state: 1,
                observation_count: 42,
                change_count: 0,
            })
        }
    }

    #[test]
    fn era5_member_contract_separates_instantaneous_and_accumulated_fields() {
        let instantaneous = expected_era5_member_variables(ERA5_ARCHIVE_MEMBERS[0])
            .expect("instantaneous ERA5 schema");
        let accumulated = expected_era5_member_variables(ERA5_ARCHIVE_MEMBERS[1])
            .expect("accumulated ERA5 schema");
        assert_eq!(instantaneous.get("t2m"), Some(&vec![12, 721, 1_440]));
        assert_eq!(instantaneous.get("siconc"), Some(&vec![12, 721, 1_440]));
        assert!(!instantaneous.contains_key("tp"));
        assert_eq!(accumulated.get("tp"), Some(&vec![12, 721, 1_440]));
        assert!(!accumulated.contains_key("t2m"));
        assert!(expected_era5_member_variables("unexpected.nc").is_err());
    }

    #[test]
    fn terrestrial_foraging_condition_excludes_pelagic_and_water_foraging_birds() {
        assert!(is_elton_terrestrial_foraging((0, 0, 0)));
        assert!(!is_elton_terrestrial_foraging((1, 0, 0)));
        assert!(!is_elton_terrestrial_foraging((0, 1, 0)));
        assert!(!is_elton_terrestrial_foraging((0, 0, 1)));
    }

    #[test]
    fn copernicus_land_cover_contract_keeps_classes_and_quality_fields_separate() {
        let variables = expected_copernicus_land_cover_variables();
        let expected_raster = vec![1, 64_800, 129_600];
        assert_eq!(
            variables.get("lccs_class"),
            Some(&(NcType::UByte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("processed_flag"),
            Some(&(NcType::Byte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("current_pixel_state"),
            Some(&(NcType::Byte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("observation_count"),
            Some(&(NcType::UShort, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("change_count"),
            Some(&(NcType::UByte, expected_raster))
        );
        assert_eq!(variables.len(), 12);
        assert_eq!(
            expected_copernicus_land_cover_global_attributes(),
            BTreeMap::from([
                ("id", "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1"),
                ("license", "EC C3S Land cover Data Policy"),
                ("product_version", "2.1.1"),
                ("source", "Sentinel-3 OLCI"),
                ("spatial_resolution", "300m"),
                ("time_coverage_end", "20221231"),
                ("time_coverage_start", "20220101"),
                ("tracking_id", "cbc0983e-a0fd-4277-9023-2e618c0c2067"),
            ])
        );
        assert_eq!(COPERNICUS_LCCS_CLASSES.len(), 38);
        assert_eq!(
            COPERNICUS_LAND_COVER_LATITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS,
            32
        );
        assert_eq!(
            COPERNICUS_LAND_COVER_LONGITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS,
            64
        );
    }

    #[test]
    fn committed_copernicus_census_fingerprints_complete_global_counts() {
        const CENSUS: &[u8] = include_bytes!(
            "../../../data/source-inspections/copernicus-satellite-land-cover-v2-1-1-2022-census.json"
        );
        assert_eq!(
            Digest::sha256(CENSUS).to_string(),
            "118fa2b71c9acdc785c131c0c9e8e19e00c1bad1b96805f4d286c48a8b35efee"
        );
        let census: serde_json::Value =
            serde_json::from_slice(CENSUS).expect("committed Copernicus census is JSON");
        let expected_cells = 8_398_080_000_u64;
        assert_eq!(
            census["source_snapshot_digest"].as_str(),
            Some("6b2acf6608c382c9321de4f69268c6e5caa2e564820094b41be844643bc27894")
        );
        assert_eq!(census["raster_cells"].as_u64(), Some(expected_cells));
        assert_eq!(census["chunks_scanned"].as_u64(), Some(2_048));
        assert_eq!(
            census["lccs_classes"]
                .as_array()
                .expect("LCCS class counts")
                .len(),
            38
        );
        for field in [
            "lccs_classes",
            "processed_flag_counts",
            "current_pixel_state_counts",
            "observation_count_counts",
            "change_count_counts",
        ] {
            let total = census[field]
                .as_array()
                .expect("census count array")
                .iter()
                .map(|entry| entry["cells"].as_u64().expect("integer cell count"))
                .sum::<u64>();
            assert_eq!(total, expected_cells, "incomplete {field} census");
        }
        let water = census["lccs_classes"]
            .as_array()
            .expect("LCCS class counts")
            .iter()
            .find(|entry| entry["value"].as_u64() == Some(210))
            .expect("water class");
        assert_eq!(water["cells"].as_u64(), Some(5_675_161_787));
        assert_eq!(
            census["observation_count_counts"]
                .as_array()
                .expect("observation counts")
                .last()
                .expect("maximum observed count")["value"]
                .as_u64(),
            Some(994)
        );
    }

    #[test]
    fn copernicus_source_area_lookup_has_explicit_global_edge_ownership() {
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(900_000_000, -1_800_000_000).expect("northwest corner")
            )
            .expect("northwest source area"),
            (0, 0)
        );
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(0, 0).expect("prime-meridian origin")
            )
            .expect("central source area"),
            (32_400, 64_800)
        );
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(-900_000_000, 1_799_999_999)
                    .expect("southeast terminal coordinate")
            )
            .expect("southeast source area"),
            (64_799, 129_599)
        );
    }

    #[test]
    fn copernicus_l10_q32_target_support_has_a_stable_full_fingerprint() {
        let target: S2CellId = "1000010000000000".parse().expect("equatorial L10 cell");
        let samples = copernicus_land_cover_target_support_samples(target, 32)
            .expect("Copernicus target support");
        assert_eq!(samples.len(), 1_024);
        assert_eq!(
            copernicus_target_support_fingerprint(&samples).to_string(),
            "213ce73747aa77914e0fb5f41f7382377d4b8da7a872f7c43e79ad493794c834"
        );
        assert_eq!(
            samples.first(),
            Some(&CopernicusLandCoverSourceSample {
                latitude_e7: 11_747,
                longitude_e7: 11_668,
                source_row: 32_399,
                source_column: 64_800,
            })
        );
        assert_eq!(
            samples.last(),
            Some(&CopernicusLandCoverSourceSample {
                latitude_e7: 740_052,
                longitude_e7: 735_099,
                source_row: 32_373,
                source_column: 64_826,
            })
        );
        assert_eq!(
            samples
                .iter()
                .map(|sample| (sample.source_row, sample.source_column))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            729
        );
        for sample in samples {
            let coordinate = GeographicCoordinateE7::new(sample.latitude_e7, sample.longitude_e7)
                .expect("retained sample coordinate");
            assert_eq!(
                route_geographic_to_s2(coordinate, 10).expect("route retained target sample"),
                target
            );
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "a-tiny-civilization-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create test root");
        root
    }

    #[test]
    fn partial_download_publishes_once_without_replacement() {
        let root = temporary_root("source-publish");
        let destination = root.join("source.bin");
        let mut first = PartialDownload::create(&root, "source.bin").expect("first partial");
        first.file.write_all(b"first").expect("write first partial");
        first
            .persist_without_replacement(&destination)
            .expect("publish first source");

        let mut second = PartialDownload::create(&root, "source.bin").expect("second partial");
        second
            .file
            .write_all(b"second")
            .expect("write second partial");
        assert!(second.persist_without_replacement(&destination).is_err());
        assert_eq!(
            fs::read(&destination).expect("read published source"),
            b"first"
        );
        drop(second);
        assert_eq!(fs::read_dir(&root).expect("read test root").count(), 1);
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[cfg(unix)]
    #[test]
    fn destination_preparation_rejects_symlinked_parents() {
        use std::os::unix::fs::symlink;

        let root = temporary_root("source-path");
        let outside = temporary_root("source-outside");
        symlink(&outside, root.join("redirect")).expect("create parent symlink");
        let canonical_root = root.canonicalize().expect("canonical test root");
        assert!(prepare_destination(&canonical_root, "redirect/source.bin").is_err());
        fs::remove_dir_all(root).expect("remove test root");
        fs::remove_dir_all(outside).expect("remove outside root");
    }

    #[test]
    fn parses_a_null_record_in_a_polygon_shapefile() {
        let mut bytes = vec![0_u8; 112];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&56_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        bytes[104..108].copy_from_slice(&2_u32.to_be_bytes());
        let summary = parse_polygon_shapefile(&bytes).expect("valid minimal shapefile");
        assert_eq!(summary.records, 1);
        assert_eq!(summary.polygons, 0);
    }

    fn minimal_null_polygon_shapefile() -> Vec<u8> {
        let mut bytes = vec![0_u8; 112];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&56_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        bytes[104..108].copy_from_slice(&2_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn rejects_a_header_with_a_wrong_declared_length() {
        let mut bytes = vec![0_u8; 100];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&49_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        assert!(parse_polygon_shapefile(&bytes).is_err());
    }

    #[test]
    fn etopo_grid_bytes_preserve_provenance_dimensions_and_float_bits() {
        let snapshot = Digest::sha256(b"snapshot");
        let artifact = Digest::sha256(b"artifact");
        let encoded = encode_etopo_grid(5, snapshot, artifact, 1, 2, &[1.5, -42.25])
            .expect("encode a small ETOPO grid");
        assert_eq!(encoded.len(), ETOPO_GRID_HEADER_LENGTH + 8);
        assert_eq!(&encoded[0..8], ETOPO_GRID_MAGIC);
        assert_eq!(
            u16::from_le_bytes(encoded[8..10].try_into().expect("schema bytes")),
            1
        );
        assert_eq!(
            u16::from_le_bytes(encoded[10..12].try_into().expect("stride bytes")),
            5
        );
        assert_eq!(&encoded[12..44], snapshot.as_bytes());
        assert_eq!(&encoded[44..76], artifact.as_bytes());
        assert_eq!(
            u32::from_le_bytes(encoded[76..80].try_into().expect("latitude bytes")),
            1
        );
        assert_eq!(
            u32::from_le_bytes(encoded[80..84].try_into().expect("longitude bytes")),
            2
        );
        assert_eq!(
            u32::from_le_bytes(encoded[84..88].try_into().expect("first value bytes")),
            1.5_f32.to_bits()
        );
        assert_eq!(
            u32::from_le_bytes(encoded[88..92].try_into().expect("second value bytes")),
            (-42.25_f32).to_bits()
        );
    }

    #[test]
    fn etopo_grid_rejects_dimensions_that_disagree_with_values() {
        assert!(
            encode_etopo_grid(
                5,
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                1,
                2,
                &[1.5],
            )
            .is_err()
        );
    }

    #[test]
    fn etopo_centre_index_binds_sample_bits_to_exact_routed_centres() {
        let snapshot = Digest::sha256(b"snapshot");
        let artifact = Digest::sha256(b"artifact");
        let mut values = vec![0.0_f32; 180 * 360];
        values[0] = 1.5;
        let encoded = encode_etopo_centre_index(60, 10, snapshot, artifact, 180, 360, &values)
            .expect("encode one-degree ETOPO centre index");
        assert_eq!(
            encoded.len(),
            ETOPO_CENTRE_INDEX_HEADER_LENGTH + values.len() * ETOPO_CENTRE_INDEX_RECORD_LENGTH
        );
        assert_eq!(&encoded[0..8], ETOPO_CENTRE_INDEX_MAGIC);
        assert_eq!(
            u16::from_le_bytes(encoded[8..10].try_into().expect("schema bytes")),
            ETOPO_CENTRE_INDEX_SCHEMA_VERSION
        );
        assert_eq!(encoded[12], 10);
        assert_eq!(&encoded[16..48], snapshot.as_bytes());
        assert_eq!(&encoded[48..80], artifact.as_bytes());
        assert_eq!(
            u64::from_be_bytes(encoded[88..96].try_into().expect("S2 cell bytes")),
            0xa555_5500_0000_0000
        );
        assert_eq!(
            u32::from_le_bytes(encoded[96..100].try_into().expect("sample bytes")),
            1.5_f32.to_bits()
        );
    }

    #[test]
    fn etopo_centre_summary_revalidates_every_source_route_and_preserves_fixed_point_stats() {
        let root = temporary_root("etopo-centre-summary");
        let input = root.join("centres.bin");
        let output = root.join("summary.bin");
        let mut values = vec![1.5_f32; 180 * 360];
        values[0] = -2.25;
        fs::write(
            &input,
            encode_etopo_centre_index(
                60,
                10,
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                180,
                360,
                &values,
            )
            .expect("encode centre index"),
        )
        .expect("write centre index");

        derive_etopo_centre_summary(&input, 0, &output).expect("derive source-centre summary");
        let bytes = fs::read(&output).expect("read summary");
        assert_eq!(&bytes[..8], ETOPO_CENTRE_SUMMARY_MAGIC);
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().expect("summary schema")),
            ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION
        );
        assert_eq!(bytes[12], 10);
        assert_eq!(bytes[13], 0);
        let cells = u32::from_le_bytes(bytes[112..116].try_into().expect("summary cell count"));
        assert!(cells >= 6);
        let records = &bytes[ETOPO_CENTRE_SUMMARY_HEADER_LENGTH..];
        assert_eq!(
            records.len(),
            usize::try_from(cells).expect("cell count") * 40
        );
        let source_samples = records
            .chunks_exact(ETOPO_CENTRE_SUMMARY_RECORD_LENGTH)
            .map(|record| u64::from_le_bytes(record[8..16].try_into().expect("sample count")))
            .sum::<u64>();
        assert_eq!(source_samples, 64_800);
        assert_eq!(
            u64::from_le_bytes(bytes[116..124].try_into().expect("header sample count")),
            64_800
        );

        let mut tampered = fs::read(&input).expect("read centre index");
        tampered[88] ^= 1;
        let tampered_input = root.join("tampered-centres.bin");
        fs::write(&tampered_input, tampered).expect("write tampered index");
        assert!(
            derive_etopo_centre_summary(&tampered_input, 0, &root.join("rejected.bin")).is_err()
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn etopo_float_bits_round_to_signed_millimetres_without_host_float_math() {
        assert_eq!(
            f32_bits_to_rounded_millimetres(1.5_f32.to_bits()).expect("finite f32"),
            1_500
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres((-2.25_f32).to_bits()).expect("finite f32"),
            -2_250
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres(0.0005_f32.to_bits()).expect("finite f32"),
            1
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres((-0.0005_f32).to_bits()).expect("finite f32"),
            -1
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres(1).expect("subnormal f32"),
            0
        );
        assert!(f32_bits_to_rounded_millimetres(f32::NAN.to_bits()).is_err());
        assert!(f32_bits_to_rounded_millimetres(f32::INFINITY.to_bits()).is_err());
    }

    #[test]
    fn ephemeris_binary64_values_cross_to_fixed_scale_with_nearest_even_rounding() {
        assert_eq!(
            f64_bits_to_rounded_scaled_integer(1.5_f64.to_bits(), 1).expect("exact positive tie"),
            2
        );
        assert_eq!(
            f64_bits_to_rounded_scaled_integer(2.5_f64.to_bits(), 1).expect("exact even tie"),
            2
        );
        assert_eq!(
            f64_bits_to_rounded_scaled_integer((-1.5_f64).to_bits(), 1)
                .expect("exact negative tie"),
            -2
        );
        assert_eq!(
            f64_bits_to_rounded_scaled_integer(42.0_f64.to_bits(), 1_000_000)
                .expect("kilometres to millimetres"),
            42_000_000
        );
        assert!(f64_bits_to_rounded_scaled_integer(f64::NAN.to_bits(), 1_000_000).is_err());
    }

    #[test]
    fn etopo_cell_centres_keep_the_pinned_area_raster_lattice() {
        let first_support = etopo_cell_support(0, 0).expect("first cell support");
        let first = first_support.centre;
        assert_eq!(
            (
                first.latitude_half_arcseconds(),
                first.longitude_half_arcseconds()
            ),
            (-647_940, -1_295_940)
        );
        assert_eq!(
            (
                first_support.south_boundary_half_arcseconds,
                first_support.north_boundary_half_arcseconds,
                first_support.west_boundary_half_arcseconds,
                first_support.east_boundary_half_arcseconds,
            ),
            (-648_000, -647_880, -1_296_000, -1_295_880)
        );
        let last_support = etopo_cell_support(10_799, 21_599).expect("last cell support");
        let last = last_support.centre;
        assert_eq!(
            (
                last.latitude_half_arcseconds(),
                last.longitude_half_arcseconds()
            ),
            (647_940, 1_295_940)
        );
        assert_eq!(
            (
                last_support.south_boundary_half_arcseconds,
                last_support.north_boundary_half_arcseconds,
                last_support.west_boundary_half_arcseconds,
                last_support.east_boundary_half_arcseconds,
            ),
            (647_880, 648_000, 1_295_880, 1_296_000)
        );
        let seam_left = etopo_cell_support(5_400, 0).expect("western seam support");
        let seam_right = etopo_cell_support(5_400, 21_599).expect("eastern seam support");
        assert_eq!(seam_left.west_boundary_half_arcseconds, -1_296_000);
        assert_eq!(seam_right.east_boundary_half_arcseconds, 1_296_000);
        assert!(etopo_cell_support(10_800, 0).is_err());
        assert!(etopo_cell_support(0, 21_600).is_err());
    }

    #[test]
    fn coordinate_lattice_quantization_rejects_off_grid_values() {
        assert_eq!(
            f64_to_half_arcseconds(-647_940.0 / 7_200.0).expect("valid latitude"),
            -647_940
        );
        assert_eq!(
            f64_to_half_arcseconds(1_295_940.0 / 7_200.0).expect("valid longitude"),
            1_295_940
        );
        assert!(f64_to_half_arcseconds(0.000_000_1).is_err());
        assert!(f64_to_half_arcseconds(f64::NAN).is_err());
    }

    #[test]
    fn chelsa_cell_addresses_are_zero_based_and_bounded_by_the_pinned_grid() {
        validate_chelsa_cell_address(0, 0).expect("first CHELSA cell");
        validate_chelsa_cell_address(CHELSA_LATITUDE_CELLS - 1, CHELSA_LONGITUDE_CELLS - 1)
            .expect("last CHELSA cell");
        assert!(validate_chelsa_cell_address(CHELSA_LATITUDE_CELLS, 0).is_err());
        assert!(validate_chelsa_cell_address(0, CHELSA_LONGITUDE_CELLS).is_err());
    }

    #[test]
    fn nearest_chelsa_axis_cell_uses_exact_lattice_and_lower_index_ties() {
        let axis = [-100, 0, 100];
        assert_eq!(nearest_sorted_e7_index(&axis, -100).expect("first"), 0);
        assert_eq!(nearest_sorted_e7_index(&axis, 100).expect("last"), 2);
        assert_eq!(nearest_sorted_e7_index(&axis, 49).expect("near lower"), 1);
        assert_eq!(nearest_sorted_e7_index(&axis, 50).expect("tie"), 1);
        assert!(nearest_sorted_e7_index(&axis, -101).is_err());
        assert!(nearest_sorted_e7_index(&axis, 101).is_err());
    }

    #[test]
    fn chelsa_documented_tas_transform_uses_integer_millicelsius() {
        assert_eq!(
            chelsa_raw_tas_to_millicelsius(2992.0).expect("documented conversion"),
            26_050
        );
        assert!(chelsa_raw_tas_to_millicelsius(2992.5).is_err());
    }

    #[test]
    fn shapefile_ring_uses_even_odd_membership() {
        let mut points = Vec::new();
        for (x, y) in [
            (0.0_f64, 0.0_f64),
            (2.0_f64, 0.0_f64),
            (2.0_f64, 2.0_f64),
            (0.0_f64, 2.0_f64),
            (0.0_f64, 0.0_f64),
        ] {
            points.extend_from_slice(&x.to_le_bytes());
            points.extend_from_slice(&y.to_le_bytes());
        }
        assert!(point_in_shapefile_ring(&points, 0, 5, 10_000_000, 10_000_000).expect("inside"));
        assert!(!point_in_shapefile_ring(&points, 0, 5, 30_000_000, 10_000_000).expect("outside"));
    }

    #[test]
    fn prepared_land_index_preserves_even_odd_membership_across_buckets() {
        // The two vertical sides of a 0°..2° square cross both 0° and 1° buckets;
        // the horizontal sides intentionally do not enter a horizontal-ray index.
        let mut buckets = vec![Vec::new(); NATURAL_EARTH_LATITUDE_BUCKETS];
        for bucket_edges in buckets.iter_mut().take(93).skip(90) {
            bucket_edges.extend([
                NaturalEarthEdge {
                    current_x: 20_000_000,
                    current_y: 20_000_000,
                    previous_x: 20_000_000,
                    previous_y: 0,
                },
                NaturalEarthEdge {
                    current_x: 0,
                    current_y: 0,
                    previous_x: 0,
                    previous_y: 20_000_000,
                },
            ]);
        }
        let prepared = PreparedNaturalEarthLand {
            edges_by_latitude_bucket: buckets,
        };
        assert!(prepared.contains_point(10_000_000, 10_000_000));
        assert!(!prepared.contains_point(30_000_000, 10_000_000));
        assert!(prepared.contains_point(10_000_000, 19_000_000));
        assert!(!prepared.contains_point(10_000_000, 30_000_000));
    }

    #[test]
    fn shapefile_ieee754_coordinates_round_to_the_e7_lattice_without_host_float_math() {
        assert_eq!(
            ieee754_degrees_bits_to_e7((-93.125_f64).to_bits()).expect("finite coordinate"),
            -931_250_000
        );
        assert_eq!(
            ieee754_degrees_bits_to_e7(0.5_f64.to_bits()).expect("finite coordinate"),
            5_000_000
        );
        assert!(ieee754_degrees_bits_to_e7(f64::NAN.to_bits()).is_err());
    }

    #[test]
    fn etopo_one_point_quadrature_is_the_exact_source_centre_route() {
        let support = etopo_cell_support(5_400, 10_800).expect("ETOP0 source cell");
        let expected = route_half_arcsecond_to_s2(support.centre, 10).expect("route centre");
        let quadrature =
            etopo_cell_quadrature(5_400, 10_800, 10, 1).expect("one-point source quadrature");
        assert_eq!(
            quadrature,
            std::collections::BTreeMap::from([(expected, 1)])
        );
    }

    #[test]
    fn etopo_quadrature_uses_only_interior_exact_lattice_points() {
        let quadrature =
            etopo_cell_quadrature(5_400, 10_800, 10, 4).expect("four-by-four source quadrature");
        assert_eq!(quadrature.values().copied().sum::<u32>(), 16);
        assert!(quadrature.values().all(|count| *count > 0));
        assert!(etopo_cell_quadrature(5_400, 10_800, 10, 7).is_err());
    }

    #[test]
    fn unordered_full_batch_accumulation_has_the_same_sorted_summary() {
        let samples = [
            (0, 0, (-10.25_f32).to_bits()),
            (1, 1, 0.5_f32.to_bits()),
            (5_400, 10_800, 123.75_f32.to_bits()),
            (
                ETOPO_LATITUDE_CELLS as u32 - 1,
                ETOPO_LONGITUDE_CELLS as u32 - 1,
                4.0_f32.to_bits(),
            ),
        ];
        let mut ordered = BTreeMap::new();
        let mut unordered = HashMap::new();
        for (row, column, value_bits) in samples {
            accumulate_etopo_cell_quadrature(&mut ordered, row, column, value_bits, 10, 4)
                .expect("ordered accumulation");
            accumulate_etopo_cell_quadrature_unordered(
                &mut unordered,
                row,
                column,
                value_bits,
                10,
                4,
            )
            .expect("unordered accumulation");
        }
        assert_eq!(unordered.into_iter().collect::<BTreeMap<_, _>>(), ordered);
    }

    #[test]
    fn packs_complete_quadrature_summaries_into_a_canonical_terrain_tile() {
        let container: S2CellId = "1000010000000000".parse().expect("valid L10 container");
        let mut summaries = std::collections::BTreeMap::new();
        for (index, child) in container
            .children()
            .expect("children")
            .into_iter()
            .enumerate()
        {
            let mut stats = EtopoCentreSummaryStats::default();
            stats
                .add_weighted(i64::try_from(index).expect("index fits") * 1_000, 16)
                .expect("weighted support");
            summaries.insert(child, stats);
        }
        let tile = pack_etopo_terrain_tile(
            "bedrock-relief",
            Digest::sha256(b"snapshot"),
            Digest::sha256(b"artifact"),
            4,
            container,
            11,
            &summaries,
        )
        .expect("complete terrain tile");
        assert_eq!(tile.cells.len(), 4);
        assert!(tile.cells.iter().all(|cell| cell.support_samples == 16));
        assert_eq!(tile.cells[3].mean_millimetres, 3_000);
        summaries.pop_first();
        assert!(
            pack_etopo_terrain_tile(
                "bedrock-relief",
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                4,
                container,
                11,
                &summaries,
            )
            .is_err()
        );
    }

    #[test]
    fn global_s2_container_enumeration_is_complete_and_canonical() {
        let cells = global_s2_cells_at_level(1).expect("global L1 cells");
        assert_eq!(cells.len(), 24);
        assert!(cells.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(cells.iter().filter(|cell| cell.face() == 0).count(), 4);
    }

    #[test]
    fn miniature_terrain_layer_writes_and_reloads_every_packed_tile() {
        let root = temporary_root("terrain-layer");
        let mut summaries = std::collections::BTreeMap::new();
        for cell in global_s2_cells_at_level(1).expect("global target cells") {
            let mut stats = EtopoCentreSummaryStats::default();
            stats.add_weighted(42, 4).expect("support");
            summaries.insert(cell, stats);
        }
        let (root_path, root_bytes) = write_packed_etopo_terrain_layer(
            &root,
            EtopoTerrainPackingProfile {
                layer_id: "bedrock-relief",
                source_snapshot_digest: Digest::sha256(b"snapshot"),
                source_artifact_digest: Digest::sha256(b"artifact"),
                points_per_axis: 4,
                container_s2_level: 0,
                target_s2_level: 1,
            },
            &summaries,
        )
        .expect("write miniature terrain layer");
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        assert_eq!(
            fs::read(root.join(&root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_etopo_terrain_layer(&root, "bedrock-relief", 0, 1, 4)
            .expect("independently inspect miniature terrain layer");
        for entry in index.entries {
            let tile = PackedScalarTerrainTile::from_canonical_slice(
                &fs::read(root.join(entry.artifact.path)).expect("tile bytes"),
            )
            .expect("packed terrain tile");
            assert_eq!(tile.cells.len(), 4);
            assert!(tile.cells.iter().all(|cell| cell.mean_millimetres == 42));
        }
        fs::remove_dir_all(root).expect("remove miniature terrain layer");
    }

    #[test]
    fn miniature_land_reference_layer_writes_and_reloads_every_packed_tile() {
        let root = temporary_root("land-reference-layer");
        let source = minimal_null_polygon_shapefile();
        let prepared =
            PreparedNaturalEarthLand::from_shapefile(&source).expect("validated empty source");
        let (root_path, root_bytes) = write_packed_natural_earth_land_reference_layer(
            &root,
            "land-reference",
            Digest::sha256(b"snapshot"),
            Digest::sha256(b"artifact"),
            &prepared,
            0,
            1,
        )
        .expect("write miniature land-reference layer");
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        assert_eq!(
            fs::read(root.join(&root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_natural_earth_land_reference_layer(&root, "land-reference", 0, 1)
            .expect("independently inspect miniature land-reference layer");
        // Replaying an interrupted writer against an already-complete hidden tree
        // must verify and reuse its exact canonical artifacts rather than overwrite.
        let (resumed_root_path, resumed_root_bytes) =
            write_packed_natural_earth_land_reference_layer(
                &root,
                "land-reference",
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                &prepared,
                0,
                1,
            )
            .expect("resume miniature land-reference layer");
        assert_eq!(resumed_root_path, root_path);
        assert_eq!(resumed_root_bytes, root_bytes);
        for entry in index.entries {
            let tile = PackedBooleanFieldTile::from_canonical_slice(
                &fs::read(root.join(entry.artifact.path)).expect("tile bytes"),
            )
            .expect("packed Boolean tile");
            assert_eq!(tile.cells.len(), 4);
            assert!(tile.cells.iter().all(|cell| cell.true_samples == 0));
        }
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn miniature_land_cover_layer_conserves_support_and_resumes_without_recalculation() {
        let root = temporary_root("land-cover-layer");
        let profile = CopernicusLandCoverPackingProfile {
            layer_id: "observed-land-cover",
            source_snapshot_digest: Digest::sha256(b"snapshot"),
            source_artifact_digest: Digest::sha256(b"artifact"),
            container_s2_level: 0,
            target_s2_level: 1,
            points_per_axis: 2,
        };
        let mut lookup = ConstantLandCoverLookup { calls: 0 };
        let (root_path, root_bytes, target_cells) =
            write_packed_copernicus_land_cover_layer(&root, profile, &mut lookup)
                .expect("write miniature land-cover layer");
        assert_eq!(target_cells, 24);
        assert_eq!(lookup.calls, 96);
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        assert_eq!(
            fs::read(root.join(&root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_copernicus_land_cover_layer(&root, "observed-land-cover", 0, 1, 2)
            .expect("independently inspect miniature land-cover layer");
        for entry in &index.entries {
            let tile = PackedLandCoverEvidenceTile::from_canonical_slice(
                &fs::read(root.join(&entry.artifact.path)).expect("tile bytes"),
            )
            .expect("packed land-cover tile");
            assert_eq!(tile.cells.len(), 4);
            assert!(tile.cells.iter().all(|cell| {
                cell.support_samples == 4
                    && cell.class_counts
                        == vec![LandCoverClassCount {
                            class_value: 130,
                            samples: 4,
                        }]
                    && cell.observation_count_sum == 168
            }));
        }
        let mut resumed_lookup = ConstantLandCoverLookup { calls: 0 };
        let (resumed_path, resumed_bytes, resumed_cells) =
            write_packed_copernicus_land_cover_layer(&root, profile, &mut resumed_lookup)
                .expect("resume miniature land-cover layer");
        assert_eq!(resumed_path, root_path);
        assert_eq!(resumed_bytes, root_bytes);
        assert_eq!(resumed_cells, 24);
        assert_eq!(resumed_lookup.calls, 0);
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn soilgrids_chunk_inspection_preserves_integer_sentinel_and_float_finiteness() {
        let integers =
            inspect_soilgrids_chunk(7, 2, 2, &TiffDecodingResult::I16(vec![-32_768, 1, 2, 9]));
        assert_eq!(integers.chunk_index, 7);
        assert_eq!(integers.sample_type, "i16");
        assert_eq!(integers.sample_count, 4);
        assert_eq!(integers.finite_minimum.as_deref(), Some("-32768"));
        assert_eq!(integers.finite_maximum.as_deref(), Some("9"));
        assert_eq!(integers.non_finite_samples, 0);

        let floats = inspect_soilgrids_chunk(
            8,
            3,
            1,
            &TiffDecodingResult::F32(vec![f32::NAN, -1.5, 4.25]),
        );
        assert_eq!(floats.sample_type, "f32");
        assert_eq!(floats.finite_minimum.as_deref(), Some("-1.5"));
        assert_eq!(floats.finite_maximum.as_deref(), Some("4.25"));
        assert_eq!(floats.non_finite_samples, 1);
    }

    #[test]
    fn soilgrids_grouped_sampling_reads_each_source_chunk_for_all_requested_targets() {
        let cells = global_s2_cells_at_level(1).expect("miniature target grid");
        let grid = SoilgridsOverviewGrid {
            west_e12: 0,
            north_e12: 0,
            pixel_size_e12: 1,
            width: 256,
            height: 256,
        };
        let projection = vec![
            SoilgridsProjectedCell {
                s2_cell_id: cells[0],
                easting_e12: 0,
                northing_e12: 0,
            },
            SoilgridsProjectedCell {
                s2_cell_id: cells[1],
                easting_e12: 1,
                northing_e12: -1,
            },
            SoilgridsProjectedCell {
                s2_cell_id: cells[2],
                easting_e12: 130,
                northing_e12: -129,
            },
            SoilgridsProjectedCell {
                s2_cell_id: cells[3],
                easting_e12: 0,
                northing_e12: 1,
            },
        ];
        let groups = group_soilgrids_projection_by_source_chunk(&projection, grid)
            .expect("group projected targets");
        assert_eq!(groups.requests.len(), 4);
        assert_eq!(groups.requests[0].len(), 2);
        assert_eq!(groups.requests[3].len(), 1);
        assert!(groups.requests[1].is_empty());
        assert!(groups.requests[2].is_empty());

        let mut values = vec![[[SOILGRIDS_NO_DATA_VALUE; 3]; 9]; projection.len()];
        apply_soilgrids_decoded_chunk(
            SoilgridsDecodedChunk {
                decoded: &TiffDecodingResult::I16(vec![11, 12, 13, 14]),
                data_width: 2,
                data_height: 2,
                source_width: 2,
                source_height: 2,
                property_index: 0,
                quantile_index: 1,
            },
            &groups.requests[0],
            &mut values,
        )
        .expect("apply one decoded source chunk");
        assert_eq!(values[0][0][1], 11);
        assert_eq!(values[1][0][1], 14);
        assert_eq!(values[2][0][1], SOILGRIDS_NO_DATA_VALUE);
        assert_eq!(values[3][0][1], SOILGRIDS_NO_DATA_VALUE);
    }

    #[test]
    fn miniature_soilgrids_layer_round_trips_through_independent_inspection() {
        let root = temporary_root("soilgrids-layer");
        let projected_cells = global_s2_cells_at_level(1)
            .expect("miniature target grid")
            .into_iter()
            .map(|s2_cell_id| SoilgridsProjectedCell {
                s2_cell_id,
                easting_e12: 0,
                northing_e12: 0,
            })
            .collect::<Vec<_>>();
        let values = projected_cells
            .iter()
            .enumerate()
            .map(|(cell_index, _)| {
                std::array::from_fn(|property_index| {
                    std::array::from_fn(|quantile_index| {
                        if cell_index == 0 && property_index == 0 && quantile_index == 0 {
                            SOILGRIDS_NO_DATA_VALUE
                        } else {
                            i16::try_from(property_index * 100 + quantile_index * 10 + cell_index)
                                .expect("miniature source value")
                        }
                    })
                })
            })
            .collect::<Vec<SoilgridsRawCellValues>>();
        let property_sources = (0..SOILGRIDS_TOPSOIL_PROPERTIES.len())
            .map(|property_index| SoilGridsPropertySource {
                property: soilgrids_property(property_index).expect("canonical property"),
                quantile_artifact_digests: std::array::from_fn(|quantile_index| {
                    Digest::sha256(&[property_index as u8, quantile_index as u8])
                }),
            })
            .collect::<Vec<_>>();
        let source_set_digest = soilgrids_source_set_digest(&property_sources);
        let (root_path, root_bytes) = write_packed_soilgrids_topsoil_layer(
            &root,
            SoilgridsLayerPackingProfile {
                layer_id: "soilgrids-topsoil",
                source_inventory_digest: Digest::sha256(b"inventory"),
                source_set_digest,
                property_sources: &property_sources,
                container_s2_level: 0,
                target_s2_level: 1,
            },
            &projected_cells,
            &values,
        )
        .expect("write miniature SoilGrids layer");
        assert_eq!(
            fs::read(root.join(root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_soilgrids_topsoil_layer(&root, "soilgrids-topsoil", 0, 1)
            .expect("independently inspect miniature SoilGrids layer");
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        let first_tile = PackedSoilGridsTopsoilTile::from_canonical_slice(
            &fs::read(root.join(&index.entries[0].artifact.path)).expect("first tile bytes"),
        )
        .expect("first tile");
        assert_eq!(first_tile.cells.len(), 4);
        assert_eq!(
            first_tile.cells[0].property_values[0].q0_05,
            SOILGRIDS_NO_DATA_VALUE
        );
        fs::remove_dir_all(root).expect("remove SoilGrids test root");
    }

    #[test]
    fn gbif_catalog_strings_round_trip_utf8_without_delimiter_assumptions() {
        let mut bytes = Vec::new();
        write_length_prefixed_utf8(&mut bytes, "Loxodonta africana\t🐘")
            .expect("write catalog string");
        let mut input = bytes.as_slice();
        assert_eq!(
            read_length_prefixed_utf8(&mut input).expect("read catalog string"),
            "Loxodonta africana\t🐘"
        );
        assert!(input.is_empty());
    }

    #[test]
    fn fauna_trait_parser_preserves_legacy_encoding_and_quoted_newlines() {
        assert_eq!(decode_windows_1252(b"range\x96map"), "range–map");
        assert_eq!(
            parse_delimited_records(
                "Scientific\tReference\nTestus animalia\t\"Ref_1\n\"\n",
                '\t'
            )
            .expect("parse retained trait rows"),
            vec![
                vec!["Scientific".to_owned(), "Reference".to_owned()],
                vec!["Testus animalia".to_owned(), "Ref_1\n".to_owned()],
            ]
        );
    }

    #[test]
    fn fauna_trait_inspection_excludes_completely_blank_fixed_width_rows() {
        let root = temporary_root("fauna-trait-empty-rows");
        let input = root.join("traits.tsv");
        fs::write(
            &input,
            "Scientific\tReference\nTestus animalia\tRef_1\n\t\n",
        )
        .expect("write fixture");
        assert_eq!(
            read_source_scientific_names(&input, '\t', "Scientific", SourceTextEncoding::Utf8,)
                .expect("inspect fixture"),
            (1, 0, BTreeSet::from(["Testus animalia".to_owned()]),)
        );
        fs::remove_dir_all(root).expect("remove fixture root");
    }

    #[test]
    fn jrc_horizontal_predictor_and_coordinate_names_are_exact() {
        let mut differences = [10_u8, 2, 254, 5];
        undo_horizontal_u8_predictor(&mut differences);
        assert_eq!(differences, [10, 12, 10, 15]);
        assert_eq!(jrc_coordinate_code(-180, 'W', 'E'), "180W");
        assert_eq!(jrc_coordinate_code(0, 'S', 'N'), "0N");
    }

    #[test]
    fn jrc_coordinate_lookup_preserves_tile_boundaries_and_polar_absence() {
        let address = jrc_occurrence_sample_address(
            GeographicCoordinateE7::new(351_234_567, -927_654_321).expect("coordinate"),
        )
        .expect("covered JRC coordinate");
        assert_eq!(address.tile.west_degrees, -100);
        assert_eq!(address.tile.north_degrees, 40);
        assert_eq!(address.row, 19_506);
        assert_eq!(address.column, 28_938);
        assert_eq!(
            address.tile.relative_path(),
            "jrc-global-surface-water-v1-5-2024/occurrence/occurrence_100W_40N_v1_5_2024.tif"
        );

        let equator =
            jrc_occurrence_sample_address(GeographicCoordinateE7::new(0, 0).expect("equator"))
                .expect("equator is covered");
        assert_eq!(equator.tile.north_degrees, 0);
        assert_eq!(equator.row, 0);
        assert_eq!(equator.column, 0);
        assert!(
            jrc_occurrence_sample_address(
                GeographicCoordinateE7::new(-600_000_000, 0).expect("60 south")
            )
            .is_none()
        );
        assert!(
            jrc_occurrence_sample_address(
                GeographicCoordinateE7::new(800_000_001, 0).expect("north gap")
            )
            .is_none()
        );
    }

    #[test]
    fn spk_chebyshev_evaluation_matches_the_defining_polynomials() {
        // 1*T0(x) + 2*T1(x) + 3*T2(x), at x=0.5, is 0.5.
        assert_eq!(
            evaluate_chebyshev(&[1.0, 2.0, 3.0], 0.5).expect("Chebyshev evaluation"),
            0.5
        );
        assert_eq!(
            daf_control_integer(62.0, "test").expect("integer control"),
            62
        );
        assert!(daf_control_integer(1.5, "test").is_err());
    }

    #[test]
    fn terrain_release_staging_is_hidden_until_atomic_rename() {
        let root = temporary_root("terrain-staging");
        let output = root.join("release");
        let staging = prepare_terrain_layer_staging_directory(&output).expect("stage release");
        assert_eq!(staging, root.join(".release.staging"));
        assert!(!output.exists());
        assert!(staging.is_dir());
        fs::write(staging.join("complete"), b"complete").expect("write staged marker");
        fs::rename(&staging, &output).expect("atomically publish staged release");
        assert!(output.join("complete").is_file());
        assert!(prepare_terrain_layer_staging_directory(&output).is_err());
        fs::remove_dir_all(root).expect("remove terrain staging root");
    }

    #[test]
    fn provisional_founders_keep_every_seeded_taxon_without_metabolic_coverage() {
        let species = |identifier: &str, name: &str| {
            SpeciesIdentity::new(
                "gbif",
                identifier,
                name,
                format!("https://www.gbif.org/species/{identifier}"),
            )
            .expect("test species")
        };
        let selection = FaunaSeededSelection {
            selection_schema_version: 1,
            candidate_set_digest: Digest::sha256(b"test candidates"),
            world_seed: WorldSeed::new(42),
            species_limit: 2,
            identity_tier_policy: None,
            selected_candidates: vec![
                world_data::FaunaRangeCandidate {
                    species: species("10", "Ten testii"),
                    inaturalist_taxon_id: 10,
                    range_package: "test-range".to_owned(),
                    range_feature_fid: 10,
                },
                world_data::FaunaRangeCandidate {
                    species: species("2", "Two testii"),
                    inaturalist_taxon_id: 2,
                    range_package: "test-range".to_owned(),
                    range_feature_fid: 2,
                },
            ],
        };
        let entries = provisional_founder_entries(&selection);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].species.identifier, "2");
        assert_eq!(entries[1].species.identifier, "10");
        for entry in entries {
            assert_eq!(entry.initial_individual_count, 2);
            assert_eq!(
                entry
                    .birth_category_counts
                    .iter()
                    .map(|count| (count.category.as_str(), count.count))
                    .collect::<Vec<_>>(),
                vec![("female", 1), ("male", 1)]
            );
        }
    }

    #[test]
    fn unsourced_body_metabolism_is_structurally_an_engineering_assumption() {
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("test species");
        let commitment = engineering_metabolic_commitment(species.clone());
        assert_eq!(
            commitment.evidence_basis,
            PhysiologicalEvidenceBasis::EngineeringAssumption
        );
        assert_eq!(commitment.observed_species, species);
        commitment.validate().expect("valid explicit assumption");
    }

    #[test]
    fn provisional_life_history_uses_simulation_time_not_arbitrary_tick_counts() {
        let human = provisional_life_history("homo_sapiens");
        assert_eq!(duration_ticks(human.initial_age_seconds, 300), 2_102_400);
        assert_eq!(duration_ticks(human.maturity_age_seconds, 300), 1_576_800);
        assert_eq!(duration_ticks(human.development_seconds, 300), 80_640);
        assert_eq!(duration_ticks(human.recovery_seconds, 300), 105_120);
        assert_eq!(
            duration_ticks(human.opportunity_interval_seconds, 300),
            8_064
        );
        assert_eq!(human.initiation_probability_millionths, 200_000);

        let insect = provisional_life_history("insecta_4");
        assert_eq!(duration_ticks(insect.development_seconds, 300), 4_032);
        assert_eq!(duration_ticks(301, 300), 2);
    }

    #[test]
    fn provisional_body_profiles_use_source_maturity_per_category_with_explicit_fallback() {
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("test species");
        let source_record_digest = Digest::sha256(b"female maturity source row");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: world_data::FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION,
            source_artifact_digest: Digest::sha256(b"amniote source artifact"),
            profiles: vec![world_data::FaunaPhysiologyProfile {
                species: species.clone(),
                trait_id: "female-maturity".to_owned(),
                value: world_data::ScaledFaunaTraitValue {
                    value: 123,
                    decimal_places: 0,
                    unit: "d".to_owned(),
                },
                source: world_data::FaunaEvidenceSource::AmnioteLifeHistoryAugust2015,
                source_field: "female_maturity_d".to_owned(),
                source_record_id: "gbif:2436436:female-maturity".to_owned(),
                source_record_digest,
                evidence_basis: world_data::FaunaEvidenceBasis::SourceCompiledSpeciesAggregate,
            }],
        };
        profiles.validate().expect("valid test profile set");
        let profile_set_digest = Digest::canonical(&profiles).expect("profile set digest");
        let entry = engineering_body_profile_entry(
            species.clone(),
            engineering_metabolic_commitment(species),
            300,
            "homo_sapiens",
            Some(&(profiles, profile_set_digest)),
            None,
        )
        .expect("body profile");

        let female = &entry.reproductive_physiology.category_maturity[0];
        assert_eq!(female.category.as_str(), "female");
        assert_eq!(female.maturity_age_ticks, duration_ticks(days(123), 300));
        assert_eq!(
            female.evidence_basis,
            PhysiologicalEvidenceBasis::LiteratureApproximation
        );
        assert_eq!(female.source_profile_set_digest, profile_set_digest);
        assert_eq!(female.source_record_digest, source_record_digest);

        let male = &entry.reproductive_physiology.category_maturity[1];
        assert_eq!(male.category.as_str(), "male");
        assert_eq!(
            male.evidence_basis,
            PhysiologicalEvidenceBasis::EngineeringAssumption
        );
        assert_eq!(
            male.maturity_age_ticks,
            duration_ticks(
                provisional_life_history("homo_sapiens").maturity_age_seconds,
                300
            )
        );
        entry
            .reproductive_physiology
            .validate()
            .expect("mixed-evidence commitment is valid");
    }

    #[test]
    fn provisional_body_profiles_retain_exact_taxon_body_mass_without_making_it_causal() {
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("test species");
        let record_digest = Digest::sha256(b"Elton body-mass row");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: world_data::FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION,
            source_artifact_digest: Digest::sha256(b"Elton source artifact"),
            profiles: vec![world_data::FaunaPhysiologyProfile {
                species: species.clone(),
                trait_id: "adult-body-mass".to_owned(),
                value: world_data::ScaledFaunaTraitValue {
                    value: 70_000,
                    decimal_places: 0,
                    unit: "g".to_owned(),
                },
                source: world_data::FaunaEvidenceSource::EltonTraitsV1_0,
                source_field: "BodyMass-Value".to_owned(),
                source_record_id: "elton-mammal-line-1".to_owned(),
                source_record_digest: record_digest,
                evidence_basis: world_data::FaunaEvidenceBasis::SourceCompiledSpeciesAggregate,
            }],
        };
        profiles.validate().expect("valid test profile set");
        let profile_set_digest = Digest::canonical(&profiles).expect("profile set digest");
        let entry = engineering_body_profile_entry(
            species.clone(),
            engineering_metabolic_commitment(species),
            300,
            "homo_sapiens",
            None,
            Some(&(profiles, profile_set_digest)),
        )
        .expect("body profile");
        let mass = entry.adult_body_mass.expect("adult body mass");
        assert_eq!(mass.mass_grams_value, 70_000);
        assert_eq!(mass.mass_grams_decimal_places, 0);
        assert_eq!(mass.profile_set_digest, profile_set_digest);
        assert_eq!(mass.source_record_digest, record_digest);
        assert_eq!(
            mass.evidence_basis,
            PhysiologicalEvidenceBasis::LiteratureApproximation
        );
        mass.validate().expect("valid retained body mass");
    }

    #[test]
    fn quicknet_signature_and_world_seed_derivation_are_pinned() {
        let signature = "b75c69d0b72a5d906e854e808ba7e2accb1542ac355ae486d591aa9d43765482e26cd02df835d3546d23c4b13e0dfc92";
        let beacon = DrandBeacon {
            round: 123,
            randomness: hex::encode(derive_randomness(
                &hex::decode(signature).expect("fixed signature hex"),
            )),
            signature: signature.to_owned(),
            previous_signature: None,
        };
        verify_quicknet_beacon(&beacon).expect("verified quicknet test vector");
        let (seed, digest) = derive_public_world_seed(&beacon).expect("derived seed");
        assert_eq!(
            seed,
            WorldSeed::new(16_962_325_827_322_022_972),
            "world-seed byte order and truncation are protocol"
        );
        assert_eq!(
            digest.to_string(),
            "eb6649783df68c3c9f31428147d2e93cfdcc46c8f187193a5bfcd8fdd3952ab9"
        );
        assert_eq!(
            derive_public_world_id(digest).to_string(),
            "50cd61e5-7a00-56a7-b99a-639e1b430683"
        );

        let mut changed = beacon;
        changed.randomness.replace_range(0..2, "00");
        assert!(verify_quicknet_beacon(&changed).is_err());
    }

    #[test]
    fn public_seed_commitment_requires_a_future_verified_round() {
        let signature = "b75c69d0b72a5d906e854e808ba7e2accb1542ac355ae486d591aa9d43765482e26cd02df835d3546d23c4b13e0dfc92";
        let observed_beacon = DrandBeacon {
            round: 123,
            randomness: hex::encode(derive_randomness(
                &hex::decode(signature).expect("fixed signature hex"),
            )),
            signature: signature.to_owned(),
            previous_signature: None,
        };
        let mut commitment = PublicSeedCommitment {
            schema_version: PUBLIC_SEED_SCHEMA_VERSION,
            chain_hash: QUICKNET_CHAIN_HASH.to_owned(),
            public_key: QUICKNET_PUBLIC_KEY.to_owned(),
            scheme: QUICKNET_SCHEME.to_owned(),
            period_seconds: QUICKNET_PERIOD_SECONDS,
            genesis_unix_seconds: QUICKNET_GENESIS_UNIX_SECONDS,
            target_round: 323,
            target_unix_seconds: quicknet_round_unix_seconds(323).expect("round time"),
            minimum_unrevealed_rounds: MINIMUM_UNREVEALED_ROUNDS,
            observed_beacon,
            derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
        };
        validate_public_seed_commitment(&commitment).expect("valid future commitment");
        commitment.target_round = 322;
        commitment.target_unix_seconds = quicknet_round_unix_seconds(322).expect("round time");
        assert!(validate_public_seed_commitment(&commitment).is_err());
    }

    #[test]
    fn resolved_seed_binds_beacon_seed_and_world_id() {
        let observed_signature = "b75c69d0b72a5d906e854e808ba7e2accb1542ac355ae486d591aa9d43765482e26cd02df835d3546d23c4b13e0dfc92";
        let commitment = PublicSeedCommitment {
            schema_version: PUBLIC_SEED_SCHEMA_VERSION,
            chain_hash: QUICKNET_CHAIN_HASH.to_owned(),
            public_key: QUICKNET_PUBLIC_KEY.to_owned(),
            scheme: QUICKNET_SCHEME.to_owned(),
            period_seconds: QUICKNET_PERIOD_SECONDS,
            genesis_unix_seconds: QUICKNET_GENESIS_UNIX_SECONDS,
            target_round: 323,
            target_unix_seconds: quicknet_round_unix_seconds(323).expect("round time"),
            minimum_unrevealed_rounds: MINIMUM_UNREVEALED_ROUNDS,
            observed_beacon: DrandBeacon {
                round: 123,
                randomness: hex::encode(derive_randomness(
                    &hex::decode(observed_signature).expect("observed signature"),
                )),
                signature: observed_signature.to_owned(),
                previous_signature: None,
            },
            derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
        };
        let target_beacon = DrandBeacon {
            round: 323,
            randomness: "10624fb156f7a8cc371c8777b19f5269a3ec139f21f39893dc45b91a6b050756"
                .to_owned(),
            signature: "8e8357b75918a2439ffa66ffa7e92b292e3b5c6828e458f280a626a6c06bc1187c7a1f3704c11e8369be047eb8049511"
                .to_owned(),
            previous_signature: None,
        };
        let (world_seed, derivation_digest) =
            derive_public_world_seed(&target_beacon).expect("target derivation");
        let resolution = ResolvedPublicSeed {
            schema_version: PUBLIC_SEED_SCHEMA_VERSION,
            commitment_digest: Digest::canonical(&commitment).expect("commitment digest"),
            target_beacon,
            derivation_domain: PUBLIC_SEED_DERIVATION_DOMAIN.to_owned(),
            derivation_digest,
            world_seed,
            world_id: derive_public_world_id(derivation_digest),
            verified_relays: DRAND_RELAYS.map(str::to_owned),
        };
        validate_resolved_public_seed(&commitment, &resolution).expect("bound resolution");

        let mut changed = resolution;
        changed.world_seed = WorldSeed::new(changed.world_seed.get().wrapping_add(1));
        assert!(validate_resolved_public_seed(&commitment, &changed).is_err());
    }
}
