//! État des systèmes de fichiers btrfs de l'hôte, lu via la commande
//! `btrfs` : aucune bibliothèque n'expose ces informations de façon fiable,
//! et le binaire officiel est la source de vérité des distributions.
//! Les commandes reçoivent une liste d'arguments fixe, sans shell, ce qui
//! exclut toute injection.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Délai maximal accordé à chaque commande invoquée depuis un handler HTTP.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Système de fichiers btrfs détecté sur l'hôte.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filesystem {
    pub uuid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub devices: Vec<String>,
}

/// Répartition allouée/utilisée par type de bloc d'un système de fichiers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Usage {
    #[serde(rename = "deviceSizeBytes")]
    pub device_size_bytes: u64,
    #[serde(rename = "deviceAllocatedBytes")]
    pub device_allocated: u64,
    #[serde(rename = "deviceUnallocatedBytes")]
    pub device_unallocated: u64,
    #[serde(rename = "dataUsedBytes")]
    pub data_used_bytes: u64,
    #[serde(rename = "dataTotalBytes")]
    pub data_total_bytes: u64,
    #[serde(rename = "metadataUsedBytes")]
    pub metadata_used_bytes: u64,
    #[serde(rename = "metadataTotalBytes")]
    pub metadata_total_bytes: u64,
    #[serde(rename = "freeEstimatedBytes")]
    pub free_estimated_bytes: u64,
}

/// Sous-volume btrfs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subvolume {
    pub id: i64,
    pub path: String,
    pub parent_id: i64,
}

/// État d'un scrub en cours ou terminé.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubStatus {
    pub running: bool,
    pub raw_output: String,
}

/// Échec d'une commande btrfs.
#[derive(Debug, thiserror::Error)]
pub enum BtrfsError {
    #[error("commande 'btrfs {args}' échouée: {detail}")]
    Command { args: String, detail: String },
    #[error("commande 'btrfs {args}' sans réponse après {}s", DEFAULT_TIMEOUT.as_secs())]
    Timeout { args: String },
}

enum RunError {
    Timeout,
    Failed(String),
}

/// Sortie combinée (stdout puis stderr) d'une commande. L'enfant est tué
/// si le délai expire.
async fn run_command(program: &str, args: &[&str]) -> Result<String, RunError> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(DEFAULT_TIMEOUT, child)
        .await
        .map_err(|_| RunError::Timeout)?
        .map_err(|e| RunError::Failed(e.to_string()))?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(combined)
    } else {
        Err(RunError::Failed(format!(
            "{} ({})",
            output.status,
            combined.trim()
        )))
    }
}

async fn btrfs(args: &[&str]) -> Result<String, BtrfsError> {
    run_command("btrfs", args).await.map_err(|e| {
        let args = args.join(" ");
        match e {
            RunError::Timeout => BtrfsError::Timeout { args },
            RunError::Failed(detail) => BtrfsError::Command { args, detail },
        }
    })
}

/// Systèmes de fichiers btrfs détectés via `btrfs filesystem show`,
/// enrichis de leur point de montage via `findmnt`.
pub async fn list_filesystems() -> Result<Vec<Filesystem>, BtrfsError> {
    let mut filesystems = parse_filesystem_show(&btrfs(&["filesystem", "show"]).await?);

    // findmnt sort en erreur quand aucun btrfs n'est monté : ce n'est pas
    // une erreur applicative, les points de montage restent simplement vides.
    let findmnt = run_command("findmnt", &["-J", "-t", "btrfs", "-o", "TARGET,UUID"])
        .await
        .unwrap_or_default();
    if let Some(mounts) = parse_findmnt(&findmnt) {
        for fs in &mut filesystems {
            fs.mount_point = mounts.get(&fs.uuid).cloned().unwrap_or_default();
        }
    }
    Ok(filesystems)
}

#[derive(Deserialize)]
struct FindmntOutput {
    #[serde(default)]
    filesystems: Vec<FindmntEntry>,
}

#[derive(Deserialize)]
struct FindmntEntry {
    #[serde(default)]
    target: String,
    #[serde(default)]
    uuid: String,
}

/// Association UUID -> point de montage depuis la sortie JSON de findmnt ;
/// `None` si la sortie est illisible.
fn parse_findmnt(out: &str) -> Option<HashMap<String, String>> {
    if out.trim().is_empty() {
        return Some(HashMap::new());
    }
    let parsed: FindmntOutput = serde_json::from_str(out).ok()?;
    Some(
        parsed
            .filesystems
            .into_iter()
            .map(|fs| (fs.uuid, fs.target))
            .collect(),
    )
}

static RE_UUID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"uuid:\s*(\S+)").unwrap());
static RE_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Label:\s*(?:'([^']*)'|none)").unwrap());
static RE_DEV_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"devid\s+\d+\s+size\s+(\S+)\s+used\s+(\S+)\s+path\s+(\S+)").unwrap()
});

fn parse_filesystem_show(out: &str) -> Vec<Filesystem> {
    let mut filesystems = Vec::new();
    let mut current: Option<Filesystem> = None;

    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Label:") {
            filesystems.extend(current.take());
            let mut fs = Filesystem::default();
            if let Some(m) = RE_LABEL.captures(trimmed) {
                fs.label = m.get(1).map_or("", |g| g.as_str()).to_string();
            }
            if let Some(m) = RE_UUID.captures(trimmed) {
                fs.uuid = m[1].to_string();
            }
            current = Some(fs);
            continue;
        }
        let (Some(fs), Some(m)) = (current.as_mut(), RE_DEV_ITEM.captures(trimmed)) else {
            continue;
        };
        fs.total_bytes += parse_human_bytes(&m[1]).unwrap_or(0);
        fs.used_bytes += parse_human_bytes(&m[2]).unwrap_or(0);
        fs.devices.push(m[3].to_string());
    }
    filesystems.extend(current);
    filesystems
}

/// Répartition d'allocation via `btrfs filesystem usage -b` (octets bruts).
pub async fn get_usage(mount_point: &str) -> Result<Usage, BtrfsError> {
    Ok(parse_usage(
        &btrfs(&["filesystem", "usage", "-b", mount_point]).await?,
    ))
}

static RE_OVERALL_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\S[^:]*):\s+(\d+)").unwrap());

// Dans les en-têtes « Data,RAID1: Size:X, Used:Y (Z%) », Size et Used
// figurent sur la même ligne que le nom de section, contrairement aux
// lignes « Clé: valeur » de la section Overall.
static RE_SECTION_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Data|Metadata|System),\S+:\s*Size:(\d+),\s*Used:(\d+)").unwrap()
});

fn parse_usage(out: &str) -> Usage {
    let mut u = Usage::default();
    let mut in_overall = false;

    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed == "Overall:" {
            in_overall = true;
            continue;
        }
        if let Some(m) = RE_SECTION_HEADER.captures(trimmed) {
            in_overall = false;
            let size = m[2].parse().unwrap_or(0);
            let used = m[3].parse().unwrap_or(0);
            match &m[1] {
                "Data" => (u.data_total_bytes, u.data_used_bytes) = (size, used),
                "Metadata" => (u.metadata_total_bytes, u.metadata_used_bytes) = (size, used),
                _ => {}
            }
            continue;
        }
        if !in_overall {
            continue;
        }
        let Some(m) = RE_OVERALL_LINE.captures(trimmed) else {
            continue;
        };
        let val = m[2].parse().unwrap_or(0);
        match m[1].trim() {
            "Device size" => u.device_size_bytes = val,
            "Device allocated" => u.device_allocated = val,
            "Device unallocated" => u.device_unallocated = val,
            "Free (estimated)" => u.free_estimated_bytes = val,
            _ => {}
        }
    }
    u
}

/// Sous-volumes via `btrfs subvolume list`.
pub async fn list_subvolumes(mount_point: &str) -> Result<Vec<Subvolume>, BtrfsError> {
    Ok(parse_subvolume_list(
        &btrfs(&["subvolume", "list", mount_point]).await?,
    ))
}

static RE_SUBVOL_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ID (\d+) gen \d+ top level (\d+) path (.+)$").unwrap());

fn parse_subvolume_list(out: &str) -> Vec<Subvolume> {
    out.lines()
        .filter_map(|line| RE_SUBVOL_LINE.captures(line.trim()))
        .map(|m| Subvolume {
            id: m[1].parse().unwrap_or(0),
            parent_id: m[2].parse().unwrap_or(0),
            path: m[3].to_string(),
        })
        .collect()
}

/// Démarre un scrub. Sans `-B`, `btrfs scrub start` rend la main
/// immédiatement et le scrub se poursuit en arrière-plan.
pub async fn start_scrub(mount_point: &str) -> Result<(), BtrfsError> {
    btrfs(&["scrub", "start", mount_point]).await.map(|_| ())
}

/// État du dernier scrub via `btrfs scrub status`.
pub async fn get_scrub_status(mount_point: &str) -> Result<ScrubStatus, BtrfsError> {
    let out = btrfs(&["scrub", "status", mount_point]).await?;
    Ok(ScrubStatus {
        running: out.contains("running"),
        raw_output: out.trim().to_string(),
    })
}

static RE_HUMAN_BYTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([\d.]+)\s*([KMGTP]?i?B)$").unwrap());

/// Convertit une taille lisible (ex : `1.82TiB`), telle qu'émise par
/// `btrfs filesystem show`, en octets.
fn parse_human_bytes(s: &str) -> Option<u64> {
    let m = RE_HUMAN_BYTES.captures(s.trim())?;
    let val: f64 = m[1].parse().ok()?;
    let mult: f64 = match m[2].to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KIB" => (1u64 << 10) as f64,
        "MIB" => (1u64 << 20) as f64,
        "GIB" => (1u64 << 30) as f64,
        "TIB" => (1u64 << 40) as f64,
        "PIB" => (1u64 << 50) as f64,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        _ => return None,
    };
    Some((val * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tib(x: f64) -> u64 {
        (x * (1u64 << 40) as f64) as u64
    }

    fn gib(x: f64) -> u64 {
        (x * (1u64 << 30) as f64) as u64
    }

    #[test]
    fn test_parse_filesystem_show_une_grappe_somme_les_devices() {
        let out = "Label: 'pool'  uuid: 1a2b3c4d-0000-1111-2222-333344445555
\tTotal devices 2 FS bytes used 1.82TiB
\tdevid    1 size 3.64TiB used 1.85TiB path /dev/sda1
\tdevid    2 size 3.64TiB used 1.85TiB path /dev/sdb1

";
        let got = parse_filesystem_show(out);
        assert_eq!(got.len(), 1);
        let fs = &got[0];
        assert_eq!(fs.uuid, "1a2b3c4d-0000-1111-2222-333344445555");
        assert_eq!(fs.label, "pool");
        assert_eq!(fs.devices, ["/dev/sda1", "/dev/sdb1"]);
        assert_eq!(fs.used_bytes, tib(1.85) * 2);
    }

    #[test]
    fn test_parse_filesystem_show_label_none_retourne_label_vide() {
        let out = "Label: none  uuid: 1a2b3c4d-0000-1111-2222-333344445555
\tTotal devices 1 FS bytes used 512.00GiB
\tdevid    1 size 1.00TiB used 512.00GiB path /dev/sda1
";
        let got = parse_filesystem_show(out);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "");
    }

    #[test]
    fn test_parse_filesystem_show_plusieurs_grappes_distinctes() {
        let out = "Label: 'a'  uuid: 11111111-1111-1111-1111-111111111111
\tdevid    1 size 1.00TiB used 500.00GiB path /dev/sda1

Label: 'b'  uuid: 22222222-2222-2222-2222-222222222222
\tdevid    1 size 2.00TiB used 1.00TiB path /dev/sdb1
";
        let got = parse_filesystem_show(out);
        assert_eq!(got.len(), 2);
        assert_ne!(got[0].uuid, got[1].uuid);
    }

    #[test]
    fn test_parse_usage_format_reel_extrait_toutes_les_valeurs() {
        let out = "Overall:
    Device size:\t\t4000000000000
    Device allocated:\t\t2000000000000
    Device unallocated:\t2000000000000
    Used:\t\t\t1800000000000
    Free (estimated):\t\t1800000000000\t(min: 900000000000)
    Data ratio:\t\t\t\t   2
    Metadata ratio:\t\t\t   2
    Global reserve:\t\t536870912\t(used: 0)

Data,RAID1: Size:900000000000, Used:850000000000 (94.44%)
   /dev/sda1     900000000000
   /dev/sdb1     900000000000

Metadata,RAID1: Size:10000000000, Used:5000000000 (50.00%)
   /dev/sda1      10000000000
   /dev/sdb1      10000000000

System,RAID1: Size:33554432, Used:114688 (0.34%)
   /dev/sda1        33554432
   /dev/sdb1        33554432
";
        let got = parse_usage(out);
        assert_eq!(got.device_size_bytes, 4_000_000_000_000);
        assert_eq!(got.device_allocated, 2_000_000_000_000);
        assert_eq!(got.device_unallocated, 2_000_000_000_000);
        assert_eq!(got.data_total_bytes, 900_000_000_000);
        assert_eq!(got.data_used_bytes, 850_000_000_000);
        assert_eq!(got.metadata_total_bytes, 10_000_000_000);
        assert_eq!(got.metadata_used_bytes, 5_000_000_000);
        assert_eq!(got.free_estimated_bytes, 1_800_000_000_000);
    }

    #[test]
    fn test_parse_subvolume_list_deux_lignes_deux_sous_volumes() {
        let out = "ID 256 gen 120 top level 5 path @home
ID 257 gen 121 top level 5 path @snapshots/2026-01-01
";
        let got = parse_subvolume_list(out);
        assert_eq!(
            got,
            [
                Subvolume {
                    id: 256,
                    path: "@home".into(),
                    parent_id: 5
                },
                Subvolume {
                    id: 257,
                    path: "@snapshots/2026-01-01".into(),
                    parent_id: 5
                },
            ]
        );
    }

    #[test]
    fn test_parse_human_bytes_unites_binaires_et_decimales() {
        let cases = [
            ("1.82TiB", tib(1.82)),
            ("512.00GiB", gib(512.0)),
            ("0.00B", 0),
            ("1TB", 1_000_000_000_000),
        ];
        for (input, want) in cases {
            assert_eq!(parse_human_bytes(input), Some(want), "{input}");
        }
    }

    #[test]
    fn test_parse_human_bytes_invalide_retourne_none() {
        assert_eq!(parse_human_bytes("pas-une-taille"), None);
    }

    #[test]
    fn test_parse_findmnt_sortie_json_resout_le_point_de_montage() {
        let got = parse_findmnt(
            r#"{"filesystems": [{"target": "/mnt/pool", "uuid": "1a2b3c4d-0000-1111-2222-333344445555"}]}"#,
        )
        .unwrap();
        assert_eq!(got["1a2b3c4d-0000-1111-2222-333344445555"], "/mnt/pool");
    }

    #[test]
    fn test_parse_findmnt_vide_ou_illisible() {
        assert!(parse_findmnt("").unwrap().is_empty());
        assert!(parse_findmnt("pas du json").is_none());
    }

    #[test]
    fn test_filesystem_serialisation_omet_label_et_montage_vides() {
        let fs = Filesystem {
            uuid: "u".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&fs).unwrap(),
            r#"{"uuid":"u","totalBytes":0,"usedBytes":0,"devices":[]}"#
        );
    }
}
