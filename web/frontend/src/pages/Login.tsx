import { useState, type FormEvent } from 'react'
import { Server } from 'lucide-react'
import { useAuth } from '../context/AuthContext'
import { ApiError } from '../api/client'
import { Button } from '../components/ui'
import { Field } from './Setup'

export function Login() {
  const { login } = useAuth()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    setBusy(true)
    try {
      await login(username, password)
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
          <h1 className="text-lg font-semibold">silo</h1>
        </div>

        <form onSubmit={onSubmit} className="space-y-3">
          <Field label="Identifiant" value={username} onChange={setUsername} autoFocus />
          <Field label="Mot de passe" type="password" value={password} onChange={setPassword} />

          {error && (
            <p className="text-sm" style={{ color: 'var(--danger)' }}>
              {error}
            </p>
          )}

          <Button type="submit" variant="primary" disabled={busy}>
            Se connecter
          </Button>
        </form>
      </div>
    </div>
  )
}
