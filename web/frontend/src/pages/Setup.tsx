import { useState, type FormEvent } from 'react'
import { Server } from 'lucide-react'
import { useAuth } from '../context/AuthContext'
import { ApiError } from '../api/client'
import { Button } from '../components/ui'

export function Setup() {
  const { setup } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    if (password !== confirm) {
      setError('Les mots de passe ne correspondent pas')
      return
    }
    setBusy(true)
    try {
      await setup(username, password)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Erreur inattendue')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="min-h-full flex items-center justify-center p-6" style={{ background: 'var(--bg)' }}>
      <div className="w-full max-w-sm rounded-xl border p-6" style={{ background: 'var(--surface)', borderColor: 'var(--border)' }}>
        <div className="flex flex-col items-center gap-2 mb-6">
          <Server size={28} style={{ color: 'var(--accent)' }} />
          <h1 className="text-lg font-semibold">Bienvenue sur silo</h1>
          <p className="text-sm text-center" style={{ color: 'var(--text-muted)' }}>
            Créez le compte administrateur pour commencer.
          </p>
        </div>

        <form onSubmit={onSubmit} className="space-y-3">
          <Field label="Identifiant" value={username} onChange={setUsername} autoFocus />
          <Field label="Mot de passe" type="password" value={password} onChange={setPassword} />
          <Field label="Confirmer le mot de passe" type="password" value={confirm} onChange={setConfirm} />

          {error && (
            <p className="text-sm" style={{ color: 'var(--danger)' }}>
              {error}
            </p>
          )}

          <Button type="submit" variant="primary" disabled={busy}>
            Créer le compte administrateur
          </Button>
        </form>
      </div>
    </div>
  )
}

export function Field({
  label,
  value,
  onChange,
  type = 'text',
  autoFocus,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  type?: string
  autoFocus?: boolean
}) {
  return (
    <label className="block text-sm">
      <span style={{ color: 'var(--text-muted)' }}>{label}</span>
      <input
        type={type}
        value={value}
        autoFocus={autoFocus}
        onChange={(e) => onChange(e.target.value)}
        required
        className="mt-1 w-full rounded-lg border px-3 py-2 text-sm outline-none"
        style={{ background: 'var(--surface-alt)', borderColor: 'var(--border)', color: 'var(--text)' }}
      />
    </label>
  )
}
