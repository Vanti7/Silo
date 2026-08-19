import { useEffect, useState } from 'react'
import { HardDrive, RefreshCw } from 'lucide-react'
import { api } from '../api/client'
import type { Filesystem, ScrubStatus, Subvolume, Usage } from '../api/types'
import { usePolling } from '../hooks/usePolling'
import { useAuth } from '../context/AuthContext'
import { Badge, Button, Card, ProgressBar, formatBytes } from '../components/ui'

export function Storage() {
  const { user } = useAuth()
  const { data: filesystems } = usePolling<Filesystem[]>(() => api.get('/storage/filesystems'), 10000)
  const [selected, setSelected] = useState<Filesystem | null>(null)

  useEffect(() => {
    if (!selected && filesystems && filesystems.length > 0) {
      setSelected(filesystems.find((fs) => fs.mountPoint) ?? filesystems[0])
    }
  }, [filesystems, selected])

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Stockage btrfs</h1>

      {!filesystems || filesystems.length === 0 ? (
        <Card>
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Aucun système de fichiers btrfs détecté sur cet hôte.
          </p>
        </Card>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          <div className="space-y-2">
            {filesystems.map((fs) => {
              const percent = fs.totalBytes ? (fs.usedBytes / fs.totalBytes) * 100 : 0
              const active = selected?.uuid === fs.uuid
              return (
                <button
                  key={fs.uuid}
                  onClick={() => setSelected(fs)}
                  className="w-full text-left rounded-xl border p-4"
                  style={{
                    background: active ? 'var(--accent-soft)' : 'var(--surface)',
                    borderColor: active ? 'var(--accent)' : 'var(--border)',
                  }}
                >
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <HardDrive size={16} style={{ color: 'var(--accent)' }} />
                    {fs.label || fs.mountPoint || fs.uuid}
                  </div>
                  <p className="text-xs mt-1 truncate" style={{ color: 'var(--text-muted)' }}>
                    {fs.mountPoint || 'non monté'}
                  </p>
                  <div className="mt-2">
                    <ProgressBar percent={percent} />
                  </div>
                  <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
                    {formatBytes(fs.usedBytes)} / {formatBytes(fs.totalBytes)}
                  </p>
                </button>
              )
            })}
          </div>

          <div className="lg:col-span-2">
            {selected && <FilesystemDetail fs={selected} canManage={user?.role === 'admin'} />}
          </div>
        </div>
      )}
    </div>
  )
}

function FilesystemDetail({ fs, canManage }: { fs: Filesystem; canManage: boolean }) {
  const mountPoint = fs.mountPoint
  const { data: usage } = usePolling<Usage>(
    () => (mountPoint ? api.get(`/storage/usage?path=${encodeURIComponent(mountPoint)}`) : Promise.resolve(null as unknown as Usage)),
    10000,
    [mountPoint],
  )
  const { data: subvolumes } = usePolling<Subvolume[]>(
    () => (mountPoint ? api.get(`/storage/subvolumes?path=${encodeURIComponent(mountPoint)}`) : Promise.resolve([])),
    15000,
    [mountPoint],
  )
  const { data: scrub, error: scrubError } = usePolling<ScrubStatus>(
    () => (mountPoint ? api.get(`/storage/scrub?path=${encodeURIComponent(mountPoint)}`) : Promise.resolve(null as unknown as ScrubStatus)),
    10000,
    [mountPoint],
  )
  const [scrubBusy, setScrubBusy] = useState(false)

  if (!mountPoint) {
    return (
      <Card>
        <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
          Ce système de fichiers n'est pas monté : détails indisponibles.
        </p>
      </Card>
    )
  }

  async function startScrub() {
    setScrubBusy(true)
    try {
      await api.post('/storage/scrub', { path: mountPoint })
    } finally {
      setScrubBusy(false)
    }
  }

  return (
    <div className="space-y-4">
      <Card title="Périphériques">
        <ul className="text-sm space-y-1">
          {fs.devices.map((d) => (
            <li key={d} style={{ color: 'var(--text-muted)' }}>
              {d}
            </li>
          ))}
        </ul>
      </Card>

      {usage && (
        <Card title="Allocation">
          <div className="space-y-4">
            <UsageRow label="Données" used={usage.dataUsedBytes} total={usage.dataTotalBytes} />
            <UsageRow label="Métadonnées" used={usage.metadataUsedBytes} total={usage.metadataTotalBytes} />
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
              Libre estimé : {formatBytes(usage.freeEstimatedBytes)} · Alloué : {formatBytes(usage.deviceAllocatedBytes)} / {formatBytes(usage.deviceSizeBytes)}
            </p>
          </div>
        </Card>
      )}

      <Card
        title="Scrub"
        action={
          canManage && (
            <Button onClick={startScrub} disabled={scrubBusy || scrub?.running}>
              <RefreshCw size={14} />
              {scrub?.running ? 'En cours…' : 'Lancer un scrub'}
            </Button>
          )
        }
      >
        {scrubError ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Statut indisponible.
          </p>
        ) : (
          <div className="space-y-2">
            <Badge tone={scrub?.running ? 'warning' : 'success'}>{scrub?.running ? 'En cours' : 'Inactif'}</Badge>
            {scrub?.rawOutput && (
              <pre className="text-xs whitespace-pre-wrap rounded-lg p-3" style={{ background: 'var(--surface-alt)', color: 'var(--text-muted)' }}>
                {scrub.rawOutput}
              </pre>
            )}
          </div>
        )}
      </Card>

      <Card title="Sous-volumes">
        {!subvolumes || subvolumes.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Aucun sous-volume.
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left" style={{ color: 'var(--text-muted)' }}>
                <th className="font-medium pb-2">ID</th>
                <th className="font-medium pb-2">Chemin</th>
              </tr>
            </thead>
            <tbody>
              {subvolumes.map((sv) => (
                <tr key={sv.id} className="border-t" style={{ borderColor: 'var(--border)' }}>
                  <td className="py-1.5">{sv.id}</td>
                  <td className="py-1.5">{sv.path}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  )
}

function UsageRow({ label, used, total }: { label: string; used: number; total: number }) {
  const percent = total ? (used / total) * 100 : 0
  return (
    <div>
      <div className="flex items-center justify-between text-sm mb-1">
        <span>{label}</span>
        <span style={{ color: 'var(--text-muted)' }}>
          {formatBytes(used)} / {formatBytes(total)}
        </span>
      </div>
      <ProgressBar percent={percent} />
    </div>
  )
}
