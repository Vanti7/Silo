package btrfs

import (
	"context"
	"testing"
)

// tib/gib/mib convertissent en octets via une conversion à l'exécution
// (et non une constante), pour éviter le refus du compilateur de tronquer
// une constante flottante non exactement représentable en entier.
func tib(x float64) uint64 { return uint64(x * (1 << 40)) }
func gib(x float64) uint64 { return uint64(x * (1 << 30)) }

func TestParseFilesystemShow_UneSeuleGrappe(t *testing.T) {
	const out = `Label: 'pool'  uuid: 1a2b3c4d-0000-1111-2222-333344445555
	Total devices 2 FS bytes used 1.82TiB
	devid    1 size 3.64TiB used 1.85TiB path /dev/sda1
	devid    2 size 3.64TiB used 1.85TiB path /dev/sdb1

`
	got := parseFilesystemShow(out)
	if len(got) != 1 {
		t.Fatalf("nombre de systèmes de fichiers = %d, attendu 1", len(got))
	}
	fs := got[0]
	if fs.UUID != "1a2b3c4d-0000-1111-2222-333344445555" {
		t.Errorf("UUID = %q", fs.UUID)
	}
	if fs.Label != "pool" {
		t.Errorf("Label = %q, attendu %q", fs.Label, "pool")
	}
	if len(fs.Devices) != 2 {
		t.Fatalf("nombre de devices = %d, attendu 2", len(fs.Devices))
	}
	wantUsed := tib(1.85) * 2
	if fs.UsedBytes != wantUsed {
		t.Errorf("UsedBytes = %d, attendu %d", fs.UsedBytes, wantUsed)
	}
}

func TestParseFilesystemShow_LabelAbsent(t *testing.T) {
	const out = `Label: none  uuid: 1a2b3c4d-0000-1111-2222-333344445555
	Total devices 1 FS bytes used 512.00GiB
	devid    1 size 1.00TiB used 512.00GiB path /dev/sda1
`
	got := parseFilesystemShow(out)
	if len(got) != 1 {
		t.Fatalf("nombre de systèmes de fichiers = %d, attendu 1", len(got))
	}
	if got[0].Label != "" {
		t.Errorf("Label = %q, attendu vide pour 'none'", got[0].Label)
	}
}

func TestParseFilesystemShow_PlusieursGrappes(t *testing.T) {
	const out = `Label: 'a'  uuid: 11111111-1111-1111-1111-111111111111
	devid    1 size 1.00TiB used 500.00GiB path /dev/sda1

Label: 'b'  uuid: 22222222-2222-2222-2222-222222222222
	devid    1 size 2.00TiB used 1.00TiB path /dev/sdb1
`
	got := parseFilesystemShow(out)
	if len(got) != 2 {
		t.Fatalf("nombre de systèmes de fichiers = %d, attendu 2", len(got))
	}
	if got[0].UUID == got[1].UUID {
		t.Error("les deux systèmes de fichiers ne devraient pas partager le même UUID")
	}
}

func TestParseUsage(t *testing.T) {
	// Format réel de `btrfs filesystem usage -b` : les valeurs Size/Used
	// des sections Data/Metadata/System figurent sur la même ligne que
	// l'en-tête de section (séparées par des virgules), contrairement aux
	// lignes "Clé: valeur" de la section Overall.
	const out = `Overall:
    Device size:		4000000000000
    Device allocated:		2000000000000
    Device unallocated:	2000000000000
    Used:			1800000000000
    Free (estimated):		1800000000000	(min: 900000000000)
    Data ratio:				   2
    Metadata ratio:			   2
    Global reserve:		536870912	(used: 0)

Data,RAID1: Size:900000000000, Used:850000000000 (94.44%)
   /dev/sda1     900000000000
   /dev/sdb1     900000000000

Metadata,RAID1: Size:10000000000, Used:5000000000 (50.00%)
   /dev/sda1      10000000000
   /dev/sdb1      10000000000

System,RAID1: Size:33554432, Used:114688 (0.34%)
   /dev/sda1        33554432
   /dev/sdb1        33554432
`
	got := parseUsage(out)
	if got.DeviceSizeBytes != 4000000000000 {
		t.Errorf("DeviceSizeBytes = %d", got.DeviceSizeBytes)
	}
	if got.DataTotalBytes != 900000000000 {
		t.Errorf("DataTotalBytes = %d", got.DataTotalBytes)
	}
	if got.DataUsedBytes != 850000000000 {
		t.Errorf("DataUsedBytes = %d", got.DataUsedBytes)
	}
	if got.MetadataTotalBytes != 10000000000 {
		t.Errorf("MetadataTotalBytes = %d", got.MetadataTotalBytes)
	}
	if got.MetadataUsedBytes != 5000000000 {
		t.Errorf("MetadataUsedBytes = %d", got.MetadataUsedBytes)
	}
	if got.FreeEstimatedBytes != 1800000000000 {
		t.Errorf("FreeEstimatedBytes = %d", got.FreeEstimatedBytes)
	}
}

func TestParseSubvolumeList(t *testing.T) {
	const out = `ID 256 gen 120 top level 5 path @home
ID 257 gen 121 top level 5 path @snapshots/2026-01-01
`
	got := parseSubvolumeList(out)
	if len(got) != 2 {
		t.Fatalf("nombre de sous-volumes = %d, attendu 2", len(got))
	}
	if got[0].ID != 256 || got[0].Path != "@home" {
		t.Errorf("premier sous-volume = %+v", got[0])
	}
	if got[1].ParentID != 5 {
		t.Errorf("ParentID = %d, attendu 5", got[1].ParentID)
	}
}

func TestParseHumanBytes(t *testing.T) {
	cases := map[string]uint64{
		"1.82TiB":   tib(1.82),
		"512.00GiB": gib(512.00),
		"0.00B":     0,
		"1TB":       1_000_000_000_000,
	}
	for in, want := range cases {
		got, err := parseHumanBytes(in)
		if err != nil {
			t.Errorf("parseHumanBytes(%q) erreur: %v", in, err)
			continue
		}
		if got != want {
			t.Errorf("parseHumanBytes(%q) = %d, attendu %d", in, got, want)
		}
	}
}

func TestParseHumanBytes_Invalide(t *testing.T) {
	if _, err := parseHumanBytes("pas-une-taille"); err == nil {
		t.Error("attendu une erreur pour une entrée invalide")
	}
}

func TestMountPointsByUUID(t *testing.T) {
	orig := findmntRunner
	defer func() { findmntRunner = orig }()

	findmntRunner = func(_ context.Context, _ ...string) (string, error) {
		return `{"filesystems": [{"target": "/mnt/pool", "uuid": "1a2b3c4d-0000-1111-2222-333344445555"}]}`, nil
	}

	got, err := mountPointsByUUID(context.Background())
	if err != nil {
		t.Fatalf("erreur inattendue: %v", err)
	}
	if got["1a2b3c4d-0000-1111-2222-333344445555"] != "/mnt/pool" {
		t.Errorf("point de montage non résolu: %v", got)
	}
}
