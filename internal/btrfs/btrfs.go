// Package btrfs interroge l'état des systèmes de fichiers btrfs de l'hôte
// en s'appuyant sur la commande `btrfs` (aucune bibliothèque Go mature
// n'expose ces informations ; le binaire officiel est la source de vérité
// utilisée par les distributions elles-mêmes). Toutes les commandes sont
// invoquées avec une liste d'arguments fixe, sans passage par un shell,
// pour exclure toute injection de commande.
package btrfs

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Filesystem décrit un système de fichiers btrfs détecté sur l'hôte.
type Filesystem struct {
	UUID       string   `json:"uuid"`
	Label      string   `json:"label,omitempty"`
	MountPoint string   `json:"mountPoint,omitempty"`
	TotalBytes uint64   `json:"totalBytes"`
	UsedBytes  uint64   `json:"usedBytes"`
	Devices    []string `json:"devices"`
}

// Usage détaille la répartition allouée/utilisée par type de bloc
// (data, metadata, system) pour un système de fichiers.
type Usage struct {
	DeviceSizeBytes    uint64 `json:"deviceSizeBytes"`
	DeviceAllocated    uint64 `json:"deviceAllocatedBytes"`
	DeviceUnallocated  uint64 `json:"deviceUnallocatedBytes"`
	DataUsedBytes      uint64 `json:"dataUsedBytes"`
	DataTotalBytes     uint64 `json:"dataTotalBytes"`
	MetadataUsedBytes  uint64 `json:"metadataUsedBytes"`
	MetadataTotalBytes uint64 `json:"metadataTotalBytes"`
	FreeEstimatedBytes uint64 `json:"freeEstimatedBytes"`
}

// Subvolume décrit un sous-volume btrfs.
type Subvolume struct {
	ID       int    `json:"id"`
	Path     string `json:"path"`
	ParentID int    `json:"parentId"`
}

// runner permet de mocker l'exécution de la commande btrfs dans les tests.
var runner = func(ctx context.Context, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "btrfs", args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("commande 'btrfs %s' échouée: %w (%s)", strings.Join(args, " "), err, strings.TrimSpace(string(out)))
	}
	return string(out), nil
}

// findmntRunner permet de mocker l'exécution de findmnt dans les tests.
var findmntRunner = func(ctx context.Context, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "findmnt", args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		// Aucun système de fichiers monté correspondant : findmnt retourne
		// un code non-zéro sans qu'il s'agisse d'une erreur applicative.
		return "", nil
	}
	return string(out), nil
}

// ListFilesystems retourne les systèmes de fichiers btrfs détectés via
// `btrfs filesystem show`, enrichis de leur point de montage via findmnt.
func ListFilesystems(ctx context.Context) ([]Filesystem, error) {
	out, err := runner(ctx, "filesystem", "show")
	if err != nil {
		return nil, err
	}
	filesystems := parseFilesystemShow(out)

	mounts, err := mountPointsByUUID(ctx)
	if err == nil {
		for i := range filesystems {
			filesystems[i].MountPoint = mounts[filesystems[i].UUID]
		}
	}
	return filesystems, nil
}

type findmntOutput struct {
	Filesystems []struct {
		Target string `json:"target"`
		UUID   string `json:"uuid"`
	} `json:"filesystems"`
}

// mountPointsByUUID retourne l'association UUID -> point de montage pour
// tous les systèmes de fichiers btrfs actuellement montés.
func mountPointsByUUID(ctx context.Context) (map[string]string, error) {
	out, err := findmntRunner(ctx, "-J", "-t", "btrfs", "-o", "TARGET,UUID")
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(out) == "" {
		return map[string]string{}, nil
	}
	var parsed findmntOutput
	if err := json.Unmarshal([]byte(out), &parsed); err != nil {
		return nil, fmt.Errorf("btrfs: analyse findmnt: %w", err)
	}
	result := make(map[string]string, len(parsed.Filesystems))
	for _, fs := range parsed.Filesystems {
		result[fs.UUID] = fs.Target
	}
	return result, nil
}

var (
	reUUID    = regexp.MustCompile(`uuid:\s*(\S+)`)
	reLabel   = regexp.MustCompile(`Label:\s*(?:'([^']*)'|none)`)
	reDevItem = regexp.MustCompile(`devid\s+\d+\s+size\s+(\S+)\s+used\s+(\S+)\s+path\s+(\S+)`)
)

func parseFilesystemShow(out string) []Filesystem {
	var filesystems []Filesystem
	var current *Filesystem

	scanner := bufio.NewScanner(strings.NewReader(out))
	for scanner.Scan() {
		line := scanner.Text()
		trimmed := strings.TrimSpace(line)

		if strings.HasPrefix(trimmed, "Label:") {
			if current != nil {
				filesystems = append(filesystems, *current)
			}
			current = &Filesystem{}
			if m := reLabel.FindStringSubmatch(trimmed); m != nil {
				current.Label = m[1]
			}
			if m := reUUID.FindStringSubmatch(trimmed); m != nil {
				current.UUID = m[1]
			}
			continue
		}
		if current == nil {
			continue
		}
		if m := reDevItem.FindStringSubmatch(trimmed); m != nil {
			size, _ := parseHumanBytes(m[1])
			used, _ := parseHumanBytes(m[2])
			current.TotalBytes += size
			current.UsedBytes += used
			current.Devices = append(current.Devices, m[3])
		}
	}
	if current != nil {
		filesystems = append(filesystems, *current)
	}
	return filesystems
}

// GetUsage retourne la répartition d'allocation d'un système de fichiers
// via `btrfs filesystem usage -b <path>` (octets bruts, sans conversion).
func GetUsage(ctx context.Context, mountPoint string) (*Usage, error) {
	out, err := runner(ctx, "filesystem", "usage", "-b", mountPoint)
	if err != nil {
		return nil, err
	}
	return parseUsage(out), nil
}

// reOverallLine capture les lignes "Clé:   valeur" de la section Overall.
var reOverallLine = regexp.MustCompile(`^(\S[^:]*):\s+(\d+)`)

// reSectionHeader capture les en-têtes "Data,RAID1: Size:X, Used:Y (Z%)" :
// contrairement à la section Overall, Size et Used y figurent sur la même
// ligne que le nom de la section, séparés par des virgules.
var reSectionHeader = regexp.MustCompile(`^(Data|Metadata|System),\S+:\s*Size:(\d+),\s*Used:(\d+)`)

func parseUsage(out string) *Usage {
	u := &Usage{}
	inOverall := false
	scanner := bufio.NewScanner(strings.NewReader(out))
	for scanner.Scan() {
		trimmed := strings.TrimSpace(scanner.Text())

		if trimmed == "Overall:" {
			inOverall = true
			continue
		}

		if m := reSectionHeader.FindStringSubmatch(trimmed); m != nil {
			inOverall = false
			size, _ := strconv.ParseUint(m[2], 10, 64)
			used, _ := strconv.ParseUint(m[3], 10, 64)
			switch m[1] {
			case "Data":
				u.DataTotalBytes = size
				u.DataUsedBytes = used
			case "Metadata":
				u.MetadataTotalBytes = size
				u.MetadataUsedBytes = used
			}
			continue
		}

		if !inOverall {
			continue
		}
		m := reOverallLine.FindStringSubmatch(trimmed)
		if m == nil {
			continue
		}
		key := strings.TrimSpace(m[1])
		val, _ := strconv.ParseUint(m[2], 10, 64)

		switch key {
		case "Device size":
			u.DeviceSizeBytes = val
		case "Device allocated":
			u.DeviceAllocated = val
		case "Device unallocated":
			u.DeviceUnallocated = val
		case "Free (estimated)":
			u.FreeEstimatedBytes = val
		}
	}
	return u
}

// ListSubvolumes retourne les sous-volumes d'un système de fichiers via
// `btrfs subvolume list <path>`.
func ListSubvolumes(ctx context.Context, mountPoint string) ([]Subvolume, error) {
	out, err := runner(ctx, "subvolume", "list", mountPoint)
	if err != nil {
		return nil, err
	}
	return parseSubvolumeList(out), nil
}

var reSubvolLine = regexp.MustCompile(`^ID (\d+) gen \d+ top level (\d+) path (.+)$`)

func parseSubvolumeList(out string) []Subvolume {
	var subvols []Subvolume
	scanner := bufio.NewScanner(strings.NewReader(out))
	for scanner.Scan() {
		m := reSubvolLine.FindStringSubmatch(strings.TrimSpace(scanner.Text()))
		if m == nil {
			continue
		}
		id, _ := strconv.Atoi(m[1])
		parentID, _ := strconv.Atoi(m[2])
		subvols = append(subvols, Subvolume{ID: id, ParentID: parentID, Path: m[3]})
	}
	return subvols
}

// ScrubStatus décrit l'état d'une opération de scrub en cours ou terminée.
type ScrubStatus struct {
	Running   bool   `json:"running"`
	RawOutput string `json:"rawOutput"`
}

// StartScrub démarre un scrub en arrière-plan sur le système de fichiers
// monté à mountPoint via `btrfs scrub start -B <path>` (le -B fait que la
// commande bloque : on l'exécute donc avec un contexte détaché du cycle de
// vie de la requête HTTP appelante, contrôlé par timeout côté handler).
func StartScrub(ctx context.Context, mountPoint string) error {
	_, err := runner(ctx, "scrub", "start", mountPoint)
	return err
}

// GetScrubStatus retourne l'état du dernier scrub via `btrfs scrub status`.
func GetScrubStatus(ctx context.Context, mountPoint string) (*ScrubStatus, error) {
	out, err := runner(ctx, "scrub", "status", mountPoint)
	if err != nil {
		return nil, err
	}
	return &ScrubStatus{
		Running:   strings.Contains(out, "running"),
		RawOutput: strings.TrimSpace(out),
	}, nil
}

var reHumanBytes = regexp.MustCompile(`^([\d.]+)\s*([KMGTP]?i?B)$`)

// parseHumanBytes convertit une taille lisible par un humain (ex: "1.82TiB")
// telle qu'émise par `btrfs filesystem show` en nombre d'octets.
func parseHumanBytes(s string) (uint64, error) {
	m := reHumanBytes.FindStringSubmatch(strings.TrimSpace(s))
	if m == nil {
		return 0, fmt.Errorf("btrfs: taille illisible: %q", s)
	}
	val, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		return 0, err
	}
	unit := strings.ToUpper(m[2])
	multipliers := map[string]float64{
		"B":   1,
		"KIB": 1 << 10, "MIB": 1 << 20, "GIB": 1 << 30, "TIB": 1 << 40, "PIB": 1 << 50,
		"KB": 1e3, "MB": 1e6, "GB": 1e9, "TB": 1e12, "PB": 1e15,
	}
	mult, ok := multipliers[unit]
	if !ok {
		return 0, fmt.Errorf("btrfs: unité inconnue: %q", unit)
	}
	return uint64(val * mult), nil
}

// DefaultTimeout est le délai maximal accordé aux commandes btrfs
// invoquées depuis un handler HTTP (hors scrub, qui est non bloquant).
const DefaultTimeout = 10 * time.Second
