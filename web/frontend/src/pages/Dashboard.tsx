import { Link } from 'react-router-dom'
import { Cpu, MemoryStick, Thermometer, Clock } from 'lucide-react'
import { api } from '../api/client'
import type { DockerInfo, Filesystem, SystemInfo, SystemStats } from '../api/types'
import { usePolling } from '../hooks/usePolling'
import { Card, ProgressBar, StatTile, formatBytes, formatUptime } from '../components/ui'

export function Dashboard() {
  const { data: stats } = usePolling<SystemStats>(() => api.get('/system/stats'), 3000)
  const { data: info } = usePolling<SystemInfo>(() => api.get('/system/info'), 60000)
  const { data: filesystems } = usePolling<Filesystem[]>(() => api.get('/storage/filesystems'), 10000)
  const { data: dockerInfo } = usePolling<DockerInfo>(() => api.get('/docker/info'), 10000)

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold">Tableau de bord</h1>
        {info && (
          <p className="text-sm mt-1" style={{ color: 'var(--text-muted)' }}>
            {info.hostname} · {info.platform} · {info.kernel} ({info.arch})
          </p>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <Card title="Processeur">
          <div className="flex items-center gap-4">
            <Cpu size={28} style={{ color: 'var(--accent)' }} />
            <div className="flex-1">
              <StatTile label="Utilisation CPU" value={`${(stats?.cpuPercent ?? 0).toFixed(1)} %`} />
              <div className="mt-3">
                <ProgressBar percent={stats?.cpuPercent ?? 0} tone={cpuTone(stats?.cpuPercent)} />
              </div>
              <p className="text-xs mt-2" style={{ color: 'var(--text-muted)' }}>
                Charge : {stats?.loadAvg1.toFixed(2)} / {stats?.loadAvg5.toFixed(2)} / {stats?.loadAvg15.toFixed(2)}
              </p>
            </div>
          </div>
        </Card>

        <Card title="Mémoire">
          <div className="flex items-center gap-4">
            <MemoryStick size={28} style={{ color: 'var(--accent)' }} />
            <div className="flex-1">
              <StatTile
                label="RAM utilisée"
                value={`${formatBytes(stats?.memUsedBytes ?? 0)} / ${formatBytes(stats?.memTotalBytes ?? 0)}`}
              />
              <div className="mt-3">
                <ProgressBar percent={stats?.memPercent ?? 0} tone={cpuTone(stats?.memPercent)} />
              </div>
              {stats && stats.swapTotalBytes > 0 && (
                <p className="text-xs mt-2" style={{ color: 'var(--text-muted)' }}>
                  Swap : {formatBytes(stats.swapUsedBytes)} / {formatBytes(stats.swapTotalBytes)}
                </p>
              )}
            </div>
          </div>
        </Card>

        <Card title="Disponibilité">
          <div className="flex items-center gap-4">
            <Clock size={28} style={{ color: 'var(--accent)' }} />
            <StatTile label="En ligne depuis" value={stats ? formatUptime(stats.uptimeSeconds) : '—'} />
          </div>
        </Card>

        <Card title="Température">
          <div className="flex items-center gap-4">
            <Thermometer size={28} style={{ color: 'var(--accent)' }} />
            <div>
              {stats?.temperatures && stats.temperatures.length > 0 ? (
                <StatTile label={stats.temperatures[0].label} value={`${stats.temperatures[0].celsius.toFixed(0)} °C`} />
              ) : (
                <StatTile label="Sondes" value="Non disponibles" />
              )}
            </div>
          </div>
        </Card>
      </div>

      <Card title="Stockage" action={<Link to="/storage" className="text-sm" style={{ color: 'var(--accent)' }}>Détails →</Link>}>
        {!filesystems || filesystems.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Aucun système de fichiers btrfs détecté.
          </p>
        ) : (
          <div className="space-y-4">
            {filesystems.map((fs) => {
              const percent = fs.totalBytes ? (fs.usedBytes / fs.totalBytes) * 100 : 0
              return (
                <div key={fs.uuid}>
                  <div className="flex items-center justify-between text-sm mb-1">
                    <span className="font-medium">{fs.label || fs.mountPoint || fs.uuid}</span>
                    <span style={{ color: 'var(--text-muted)' }}>
                      {formatBytes(fs.usedBytes)} / {formatBytes(fs.totalBytes)}
                    </span>
                  </div>
                  <ProgressBar percent={percent} tone={cpuTone(percent)} />
                </div>
              )
            })}
          </div>
        )}
      </Card>

      <Card title="Docker" action={<Link to="/docker" className="text-sm" style={{ color: 'var(--accent)' }}>Détails →</Link>}>
        {dockerInfo ? (
          <div className="grid grid-cols-3 gap-4">
            <StatTile label="Containers actifs" value={dockerInfo.containersRunning} />
            <StatTile label="Containers arrêtés" value={dockerInfo.containersStopped} />
            <StatTile label="Images" value={dockerInfo.images} />
          </div>
        ) : (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Daemon Docker injoignable.
          </p>
        )}
      </Card>
    </div>
  )
}

function cpuTone(percent?: number): 'success' | 'warning' | 'danger' {
  if (percent === undefined) return 'success'
  if (percent >= 90) return 'danger'
  if (percent >= 70) return 'warning'
  return 'success'
}
