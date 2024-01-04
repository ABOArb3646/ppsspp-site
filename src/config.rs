use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    pub name: String,
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<File>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DownloadInfo {
    name: String,
    icon: Option<String>,
    url: Option<String>,
    download_url: Option<String>,
    service_img: Option<String>,
    service_alt: Option<String>,
    short_name: Option<String>,
    whats_this: Option<String>,
    whats_this_url: Option<String>,
    filename: Option<String>,
    #[serde(default)]
    gold: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PlatformInfo {
    title: String,
    platform_badge: Option<String>,
    platform_key: String, // we can ignore this, this was for react
    downloads: Vec<DownloadInfo>,
}

#[derive(Debug)]
struct BinaryVersion {
    version: String,
    files: Vec<String>,
}

// Boiled-down version of the Previous Releases table for easy template consumption.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VersionDownloads {
    version: String,
    downloads: Vec<DownloadInfo>,
}

// This contains a bunch of stuff that various pages want to reference.
// Little point in restricting certain data to certain pages since we're a static generator
// so it'll be a grab bag of stuff.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GlobalMeta {
    pub app_version: String,
    pub platforms: Vec<PlatformInfo>,
    pub version_downloads: Vec<VersionDownloads>,
    pub prod: bool,
}

fn download_path(url_base: &str, version: &str, filename: &str) -> String {
    format!(
        "{}/files/{}/{}",
        url_base,
        version.replace('.', "_"),
        filename
    )
}

fn gold_download_path(url_base: &str, version: &str, filename: &str) -> String {
    format!(
        "{}/api/goldfiles/{}/{}",
        url_base,
        version.replace('.', "_"),
        filename
    )
}

impl GlobalMeta {
    pub fn new(production: bool, url_base: &str) -> anyhow::Result<Self> {
        // Parse the download path dump.

        let downloads_json = std::fs::read_to_string("src/downloads.json")?;
        let downloads_gold_json = std::fs::read_to_string("src/downloads_gold.json")?;
        let platforms_json = std::fs::read_to_string("src/platform.json")?;

        let downloads: File = serde_json::from_str(&downloads_json).unwrap();
        let downloads_gold: File = serde_json::from_str(&downloads_gold_json).unwrap();

        let mut platforms: Vec<PlatformInfo> = serde_json::from_str(&platforms_json).unwrap();

        let version_binaries = parse_files(downloads, downloads_gold);

        let file_versions = pivot(&version_binaries);

        // Update the platforms with URLs
        for platform in &mut platforms {
            for download in &mut platform.downloads {
                if let Some(filename) = &download.filename {
                    if let Some(version) = file_versions.get(filename) {
                        if let Some(first) = version.first() {
                            download.download_url = if download.gold {
                                Some(gold_download_path(url_base, first, filename))
                            } else {
                                Some(download_path(url_base, first, filename))
                            };
                        }
                    }
                }
            }
        }

        let version_downloads = boil(&version_binaries, &platforms);

        //println!("{:#?}", version_binaries);
        //println!("{:#?}", file_versions);
        //println!("{:#?}", platforms);
        println!("{:#?}", version_downloads);

        // println!("{:#?}", downloads);
        // println!("{:#?}", platforms);

        // OK, here we need to preprocess the downloads together with the platform data.

        Ok(Self {
            app_version: if let Some(file) = version_binaries.first() {
                file.version.clone()
            } else {
                "indeterminate".to_string()
            },
            prod: production,
            platforms,
            version_downloads,
        })
    }
}

fn to_binaries_per_version(files: File) -> Vec<BinaryVersion> {
    files
        .children
        .iter()
        .filter(|child| !child.children.is_empty() && child.name.find('_').is_some())
        .map(|child| BinaryVersion {
            version: child.name.replace('_', "."),
            files: child
                .children
                .iter()
                .map(|subchild| subchild.name.clone())
                .collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>()
}

fn parse_files(files: File, gold_files: File) -> Vec<BinaryVersion> {
    let mut binaries_per_version = to_binaries_per_version(files);

    // reverse order, newest first
    binaries_per_version.sort_by(|a, b| natord::compare(&b.version, &a.version));

    let mut binaries_per_version_gold = to_binaries_per_version(gold_files);

    for b in &mut binaries_per_version {
        for g in &mut binaries_per_version_gold {
            if g.version == b.version {
                b.files.append(&mut g.files);
            }
        }
    }

    binaries_per_version
}

fn pivot(binaries_per_version: &Vec<BinaryVersion>) -> HashMap<String, Vec<String>> {
    let mut hash = HashMap::<String, Vec<String>>::new();
    for entry in binaries_per_version {
        for name in &entry.files {
            hash.entry(name.clone())
                .or_default()
                .push(entry.version.clone());
        }
    }
    hash
}

fn boil(version_binaries: &[BinaryVersion], platforms: &[PlatformInfo]) -> Vec<VersionDownloads> {
    let mut downloads = vec![];
    for version in version_binaries {
        let mut version_download = VersionDownloads {
            version: version.version.clone(),
            downloads: vec![],
        };

        for platform in platforms {
            let mut first = true;
            for filename in &version.files {
                //println!("filename: {}", filename);
                //println!("{:#?}", platform.downloads);
                // Look up the filename in platforms, the ugly way.
                if let Some(download) = platform
                    .downloads
                    .iter()
                    .find(|download| download.filename.as_ref() == Some(filename))
                {
                    // Add this download!
                    let mut download = download.clone();
                    if first {
                        download.icon = platform.platform_badge.clone();
                        first = false;
                    } else {
                        download.icon = None;
                    }
                    version_download.downloads.push(download);
                }
            }
        }
        if !version_download.downloads.is_empty() {
            downloads.push(version_download);
        }
    }
    downloads
}

pub struct Config {
    pub url_base: String,
    pub indir: PathBuf,
    pub outdir: PathBuf,
    pub markdown_options: markdown::Options,
    pub global_meta: GlobalMeta,
}
