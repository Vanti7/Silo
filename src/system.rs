//! Métriques matérielles et système de l'hôte (CPU, mémoire, réseau,
//! température, uptime) pour le tableau de bord, lues dans `/proc` et
//! `/sys` : silo cible exclusivement un NAS Linux.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

/// Instantané des métriques système.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub cpu_percent: f64,
    pub load_avg1: f64,
    pub load_avg5: f64,
    pub load_avg15: f64,
    pub mem_total_bytes: u64,
    pub mem_used_bytes: u64,
    pub mem_percent: f64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub uptime_seconds: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub temperatures: Vec<Temperature>,
    pub network: Vec<NetInterface>,
}

/// Sonde thermique (CPU, disque…).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

/// Compteurs cumulés d'octets envoyés/reçus par interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInterface {
    pub name: String,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
}

/// Informations statiques sur l'hôte.
#[derive(Debug, Default, Serialize)]
pub struct Info {
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub kernel: String,
    pub arch: String,
}

/// Erreur de lecture d'une métrique indispensable.
#[derive(Debug, thiserror::Error)]
#[error("system: lecture {what}: {source}")]
pub struct SystemError {
    what: &'static str,
    #[source]
    source: std::io::Error,
}

fn read(what: &'static str, path: &str) -> Result<String, SystemError> {
    fs::read_to_string(path).map_err(|source| SystemError { what, source })
}

// Fenêtre de mesure de l'utilisation CPU, entre deux lectures de /proc/stat.
const CPU_SAMPLE: Duration = Duration::from_millis(200);

/// Prend un instantané des métriques courantes. Seuls le CPU et la mémoire
/// sont indispensables ; les autres métriques restent vides si illisibles.
pub async fn collect() -> Result<Stats, SystemError> {
    let before = parse_cpu_times(&read("CPU", "/proc/stat")?);
    tokio::time::sleep(CPU_SAMPLE).await;
    let after = parse_cpu_times(&read("CPU", "/proc/stat")?);

    let mut s = Stats {
        cpu_percent: match (before, after) {
            (Some(b), Some(a)) => cpu_percent(b, a),
            _ => 0.0,
        },
        ..Default::default()
    };

    if let Some((l1, l5, l15)) = fs::read_to_string("/proc/loadavg")
        .ok()
        .as_deref()
        .and_then(parse_loadavg)
    {
        (s.load_avg1, s.load_avg5, s.load_avg15) = (l1, l5, l15);
    }

    let mem = parse_meminfo(&read("mémoire", "/proc/meminfo")?);
    s.mem_total_bytes = mem.total;
    s.mem_used_bytes = mem.used;
    s.mem_percent = mem.used_percent;
    s.swap_total_bytes = mem.swap_total;
    s.swap_used_bytes = mem.swap_used;

    if let Some(up) = fs::read_to_string("/proc/uptime")
        .ok()
        .as_deref()
        .and_then(parse_uptime)
    {
        s.uptime_seconds = up;
    }

    s.temperatures = read_temperatures();

    if let Ok(dev) = fs::read_to_string("/proc/net/dev") {
        s.network = parse_net_dev(&dev);
    }

    Ok(s)
}

/// Informations statiques de l'hôte.
pub fn collect_info() -> Result<Info, SystemError> {
    let hostname = read("hostname", "/proc/sys/kernel/hostname")?
        .trim()
        .to_string();
    Ok(Info {
        hostname,
        os: std::env::consts::OS.to_string(),
        platform: fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|c| parse_os_release_id(&c))
            .unwrap_or_default(),
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|k| k.trim().to_string())
            .unwrap_or_default(),
        // Architecture du noyau (`uname -m`), qui peut différer de celle du
        // binaire (userland 32 bits sur noyau 64 bits). Ce fichier n'existe
        // que depuis Linux 6.1 : à défaut, celle du binaire.
        arch: fs::read_to_string("/proc/sys/kernel/arch")
            .map(|a| a.trim().to_string())
            .unwrap_or_else(|_| std::env::consts::ARCH.to_string()),
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CpuTimes {
    total: f64,
    busy: f64,
}

/// Temps CPU cumulés de la ligne agrégée `cpu` de /proc/stat. Les temps
/// « guest » sont déjà comptés dans user/nice et ne sont pas ajoutés.
fn parse_cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<f64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|x| x.parse().unwrap_or(0.0))
        .collect();
    if v.len() < 4 {
        return None;
    }
    let total: f64 = v.iter().sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0.0);
    Some(CpuTimes {
        total,
        busy: total - idle,
    })
}

fn cpu_percent(before: CpuTimes, after: CpuTimes) -> f64 {
    if after.busy <= before.busy {
        return 0.0;
    }
    if after.total <= before.total {
        return 100.0;
    }
    ((after.busy - before.busy) / (after.total - before.total) * 100.0).clamp(0.0, 100.0)
}

fn parse_loadavg(s: &str) -> Option<(f64, f64, f64)> {
    let mut it = s.split_whitespace().map(|x| x.parse::<f64>());
    Some((it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?))
}

fn parse_uptime(s: &str) -> Option<u64> {
    let secs: f64 = s.split_whitespace().next()?.parse().ok()?;
    Some(secs as u64)
}

#[derive(Debug, Default, PartialEq)]
struct MemInfo {
    total: u64,
    used: u64,
    used_percent: f64,
    swap_total: u64,
    swap_used: u64,
}

/// Mémoire « utilisée » au sens de gopsutil, que reprenait le tableau de
/// bord : total - libre - tampons - cache (dont la part récupérable du slab).
fn parse_meminfo(s: &str) -> MemInfo {
    let mut total = 0;
    let mut free = 0;
    let mut buffers = 0;
    let mut cached = 0;
    let mut sreclaimable = 0;
    let mut swap_total = 0;
    let mut swap_free = 0;
    for line in s.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let kib: u64 = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let bytes = kib * 1024;
        match key {
            "MemTotal" => total = bytes,
            "MemFree" => free = bytes,
            "Buffers" => buffers = bytes,
            "Cached" => cached = bytes,
            "SReclaimable" => sreclaimable = bytes,
            "SwapTotal" => swap_total = bytes,
            "SwapFree" => swap_free = bytes,
            _ => {}
        }
    }
    let used = total.saturating_sub(free + buffers + cached + sreclaimable);
    MemInfo {
        total,
        used,
        used_percent: if total == 0 {
            0.0
        } else {
            used as f64 / total as f64 * 100.0
        },
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

/// Compteurs par interface depuis /proc/net/dev (2 lignes d'en-tête).
fn parse_net_dev(s: &str) -> Vec<NetInterface> {
    s.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, counters) = line.split_once(':')?;
            let fields: Vec<u64> = counters
                .split_whitespace()
                .map(|f| f.parse().unwrap_or(0))
                .collect();
            Some(NetInterface {
                name: name.trim().to_string(),
                bytes_recv: *fields.first()?,
                bytes_sent: *fields.get(8)?,
            })
        })
        .collect()
}

fn parse_os_release_id(s: &str) -> Option<String> {
    s.lines()
        .find_map(|l| l.strip_prefix("ID="))
        .map(|v| v.trim().trim_matches('"').to_string())
}

/// Sondes thermiques via hwmon, avec repli sur les zones thermiques ACPI
/// quand aucun capteur hwmon n'est exposé.
fn read_temperatures() -> Vec<Temperature> {
    let hwmon = read_hwmon(Path::new("/sys/class/hwmon"));
    if !hwmon.is_empty() {
        return hwmon;
    }
    read_thermal_zones(Path::new("/sys/class/thermal"))
}

fn sorted_entries(dir: &Path, prefix: &str) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn millidegrees(path: &Path) -> Option<f64> {
    Some(read_trimmed(path)?.parse::<f64>().ok()? / 1000.0)
}

fn read_hwmon(base: &Path) -> Vec<Temperature> {
    let mut out = Vec::new();
    for hwmon in sorted_entries(base, "hwmon") {
        let name = read_trimmed(&hwmon.join("name")).unwrap_or_default();
        for input in sorted_entries(&hwmon, "temp") {
            let file = input.file_name().unwrap_or_default().to_string_lossy();
            let Some(index) = file.strip_suffix("_input") else {
                continue;
            };
            let Some(celsius) = millidegrees(&input) else {
                continue;
            };
            let label = read_trimmed(&hwmon.join(format!("{index}_label")));
            out.push(Temperature {
                label: sensor_key(&name, label.as_deref()),
                celsius,
            });
        }
    }
    out
}

/// Clé de sonde au format de gopsutil (`coretemp_packageid0`), que le
/// frontend affiche telle quelle.
fn sensor_key(name: &str, label: Option<&str>) -> String {
    match label.map(|l| l.to_lowercase().split_whitespace().collect::<String>()) {
        Some(l) if !l.is_empty() => format!("{name}_{l}"),
        _ => name.to_string(),
    }
}

fn read_thermal_zones(base: &Path) -> Vec<Temperature> {
    sorted_entries(base, "thermal_zone")
        .into_iter()
        .filter_map(|zone| {
            Some(Temperature {
                label: read_trimmed(&zone.join("type"))?,
                celsius: millidegrees(&zone.join("temp"))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_times_ligne_agregee() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4 5 6 7 8\n";
        let t = parse_cpu_times(stat).unwrap();
        assert_eq!(t.total, 1000.0);
        assert_eq!(t.busy, 150.0);
    }

    #[test]
    fn test_cpu_percent_entre_deux_mesures() {
        let a = CpuTimes {
            total: 1000.0,
            busy: 150.0,
        };
        let b = CpuTimes {
            total: 1100.0,
            busy: 175.0,
        };
        assert_eq!(cpu_percent(a, b), 25.0);
        assert_eq!(cpu_percent(a, a), 0.0);
    }

    #[test]
    fn test_parse_meminfo_used_exclut_cache_et_tampons() {
        let s = "MemTotal:       1000 kB
MemFree:         100 kB
MemAvailable:    600 kB
Buffers:          50 kB
Cached:          300 kB
SReclaimable:     50 kB
SwapTotal:       200 kB
SwapFree:        150 kB
";
        let m = parse_meminfo(s);
        assert_eq!(m.total, 1000 * 1024);
        assert_eq!(m.used, 500 * 1024);
        assert_eq!(m.used_percent, 50.0);
        assert_eq!(m.swap_total, 200 * 1024);
        assert_eq!(m.swap_used, 50 * 1024);
    }

    #[test]
    fn test_parse_net_dev_octets_recus_et_envoyes() {
        let s = "Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:    1234      10    0    0    0     0          0         0     1234      10    0    0    0     0       0          0
  eth0: 987654     900    0    0    0     0          0         0   123456     800    0    0    0     0       0          0
";
        assert_eq!(
            parse_net_dev(s),
            [
                NetInterface {
                    name: "lo".into(),
                    bytes_sent: 1234,
                    bytes_recv: 1234
                },
                NetInterface {
                    name: "eth0".into(),
                    bytes_sent: 123456,
                    bytes_recv: 987654
                },
            ]
        );
    }

    #[test]
    fn test_parse_loadavg_et_uptime() {
        assert_eq!(
            parse_loadavg("0.52 0.58 0.59 1/389 12345\n"),
            Some((0.52, 0.58, 0.59))
        );
        assert_eq!(parse_uptime("350735.47 234388.90\n"), Some(350735));
    }

    #[test]
    fn test_parse_os_release_id_guillemets_retires() {
        assert_eq!(
            parse_os_release_id("PRETTY_NAME=\"Debian GNU/Linux 13\"\nID=debian\n").as_deref(),
            Some("debian")
        );
        assert_eq!(
            parse_os_release_id("ID=\"rocky\"\n").as_deref(),
            Some("rocky")
        );
    }

    #[test]
    fn test_sensor_key_format_gopsutil() {
        assert_eq!(
            sensor_key("coretemp", Some("Package id 0")),
            "coretemp_packageid0"
        );
        assert_eq!(sensor_key("nvme", None), "nvme");
    }

    #[test]
    fn test_read_hwmon_arborescence_synthetique() {
        let tmp = tempfile::tempdir().unwrap();
        let hw = tmp.path().join("hwmon0");
        fs::create_dir(&hw).unwrap();
        fs::write(hw.join("name"), "coretemp\n").unwrap();
        fs::write(hw.join("temp1_input"), "45000\n").unwrap();
        fs::write(hw.join("temp1_label"), "Core 0\n").unwrap();
        fs::write(hw.join("temp1_max"), "100000\n").unwrap();
        assert_eq!(
            read_hwmon(tmp.path()),
            [Temperature {
                label: "coretemp_core0".into(),
                celsius: 45.0
            }]
        );
    }
}
