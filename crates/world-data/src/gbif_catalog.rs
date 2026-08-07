//! Streaming reader for the retained accepted-GBIF-Animalia catalog.
//!
//! Taxonomy is not occurrence or ecology. This reader supplies only a real,
//! source-addressable species identity to later placement and physiology adapters.

use std::{
    collections::{BTreeMap, BTreeSet},
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

    /// Resolve an exact, bounded set of canonical scientific names in one streaming
    /// catalog pass. A name can deliberately return more than one accepted record;
    /// callers must treat that as unresolved rather than choosing a taxon.
    pub fn find_canonical_names(
        mut self,
        expected_names: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, Vec<GbifAnimaliaSpecies>>, GbifAnimaliaCatalogError> {
        let mut matches = expected_names
            .iter()
            .cloned()
            .map(|name| (name, Vec::new()))
            .collect::<BTreeMap<_, _>>();
        while let Some(species) = self.next_species()? {
            if let Some(found) = matches.get_mut(&species.canonical_name) {
                found.push(species);
            }
        }
        Ok(matches)
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

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use super::*;

    fn write_string(writer: &mut impl Write, value: &str) {
        writer
            .write_all(&(value.len() as u32).to_le_bytes())
            .expect("length");
        writer.write_all(value.as_bytes()).expect("value");
    }

    fn fixture_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "atiny-gbif-catalog-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    fn write_fixture(path: &Path) {
        let mut file = File::create(path).expect("fixture file");
        file.write_all(GBIF_ANIMALIA_CATALOG_MAGIC).expect("magic");
        file.write_all(&1_u16.to_le_bytes()).expect("schema");
        file.write_all(Digest::sha256(b"snapshot").as_bytes())
            .expect("digest");
        file.write_all(&2_u64.to_le_bytes()).expect("count");
        for (key, name) in [(2441176_u64, "Bison bison"), (5219173, "Canis lupus")] {
            file.write_all(&key.to_le_bytes()).expect("key");
            for value in [
                name,
                name,
                "Chordata",
                "Mammalia",
                "Carnivora",
                "Canidae",
                "Canis",
            ] {
                write_string(&mut file, value);
            }
        }
    }

    #[test]
    fn streams_and_finds_real_source_species_without_materializing_the_catalog() {
        let path = fixture_path();
        write_fixture(&path);
        let reader = GbifAnimaliaCatalogReader::open(&path).expect("open fixture");
        assert_eq!(reader.header().record_count, 2);
        let species = reader
            .find_taxon(5219173)
            .expect("stream fixture")
            .expect("wolf is present");
        assert_eq!(species.scientific_name, "Canis lupus");
        assert_eq!(
            species
                .species_identity()
                .expect("citable identity")
                .identifier,
            "5219173"
        );
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn exact_name_lookup_keeps_missing_and_ambiguous_records_explicit() {
        let path = fixture_path();
        write_fixture(&path);
        let names = BTreeSet::from(["Canis lupus".to_owned(), "Missing species".to_owned()]);
        let found = GbifAnimaliaCatalogReader::open(&path)
            .expect("open fixture")
            .find_canonical_names(&names)
            .expect("stream fixture");
        assert_eq!(found["Canis lupus"].len(), 1);
        assert!(found["Missing species"].is_empty());
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn rejects_malformed_catalog_header() {
        let path = fixture_path();
        fs::write(&path, b"not-a-catalog").expect("write malformed fixture");
        assert!(matches!(
            GbifAnimaliaCatalogReader::open(&path),
            Err(GbifAnimaliaCatalogError::Io(_)) | Err(GbifAnimaliaCatalogError::InvalidMagic)
        ));
        fs::remove_file(path).expect("remove fixture");
    }
}
