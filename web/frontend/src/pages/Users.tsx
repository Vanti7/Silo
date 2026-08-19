import { useEffect, useState, type FormEvent } from 'react'
import { Trash2, UserPlus } from 'lucide-react'
import { api, ApiError } from '../api/client'
import type { User } from '../api/types'
import { useAuth } from '../context/AuthContext'
import { Badge, Button, Card } from '../components/ui'
import { Field } from './Setup'

export function Users() {
  const { user: me } = useAuth()
  const [users, setUsers] = useState<User[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)

  function load() {
    api.get<User[]>('/users').then(setUsers).catch((err) => setError(err instanceof ApiError ? err.message : 'Erreur'))
  }

  useEffect(load, [])

  async function changeRole(u: User, role: 'admin' | 'user') {
    setError(null)
    try {
      await api.patch(`/users/${u.id}`, { role })
      load()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Erreur')
    }
  }

  async function removeUser(u: User) {
    if (!window.confirm(`Supprimer le compte « ${u.username} » ?`)) return
    setError(null)
    try {
      await api.delete(`/users/${u.id}`)
      load()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Erreur')
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Utilisateurs</h1>
        <Button variant="primary" onClick={() => setShowForm(true)}>
          <UserPlus size={15} />
          Nouvel utilisateur
        </Button>
      </div>

      {error && (
        <p className="text-sm" style={{ color: 'var(--danger)' }}>
          {error}
        </p>
      )}

      {showForm && (
        <CreateUserForm
          onClose={() => setShowForm(false)}
          onCreated={() => {
            setShowForm(false)
            load()
          }}
        />
      )}

      <Card>
        {!users ? (
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
            Chargement…
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left" style={{ color: 'var(--text-muted)' }}>
                <th className="font-medium pb-2">Identifiant</th>
                <th className="font-medium pb-2">Rôle</th>
                <th className="font-medium pb-2 text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id} className="border-t" style={{ borderColor: 'var(--border)' }}>
                  <td className="py-2 font-medium">
                    {u.username} {u.id === me?.id && <Badge>vous</Badge>}
                  </td>
                  <td className="py-2">
                    <select
                      value={u.role}
                      onChange={(e) => changeRole(u, e.target.value as 'admin' | 'user')}
                      className="rounded-md border px-2 py-1 text-sm"
                      style={{ background: 'var(--surface-alt)', borderColor: 'var(--border)', color: 'var(--text)' }}
                    >
                      <option value="admin">Administrateur</option>
                      <option value="user">Utilisateur</option>
                    </select>
                  </td>
                  <td className="py-2 text-right">
                    <button
                      onClick={() => removeUser(u)}
                      className="p-1.5 rounded-md"
                      style={{ background: 'var(--surface-alt)' }}
                      title="Supprimer"
                    >
                      <Trash2 size={15} style={{ color: 'var(--danger)' }} />
                    </button>
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

function CreateUserForm({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<'admin' | 'user'>('user')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await api.post('/users', { username, password, role })
      onCreated()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Erreur inattendue')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Card title="Nouvel utilisateur">
      <form onSubmit={onSubmit} className="space-y-3 max-w-sm">
        <Field label="Identifiant" value={username} onChange={setUsername} autoFocus />
        <Field label="Mot de passe" type="password" value={password} onChange={setPassword} />
        <label className="block text-sm">
          <span style={{ color: 'var(--text-muted)' }}>Rôle</span>
          <select
            value={role}
            onChange={(e) => setRole(e.target.value as 'admin' | 'user')}
            className="mt-1 w-full rounded-lg border px-3 py-2 text-sm"
            style={{ background: 'var(--surface-alt)', borderColor: 'var(--border)', color: 'var(--text)' }}
          >
            <option value="user">Utilisateur</option>
            <option value="admin">Administrateur</option>
          </select>
        </label>
        {error && (
          <p className="text-sm" style={{ color: 'var(--danger)' }}>
            {error}
          </p>
        )}
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={busy}>
            Créer
          </Button>
          <Button onClick={onClose}>Annuler</Button>
        </div>
      </form>
    </Card>
  )
}
