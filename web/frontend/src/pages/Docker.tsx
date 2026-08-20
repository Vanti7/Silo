import { useState, type ReactNode } from 'react'
import { Play, Square, RotateCw, Trash2, FileText, X } from 'lucide-react'
import { api, logsUrl } from '../api/client'
import type { Container, DockerInfo } from '../api/types'
import { usePolling } from '../hooks/usePolling'
import { useAuth } from '../context/AuthContext'
import { Badge, Card } from '../components/ui'

export function Docker() {
  const { user } = useAuth()
  const canManage = user?.role === 'admin'
  const { data: containers, error } = usePolling<Container[]>(() => api.get('/docker/containers'), 5000)
  const { data: info } = usePolling<DockerInfo>(() => api.get('/docker/info'), 10000)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [logsFor, setLogsFor] = useState<Container | null>(null)

  async function act(id: string, action: 'start' | 'stop' | 'restart' | 'remove') {
    setBusyId(id)
    try {
      if (action === 'remove') {
        await api.delete(`/docker/containers/${id}`)
      } else {
        await api.post(`/docker/containers/${id}/${action}`)
      }
    } finally {
      setBusyId(null)
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Docker</h1>
        {info && (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            {info.serverVersion} · {info.storageDriver}
          </p>
        )}
      </div>

      <Card>
        {error ? (
          <p className="text-sm" style={{ color: 'var(--danger)' }}>
            Daemon Docker injoignable.
          </p>
        ) : !containers || containers.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Aucun container.
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left" style={{ color: 'var(--text-muted)' }}>
                <th className="font-medium pb-2">Nom</th>
                <th className="font-medium pb-2">Image</th>
                <th className="font-medium pb-2">État</th>
                <th className="font-medium pb-2">Ports</th>
                <th className="font-medium pb-2 text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {containers.map((c) => {
                const name = c.names[0]?.replace(/^\//, '') ?? c.id.slice(0, 12)
                const running = c.state === 'running'
                const busy = busyId === c.id
                return (
                  <tr key={c.id} className="border-t" style={{ borderColor: 'var(--border)' }}>
                    <td className="py-2 font-medium">{name}</td>
                    <td className="py-2" style={{ color: 'var(--text-muted)' }}>
                      {c.image}
                    </td>
                    <td className="py-2">
                      <Badge tone={running ? 'success' : 'neutral'}>{c.status}</Badge>
                    </td>
                    <td className="py-2" style={{ color: 'var(--text-muted)' }}>
                      {c.ports.filter((p) => p.publicPort).map((p) => `${p.publicPort}:${p.privatePort}`).join(', ') || '—'}
                    </td>
                    <td className="py-2">
                      <div className="flex justify-end gap-1.5">
                        <IconButton title="Logs" onClick={() => setLogsFor(c)}>
                          <FileText size={15} />
                        </IconButton>
                        {canManage && (
                          <>
                            {running ? (
                              <IconButton title="Arrêter" disabled={busy} onClick={() => act(c.id, 'stop')}>
                                <Square size={15} />
                              </IconButton>
                            ) : (
                              <IconButton title="Démarrer" disabled={busy} onClick={() => act(c.id, 'start')}>
                                <Play size={15} />
                              </IconButton>
                            )}
                            <IconButton title="Redémarrer" disabled={busy} onClick={() => act(c.id, 'restart')}>
                              <RotateCw size={15} />
                            </IconButton>
                            <IconButton title="Supprimer" disabled={busy} onClick={() => act(c.id, 'remove')}>
                              <Trash2 size={15} style={{ color: 'var(--danger)' }} />
                            </IconButton>
                          </>
                        )}
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </Card>

      {logsFor && <LogsModal container={logsFor} onClose={() => setLogsFor(null)} />}
    </div>
  )
}

function IconButton({ children, onClick, title, disabled }: { children: ReactNode; onClick: () => void; title: string; disabled?: boolean }) {
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      className="p-1.5 rounded-md disabled:opacity-40"
      style={{ background: 'var(--surface-alt)' }}
    >
      {children}
    </button>
  )
}

function LogsModal({ container, onClose }: { container: Container; onClose: () => void }) {
  const name = container.names[0]?.replace(/^\//, '') ?? container.id.slice(0, 12)
  const { data: logs } = usePolling<string>(
    () => fetch(logsUrl(container.id), { credentials: 'include' }).then((r) => r.text()),
    3000,
    [container.id],
  )

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6" style={{ background: 'rgba(0,0,0,0.5)' }} onClick={onClose}>
      <div
        className="w-full max-w-3xl h-[70vh] rounded-xl border flex flex-col"
        style={{ background: 'var(--surface)', borderColor: 'var(--border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 h-12 border-b shrink-0" style={{ borderColor: 'var(--border)' }}>
          <span className="text-sm font-medium">Logs — {name}</span>
          <button onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <pre className="flex-1 overflow-auto p-4 text-xs whitespace-pre-wrap" style={{ color: 'var(--text-muted)' }}>
          {logs || 'Aucun log.'}
        </pre>
      </div>
    </div>
  )
}
