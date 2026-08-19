import { useCallback, useEffect, useRef, useState } from 'react'
import { Folder, File as FileIcon, FolderPlus, Upload, Trash2, Download, ChevronRight } from 'lucide-react'
import { api, downloadUrl, ApiError } from '../api/client'
import type { FileEntry } from '../api/types'
import { useAuth } from '../context/AuthContext'
import { Button, Card, formatBytes } from '../components/ui'

export function Files() {
  const { user } = useAuth()
  const canManage = user?.role === 'admin'

  const [roots, setRoots] = useState<string[]>([])
  const [root, setRoot] = useState<string | null>(null)
  const [path, setPath] = useState('/')
  const [entries, setEntries] = useState<FileEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    api.get<string[]>('/files/roots').then((r) => {
      setRoots(r)
      if (r.length > 0) setRoot(r[0])
    })
  }, [])

  const load = useCallback(() => {
    if (!root) return
    setError(null)
    api
      .get<FileEntry[]>(`/files/list?root=${encodeURIComponent(root)}&path=${encodeURIComponent(path)}`)
      .then(setEntries)
      .catch((err) => setError(err instanceof ApiError ? err.message : 'Erreur inattendue'))
  }, [root, path])

  useEffect(() => {
    load()
  }, [load])

  function enterDir(name: string) {
    setPath((p) => (p === '/' ? `/${name}` : `${p}/${name}`))
  }

  function goToCrumb(index: number) {
    const parts = path.split('/').filter(Boolean)
    setPath('/' + parts.slice(0, index).join('/'))
  }

  async function createFolder() {
    const name = window.prompt('Nom du nouveau dossier :')
    if (!name || !root) return
    const newPath = path === '/' ? `/${name}` : `${path}/${name}`
    await api.post('/files/mkdir', { root, path: newPath })
    load()
  }

  async function removeEntry(name: string) {
    if (!root) return
    if (!window.confirm(`Supprimer « ${name} » ?`)) return
    const target = path === '/' ? `/${name}` : `${path}/${name}`
    await api.delete(`/files?root=${encodeURIComponent(root)}&path=${encodeURIComponent(target)}`)
    load()
  }

  async function uploadFile(file: File) {
    if (!root) return
    const target = path === '/' ? `/${file.name}` : `${path}/${file.name}`
    const form = new FormData()
    form.append('file', file)
    await fetch(`/api/v1/files/upload?root=${encodeURIComponent(root)}&path=${encodeURIComponent(target)}`, {
      method: 'POST',
      credentials: 'include',
      body: form,
    })
    load()
  }

  const crumbs = path.split('/').filter(Boolean)

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Fichiers</h1>
        {canManage && (
          <div className="flex gap-2">
            <Button onClick={createFolder}>
              <FolderPlus size={15} />
              Nouveau dossier
            </Button>
            <Button
              variant="primary"
              onClick={() => fileInputRef.current?.click()}
            >
              <Upload size={15} />
              Importer
            </Button>
            <input
              ref={fileInputRef}
              type="file"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0]
                if (file) uploadFile(file)
                e.target.value = ''
              }}
            />
          </div>
        )}
      </div>

      {roots.length > 1 && (
        <div className="flex gap-2">
          {roots.map((r) => (
            <button
              key={r}
              onClick={() => {
                setRoot(r)
                setPath('/')
              }}
              className="rounded-lg px-3 py-1.5 text-sm"
              style={{
                background: r === root ? 'var(--accent-soft)' : 'var(--surface-alt)',
                color: r === root ? 'var(--accent)' : 'var(--text-muted)',
              }}
            >
              {r}
            </button>
          ))}
        </div>
      )}

      <div className="flex items-center gap-1 text-sm" style={{ color: 'var(--text-muted)' }}>
        <button onClick={() => setPath('/')} className="hover:underline">
          {root}
        </button>
        {crumbs.map((c, i) => (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight size={14} />
            <button onClick={() => goToCrumb(i + 1)} className="hover:underline">
              {c}
            </button>
          </span>
        ))}
      </div>

      <Card>
        {error ? (
          <p className="text-sm" style={{ color: 'var(--danger)' }}>
            {error}
          </p>
        ) : !entries ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Chargement…
          </p>
        ) : entries.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Dossier vide.
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left" style={{ color: 'var(--text-muted)' }}>
                <th className="font-medium pb-2">Nom</th>
                <th className="font-medium pb-2">Taille</th>
                <th className="font-medium pb-2">Modifié</th>
                <th className="font-medium pb-2 text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.name} className="border-t" style={{ borderColor: 'var(--border)' }}>
                  <td className="py-2">
                    <button
                      className="flex items-center gap-2"
                      onClick={() => (e.isDir ? enterDir(e.name) : undefined)}
                      style={{ cursor: e.isDir ? 'pointer' : 'default' }}
                    >
                      {e.isDir ? <Folder size={16} style={{ color: 'var(--accent)' }} /> : <FileIcon size={16} style={{ color: 'var(--text-muted)' }} />}
                      {e.name}
                    </button>
                  </td>
                  <td className="py-2" style={{ color: 'var(--text-muted)' }}>
                    {e.isDir ? '—' : formatBytes(e.sizeBytes)}
                  </td>
                  <td className="py-2" style={{ color: 'var(--text-muted)' }}>
                    {new Date(e.modTime).toLocaleString('fr-FR')}
                  </td>
                  <td className="py-2">
                    <div className="flex justify-end gap-1.5">
                      {!e.isDir && root && (
                        <a
                          href={downloadUrl(root, path === '/' ? `/${e.name}` : `${path}/${e.name}`)}
                          className="p-1.5 rounded-md"
                          style={{ background: 'var(--surface-alt)' }}
                          title="Télécharger"
                        >
                          <Download size={15} />
                        </a>
                      )}
                      {canManage && (
                        <button
                          onClick={() => removeEntry(e.name)}
                          className="p-1.5 rounded-md"
                          style={{ background: 'var(--surface-alt)' }}
                          title="Supprimer"
                        >
                          <Trash2 size={15} style={{ color: 'var(--danger)' }} />
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  )
}
