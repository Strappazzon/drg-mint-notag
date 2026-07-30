use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tracing::warn;

use super::{
    BlobRef, ModInfo, ModProvider, ModProviderCache,
    ModResolution, ModResponse, ModSpecification, ProviderCache,
};

static RE_MOD: OnceLock<regex::Regex> = OnceLock::new();
fn re_mod() -> &'static regex::Regex {
    RE_MOD.get_or_init(|| regex::Regex::new("^https://(?:www\\.)?nexusmods\\.com/deeprockgalactic/mods/(?P<mod_id>\\d+)(?:\\?.*)?$").unwrap())
}

pub(crate) const NEXUS_DRG_ID: &str = "deeprockgalactic";
const NEXUS_PROVIDER_ID: &str = "nexusmods";
const NEXUS_API_BASE: &str = "https://api.nexusmods.com/v1";

fn category_name(category_id: u32) -> &'static str {
    match category_id {
        2 => "Miscellaneous",
        3 => "Utilities",
        4 => "Audio",
        5 => "User Interface",
        6 => "Gameplay",
        _ => "Unknown",
    }
}

fn pick_default_file<I: Iterator<Item = (u32, bool, i64)>>(files: I) -> Option<u32> {
    let mut best_main: Option<(u32, i64)> = None;
    let mut best_any: Option<(u32, i64)> = None;

    for (file_id, is_main, uploaded_timestamp) in files {
        if best_any.map_or(true, |(_, ts)| uploaded_timestamp > ts) {
            best_any = Some((file_id, uploaded_timestamp));
        }
        if is_main && best_main.map_or(true, |(_, ts)| uploaded_timestamp > ts) {
            best_main = Some((file_id, uploaded_timestamp));
        }
    }

    best_main.or(best_any).map(|(file_id, _)| file_id)
}

inventory::submit! {
    super::ProviderFactory {
        id: NEXUS_PROVIDER_ID,
        new: NexusProvider::new_provider,
        can_provide: |url| re_mod().is_match(url),
        parameters: &[
            super::ProviderParameter {
                id: "api_key",
                name: "API Key",
                description: "Nexus Mods API key",
                link: Some("https://www.nexusmods.com/settings/api-keys"),
            },
        ]
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NexusModProviderCache {
    mods: HashMap<u32, CachedMod>,
    file_blobs: HashMap<u32, BlobRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedMod {
    name: String,
    files: Vec<CachedFile>,
    category_id: u32,
    contains_adult_content: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFile {
    file_id: u32,
    is_primary: bool,
    version: String,
    category_name: String,
    uploaded_timestamp: i64,
}

#[typetag::serde]
impl ModProviderCache for NexusModProviderCache {
    fn new() -> Self {
        Self::default()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Deserialize)]
struct ModJson {
    name: String,
    category_id: u32,
    contains_adult_content: bool,
}

#[derive(Deserialize)]
struct FilesJson {
    files: Vec<FileEntry>,
}

#[derive(Deserialize)]
struct FileEntry {
    file_id: u32,
    is_primary: bool,
    version: String,
    category_name: Option<String>,
    uploaded_timestamp: i64,
}

pub struct NexusProvider {
    client: reqwest::Client,
    api_key: String,
}

impl NexusProvider {
    fn new_provider(parameters: &HashMap<String, String>) -> Result<Arc<dyn ModProvider>> {
        Ok(Arc::new(Self {
            client: reqwest::Client::new(),
            api_key: parameters
                .get("api_key")
                .context("missing parameter api_key")?
                .to_owned(),
        }))
    }
    async fn fetch_mod_and_files(&self, mod_id: u32) -> Result<(ModJson, FilesJson)> {
        let mod_info: ModJson = self
            .client
            .get(format!("{NEXUS_API_BASE}/games/{NEXUS_DRG_ID}/mods/{mod_id}.json"))
            .header("apikey", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let files: FilesJson = self
            .client
            .get(format!("{NEXUS_API_BASE}/games/{NEXUS_DRG_ID}/mods/{mod_id}/files.json"))
            .header("apikey", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok((mod_info, files))
    }
}

#[derive(Debug, Deserialize)]
struct ValidateResponse {
    #[serde(rename = "name")]
    user_name: String,
    #[serde(rename = "is_premium?")]
    is_premium: bool,
}

#[async_trait::async_trait]
impl ModProvider for NexusProvider {
    async fn check(&self) -> Result<()> {
        let res = self
            .client
            .get(format!("{NEXUS_API_BASE}/users/validate.json"))
            .header("apikey", &self.api_key)
            .send()
            .await?
            .error_for_status()?;
        let validated: ValidateResponse = res.json().await?;

        if !validated.is_premium {
            warn!(
                "Nexus Mods account \"{}\" is not a Premium member.",
                validated.user_name
            );
        }

        Ok(())
    }

    async fn resolve_mod(
        &self,
        spec: &ModSpecification,
        _update: bool,
        cache: ProviderCache,
    ) -> Result<ModResponse> {
        let parsed = url::Url::parse(&spec.url).context("invalid mod URL")?;
        let mod_id: u32 = re_mod()
            .captures(&spec.url)
            .and_then(|c| c.name("mod_id"))
            .context("URL did not match Nexus Mods mod pattern")?
            .as_str()
            .parse()
            .context("mod_id was not a valid number")?;
        let file_id: Option<u32> = parsed
            .query_pairs()
            .find(|(k, _)| k == "file_id")
            .and_then(|(_, v)| v.parse().ok());
        let (mod_info, files) = self.fetch_mod_and_files(mod_id).await?;
        let unpinned_url = format!("https://www.nexusmods.com/{NEXUS_DRG_ID}/mods/{mod_id}");
        let versions = files
            .files
            .iter()
            .map(|f| ModSpecification::new(format!("{unpinned_url}?tab=files&file_id={}", f.file_id)))
            .collect();
        let resolution_file_id = match file_id {
            Some(id) => id,
            None => pick_default_file(
                files
                    .files
                    .iter()
                    .map(|f| (f.file_id, f.category_name.as_deref() == Some("MAIN"), f.uploaded_timestamp)),
            )
            .context("mod has no files")?,
        };

        cache
            .write()
            .unwrap()
            .get_mut::<NexusModProviderCache>(NEXUS_PROVIDER_ID)
            .mods
            .insert(
                mod_id,
                CachedMod {
                    name: mod_info.name.clone(),
                    category_id: mod_info.category_id,
                    contains_adult_content: mod_info.contains_adult_content,
                    files: files
                        .files
                        .iter()
                        .map(|f| CachedFile {
                            file_id: f.file_id,
                            is_primary: f.is_primary,
                            version: f.version.clone(),
                            category_name: f.category_name.clone().unwrap_or_else(|| "UNKNOWN".to_string()),
                            uploaded_timestamp: f.uploaded_timestamp,
                        })
                        .collect(),
                },
            );

        Ok(ModResponse::Resolve(ModInfo {
            provider: NEXUS_PROVIDER_ID,
            name: mod_info.name,
            spec: ModSpecification::new(unpinned_url.clone()),
            versions,
            resolution: ModResolution::resolvable(format!(
                "{unpinned_url}?tab=files&file_id={resolution_file_id}"
            )),
            suggested_require: false,
            suggested_dependencies: Vec::new(),
            modio_tags: None,
            modio_id: None,
            nexus_tags: Some(super::NexusTags {
                category: category_name(mod_info.category_id).to_string(),
                contains_adult_content: mod_info.contains_adult_content,
            }),
        }))
    }

    async fn fetch_mod(
        &self,
        res: &ModResolution,
        update: bool,
        cache: ProviderCache,
        blob_cache: &super::BlobCache,
        tx: Option<Sender<super::FetchProgress>>,
    ) -> Result<PathBuf> {
        let parsed = url::Url::parse(&res.url).context("invalid mod resolution URL")?;
        let mod_id: u32 = re_mod()
            .captures(&res.url)
            .and_then(|c| c.name("mod_id"))
            .context("resolution URL did not match Nexus mod pattern")?
            .as_str()
            .parse()
            .context("mod_id was not a valid number")?;
        let file_id: u32 = parsed
            .query_pairs()
            .find(|(k, _)| k == "file_id")
            .and_then(|(_, v)| v.parse().ok())
            .context("resolution URL had no file_id")?;

        if !update {
            let cached_path = cache
                .read()
                .unwrap()
                .get::<NexusModProviderCache>(NEXUS_PROVIDER_ID)
                .and_then(|c| c.file_blobs.get(&file_id))
                .and_then(|blob| blob_cache.get_path(blob));

            if let Some(path) = cached_path {
                if let Some(tx) = &tx {
                    tx.send(super::FetchProgress::Complete {
                        resolution: res.clone(),
                    })
                    .await
                    .ok();
                }
                return Ok(path);
            }
        }

        #[derive(Deserialize)]
        struct DownloadLink {
            #[serde(rename = "URI")]
            uri: String,
        }

        let links: Vec<DownloadLink> = self
            .client
            .get(format!("{NEXUS_API_BASE}/games/{NEXUS_DRG_ID}/mods/{mod_id}/files/{file_id}/download_link.json"))
            .header("apikey", &self.api_key)
            .send()
            .await?
            .error_for_status()
            .context("Failed to download mod (are you a Premium member?)")?
            .json()
            .await?;
        let link = links.first().context("no download servers returned")?;
        let response = self
            .client
            .get(&link.uri)
            .send()
            .await?
            .error_for_status()?;
        let size = response.content_length().unwrap_or(0);
        let mut stream = response.bytes_stream();
        let mut buf = Vec::new();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;

            downloaded += chunk.len() as u64;
            buf.extend_from_slice(&chunk);

            if let Some(tx) = &tx {
                tx.send(super::FetchProgress::Progress {
                    resolution: res.clone(),
                    progress: downloaded,
                    size,
                })
                .await
                .ok();
            }
        }

        if let Some(tx) = &tx {
            tx.send(super::FetchProgress::Complete {
                resolution: res.clone(),
            })
            .await
            .ok();
        }

        let blob_ref = blob_cache.write(&buf)?;
        let path = blob_cache
            .get_path(&blob_ref)
            .context("blob was written but path lookup failed")?;

        cache
            .write()
            .unwrap()
            .get_mut::<NexusModProviderCache>(NEXUS_PROVIDER_ID)
            .file_blobs
            .insert(file_id, blob_ref);

        Ok(path)
    }

    async fn update_cache(&self, cache: ProviderCache) -> Result<()> {
        let mod_ids: Vec<u32> = cache
            .read()
            .unwrap()
            .get::<NexusModProviderCache>(NEXUS_PROVIDER_ID)
            .map(|c| c.mods.keys().copied().collect())
            .unwrap_or_default();

        for mod_id in mod_ids {
            let result: Result<()> = async {
                let (mod_info, files) = self.fetch_mod_and_files(mod_id).await?;

                cache
                    .write()
                    .unwrap()
                    .get_mut::<NexusModProviderCache>(NEXUS_PROVIDER_ID)
                    .mods
                    .insert(
                        mod_id,
                        CachedMod {
                            name: mod_info.name,
                            category_id: mod_info.category_id,
                            contains_adult_content: mod_info.contains_adult_content,
                            files: files
                                .files
                                .into_iter()
                                .map(|f| CachedFile {
                                    file_id: f.file_id,
                                    is_primary: f.is_primary,
                                    version: f.version,
                                    category_name: f.category_name.unwrap_or_else(|| "UNKNOWN".to_string()),
                                    uploaded_timestamp: f.uploaded_timestamp,
                                })
                                .collect(),
                        },
                    );

                Ok(())
            }
            .await;

            if let Err(e) = result {
                warn!("failed to update cache for Nexus Mods mod {mod_id}: {e:#}");
            }
        }

        Ok(())
    }

    fn get_mod_info(&self, spec: &ModSpecification, cache: ProviderCache) -> Option<ModInfo> {
        let parsed = url::Url::parse(&spec.url).ok()?;
        let mod_id: u32 = re_mod()
            .captures(&spec.url)?
            .name("mod_id")?
            .as_str()
            .parse()
            .ok()?;
        let file_id: Option<u32> = parsed
            .query_pairs()
            .find(|(k, _)| k == "file_id")
            .and_then(|(_, v)| v.parse().ok());
        let guard = cache.read().unwrap();
        let nexus_cache = guard.get::<NexusModProviderCache>(NEXUS_PROVIDER_ID)?;
        let cached_mod = nexus_cache.mods.get(&mod_id)?;
        let unpinned_url = format!("https://www.nexusmods.com/{NEXUS_DRG_ID}/mods/{mod_id}");
        let versions = cached_mod
            .files
            .iter()
            .map(|f| ModSpecification::new(format!("{unpinned_url}?tab=files&file_id={}", f.file_id)))
            .collect();
        let resolution_file_id = match file_id {
            Some(id) => id,
            None => pick_default_file(
                cached_mod
                    .files
                    .iter()
                    .map(|f| (f.file_id, f.category_name == "MAIN", f.uploaded_timestamp)),
            )?,
        };

        Some(ModInfo {
            provider: NEXUS_PROVIDER_ID,
            name: cached_mod.name.clone(),
            spec: ModSpecification::new(unpinned_url.clone()),
            versions,
            resolution: ModResolution::resolvable(
                format!("{unpinned_url}?tab=files&file_id={resolution_file_id}")
            ),
            suggested_require: false,
            suggested_dependencies: Vec::new(),
            modio_tags: None,
            modio_id: None,
            nexus_tags: Some(super::NexusTags {
                category: category_name(cached_mod.category_id).to_string(),
                contains_adult_content: cached_mod.contains_adult_content,
            }),
        })
    }

    fn is_pinned(&self, spec: &ModSpecification, _cache: ProviderCache) -> bool {
        url::Url::parse(&spec.url)
            .map(|u| u.query_pairs().any(|(k, _)| k == "file_id"))
            .unwrap_or(false)
    }

    fn get_version_name(&self, spec: &ModSpecification, cache: ProviderCache) -> Option<String> {
        let parsed = url::Url::parse(&spec.url).ok()?;
        let file_id: Option<u32> = parsed
            .query_pairs()
            .find(|(k, _)| k == "file_id")
            .and_then(|(_, v)| v.parse().ok());
        let Some(id) = file_id else {
            return Some("latest".to_string());
        };
        let mod_id: u32 = re_mod()
            .captures(&spec.url)?
            .name("mod_id")?
            .as_str()
            .parse()
            .ok()?;
        let guard = cache.read().unwrap();
        let nexus_cache = guard.get::<NexusModProviderCache>(NEXUS_PROVIDER_ID)?;
        let cached_mod = nexus_cache.mods.get(&mod_id)?;
        let file = cached_mod.files.iter().find(|f| f.file_id == id)?;

        Some(format!("{} - {}", file.category_name, file.version))
    }
}
