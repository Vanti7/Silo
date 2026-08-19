// Package system expose les métriques matérielles et système de l'hôte
// (CPU, mémoire, réseau, température, uptime) pour le tableau de bord.
package system

import (
	"fmt"
	"os"
	"time"

	"github.com/shirou/gopsutil/v4/cpu"
	"github.com/shirou/gopsutil/v4/host"
	"github.com/shirou/gopsutil/v4/load"
	"github.com/shirou/gopsutil/v4/mem"
	"github.com/shirou/gopsutil/v4/net"
	"github.com/shirou/gopsutil/v4/sensors"
)

// Stats regroupe un instantané des métriques système.
type Stats struct {
	CPUPercent     float64        `json:"cpuPercent"`
	LoadAvg1       float64        `json:"loadAvg1"`
	LoadAvg5       float64        `json:"loadAvg5"`
	LoadAvg15      float64        `json:"loadAvg15"`
	MemTotalBytes  uint64         `json:"memTotalBytes"`
	MemUsedBytes   uint64         `json:"memUsedBytes"`
	MemPercent     float64        `json:"memPercent"`
	SwapTotalBytes uint64         `json:"swapTotalBytes"`
	SwapUsedBytes  uint64         `json:"swapUsedBytes"`
	UptimeSeconds  uint64         `json:"uptimeSeconds"`
	Temperatures   []Temperature  `json:"temperatures,omitempty"`
	Network        []NetInterface `json:"network"`
}

// Temperature est une sonde thermique (ex: CPU, disque).
type Temperature struct {
	Label   string  `json:"label"`
	Celsius float64 `json:"celsius"`
}

// NetInterface est un compteur cumulé d'octets envoyés/reçus par interface.
type NetInterface struct {
	Name      string `json:"name"`
	BytesSent uint64 `json:"bytesSent"`
	BytesRecv uint64 `json:"bytesRecv"`
}

// Info regroupe des informations statiques sur l'hôte.
type Info struct {
	Hostname string `json:"hostname"`
	OS       string `json:"os"`
	Platform string `json:"platform"`
	Kernel   string `json:"kernel"`
	Arch     string `json:"arch"`
}

// Collect prend un instantané des métriques système courantes.
func Collect() (*Stats, error) {
	s := &Stats{}

	cpuPercents, err := cpu.Percent(200*time.Millisecond, false)
	if err != nil {
		return nil, fmt.Errorf("system: lecture CPU: %w", err)
	}
	if len(cpuPercents) > 0 {
		s.CPUPercent = cpuPercents[0]
	}

	if avg, err := load.Avg(); err == nil {
		s.LoadAvg1, s.LoadAvg5, s.LoadAvg15 = avg.Load1, avg.Load5, avg.Load15
	}

	vm, err := mem.VirtualMemory()
	if err != nil {
		return nil, fmt.Errorf("system: lecture mémoire: %w", err)
	}
	s.MemTotalBytes = vm.Total
	s.MemUsedBytes = vm.Used
	s.MemPercent = vm.UsedPercent

	if sw, err := mem.SwapMemory(); err == nil {
		s.SwapTotalBytes = sw.Total
		s.SwapUsedBytes = sw.Used
	}

	if info, err := host.Info(); err == nil {
		s.UptimeSeconds = info.Uptime
	}

	if temps, err := sensors.SensorsTemperatures(); err == nil {
		for _, t := range temps {
			s.Temperatures = append(s.Temperatures, Temperature{
				Label:   t.SensorKey,
				Celsius: t.Temperature,
			})
		}
	}

	if counters, err := net.IOCounters(true); err == nil {
		for _, c := range counters {
			s.Network = append(s.Network, NetInterface{
				Name:      c.Name,
				BytesSent: c.BytesSent,
				BytesRecv: c.BytesRecv,
			})
		}
	}

	return s, nil
}

// CollectInfo retourne les informations statiques de l'hôte.
func CollectInfo() (*Info, error) {
	hostname, err := os.Hostname()
	if err != nil {
		return nil, fmt.Errorf("system: lecture hostname: %w", err)
	}

	info := &Info{Hostname: hostname}
	if hi, err := host.Info(); err == nil {
		info.OS = hi.OS
		info.Platform = hi.Platform
		info.Kernel = hi.KernelVersion
		info.Arch = hi.KernelArch
	}
	return info, nil
}
