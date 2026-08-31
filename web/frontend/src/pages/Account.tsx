import { useState, type FormEvent } from 'react'
import { api, ApiError } from '../api/client'
import { useAuth } from '../context/AuthContext'
import { Badge, Button, Card } from '../components/ui'
import { Field } from './Setup'

export function Account() {
  const { user } = useAuth()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [done, setDone] = useState(false)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setDone(false)
    if (next !== confirm) {
      setError('Les deux saisies du nouveau mot de passe ne correspondent pas')
      return
    }
    setBusy(true)
    try {
      await api.patch('/auth/password', { currentPassword: current, newPassword: next })
      setCurrent('')
      setNext('')
      setConfirm('')
      setDone(true)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Erreur inattendue')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Mon compte</h1>

      <Card title="Identité">
        <div className="flex items-center gap-3 text-sm">
          <span className="font-medium">{user?.username}</span>
          <Badge tone={user?.role === 'admin' ? 'accent' : 'neutral'}>
            {user?.role === 'admin' ? 'Administrateur' : 'Utilisateur'}
          </Badge>
        </div>
      </Card>

      <Card title="Changer le mot de passe">
        <form onSubmit={onSubmit} className="space-y-3 max-w-sm">
          <Field label="Mot de passe actuel" type="password" value={current} onChange={setCurrent} />
          <Field label="Nouveau mot de passe" type="password" value={next} onChange={setNext} />
          <Field label="Confirmer le nouveau mot de passe" type="password" value={confirm} onChange={setConfirm} />

          {error && (
            <p className="text-sm" style={{ color: 'var(--danger)' }}>
              {error}
            </p>
          )}
          {done && (
            <p className="text-sm" style={{ color: 'var(--success)' }}>
              Mot de passe modifié. Vos autres sessions ont été déconnectées.
            </p>
          )}

          <Button type="submit" variant="primary" disabled={busy}>
            Modifier le mot de passe
          </Button>

          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            8 caractères minimum. Les sessions ouvertes sur vos autres appareils
            seront déconnectées ; celle-ci reste active.
          </p>
        </form>
      </Card>
    </div>
  )
}
