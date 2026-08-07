//! Streaming reader for the retained accepted-GBIF-Animalia catalog.
//!
//! Taxonomy is not occurrence or ecology. This reader supplies only a real,
//! source-addressable species identity to later placement and physiology adapters.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use thiserror::Error;
use world_domain::{Digest, SpeciesIdentity};

pub const GBIF_ANIMALIA_CATALOG_MAGIC: &[u8; 8] = b"ATCGBF01";
pub const GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GbifAnimaliaCatalogHeader {
    pub source_snapshot_digest: Digest,
    pub record_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GbifAnimaliaSpecies {
    pub taxon_key: u64,
    pub scientific_name: String,
    pub canonical_name: String,
    pub phylum: String,
    pub class: String,
    pub order: String,
    pub family: String,
    pub genus: String,
}

impl GbifAnimaliaSpecies {
    pub fn species_identity(&self) -> Result<SpeciesIdentity, GbifAnimaliaCatalogError> {
        SpeciesIdentity::new(
            "gbif",
            self.taxon_key.to_string(),
            self.scientific_name.clone(),
            format!("https://www.gbif.org/species/{}", self.taxon_key),
        )
        .map_err(|error| GbifAnimaliaCatalogError::InvalidSpecies(error.to_string()))
    }
}

/// Opens and streams the compact retained catalog without materializing its 1.8M
/// entries. Callers still bind the artifact's content hash through the world
/// composition before treating any lookup as a world input.
pub struct GbifAnimaliaCatalogReader {
    reader: BufReader<File>,
    header: GbifAnimaliaCatalogHeader,
    remaining: u64,
}

impl GbifAnimaliaCatalogReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GbifAnimaliaCatalogError> {
        let file =
            File::open(path).map_err(|error| GbifAnimaliaCatalogError::Io(error.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut magic = [0_u8; 8];
        reader.read_exact(&mut magic).map_err(io_error)?;
        if &magic != GBIF_ANIMALIA_CATALOG_MAGIC {
            return Err(GbifAnimaliaCatalogError::InvalidMagic);
        }
        let mut schema = [0_u8; 2];
        reader.read_exact(&mut schema).map_err(io_error)?;
        if u16::from_le_bytes(schema) != GBIF_ANIMALIA_CATALOG_SCHEMA_VERSION {
            return Err(GbifAnimaliaCatalogError::UnsupportedSchema);
        }
        let mut digest = [0_u8; 32];
        reader.read_exact(&mut digest).map_err(io_error)?;
        let mut count = [0_u8; 8];
        reader.read_exact(&mut count).map_err(io_error)?;
        let record_count = u64::from_le_bytes(count);
        if record_count == 0 {
            return Err(GbifAnimaliaCatalogError::EmptyCatalog);
        }
        Ok(Self {
            reader,
            header: GbifAnimaliaCatalogHeader {
                source_snapshot_digest: Digest::from_bytes(digest),
                record_count,
            },
            remaining: record_count,
        })
    }

    #[must_use]
    pub const fn header(&self) -> &GbifAnimaliaCatalogHeader {
        &self.header
    }

    pub fn next_species(
        &mut self,
    ) -> Result<Option<GbifAnimaliaSpecies>, GbifAnimaliaCatalogError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        let mut key = [0_u8; 8];
        self.reader.read_exact(&mut key).map_err(io_error)?;
        let taxon_key = u64::from_le_bytes(key);
        if taxon_key == 0 {
            return Err(GbifAnimaliaCatalogError::ZeroTaxonKey);
        }
        let scientific_name = read_string(&mut self.reader)?;
        if scientific_name.is_empty() {
            return Err(GbifAnimaliaCatalogError::EmptyScientificName(taxon_key));
        }
        Ok(Some(GbifAnimaliaSpecies {
            taxon_key,
            scientific_name,
            canonical_name: read_string(&mut self.reader)?,
            phylum: read_string(&mut self.reader)?,
            class: read_string(&mut self.reader)?,
            order: read_string(&mut self.reader)?,
            family: read_string(&mut self.reader)?,
            genus: read_string(&mut self.reader)?,
        }))
    }

    pub fn find_taxon(
        mut self,
        expected_key: u64,
    ) -> Result<Option<GbifAnimaliaSpecies>, GbifAnimaliaCatalogError> {
        while let Some(species) = self.next_species()? {
            if species.taxon_key == expected_key {
                return Ok(Some(species));
            }
        }
        Ok(None)
    }
}

fn read_string(reader: &mut impl Read) -> Result<String, GbifAnimaliaCatalogError> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes).map_err(io_error)?;
    let length = usize::try_from(u32::from_le_bytes(bytes))
        .map_err(|_| GbifAnimaliaCatalogError::StringTooLong)?;
    if length > 1024 * 1024 {
        return Err(GbifAnimaliaCatalogError::StringTooLong);
    }
    let mut value = vec![0; length];
    reader.read_exact(&mut value).map_err(io_error)?;
    String::from_utf8(value).map_err(|_| GbifAnimaliaCatalogError::InvalidUtf8)
}

fn io_error(error: std::io::Error) -> GbifAnimaliaCatalogError {
    GbifAnimaliaCatalogError::Io(error.to_string())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GbifAnimaliaCatalogError {
    #[error("catalog I/O error: {0}")]
    Io(String),
    #[error("GBIF Animalia catalog magic is invalid")]
    InvalidMagic,
    #[error("GBIF Animalia catalog schema is unsupported")]
    UnsupportedSchema,
    #[error("GBIF Animalia catalog is empty")]
    EmptyCatalog,
    #[error("GBIF Animalia catalog contains a zero taxon key")]
    ZeroTaxonKey,
    #[error("GBIF Animalia catalog species {0} has an empty scientific name")]
    EmptyScientificName(u64),
    #[error("GBIF Animalia catalog string is too long")]
    StringTooLong,
    #[error("GBIF Animalia catalog contains invalid UTF-8")]
    InvalidUtf8,
    #[error("invalid GBIF species identity: {0}")]
    InvalidSpecies(String),
}
