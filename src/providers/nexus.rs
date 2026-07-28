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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFile {
    file_id: u32,
    is_primary: bool,
    version: String,
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
            None => files
                .files
                .iter()
                .find(|f| f.is_primary)
                .or_else(|| files.files.first())
                .context("mod has no files")?
                .file_id,
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
                    files: files
                        .files
                        .iter()
                        .map(|f| CachedFile {
                            file_id: f.file_id,
                            is_primary: f.is_primary,
                            version: f.version.clone(),
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
                            files: files
                                .files
                                .into_iter()
                                .map(|f| CachedFile {
                                    file_id: f.file_id,
                                    is_primary: f.is_primary,
                                    version: f.version,
                                })
                                .collect(),
                        },
                    );

                Ok(())
            }
            .await;

            if let Err(e) = result {
                warn!("failed to update cache for nexus mod {mod_id}: {e:#}");
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
            None => {
                cached_mod
                    .files
                    .iter()
                    .find(|f| f.is_primary)
                    .or_else(|| cached_mod.files.first())?
                    .file_id
            }
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
        })
    }

    fn is_pinned(&self, spec: &ModSpecification, _cache: ProviderCache) -> bool {
        url::Url::parse(&spec.url)
            .map(|u| u.query_pairs().any(|(k, _)| k == "file_id"))
            .unwrap_or(false)
    }

    fn get_version_name(&self, spec: &ModSpecification, cache: ProviderCache) -> Option<String> {
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
        let file = match file_id {
            Some(id) => cached_mod.files.iter().find(|f| f.file_id == id)?,
            None => cached_mod
                .files
                .iter()
                .find(|f| f.is_primary)
                .or_else(|| cached_mod.files.first())?,
        };

        Some(file.version.clone())
    }
}
