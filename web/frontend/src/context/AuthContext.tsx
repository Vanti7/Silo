import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { api, ApiError } from '../api/client'
import type { User } from '../api/types'

interface AuthState {
  user: User | null
  loading: boolean
  needsSetup: boolean
  login: (username: string, password: string) => Promise<void>
  setup: (username: string, password: string) => Promise<void>
  logout: () => Promise<void>
  refresh: () => Promise<void>
}

const AuthContext = createContext<AuthState | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [needsSetup, setNeedsSetup] = useState(false)
  const [loading, setLoading] = useState(true)

  async function refresh() {
    try {
      const status = await api.get<{ needsSetup: boolean }>('/setup/status')
      setNeedsSetup(status.needsSetup)
      if (status.needsSetup) {
        setUser(null)
        return
      }
      const me = await api.get<User>('/auth/me')
      setUser(me)
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        setUser(null)
      }
    }
  }

  useEffect(() => {
    refresh().finally(() => setLoading(false))
  }, [])

  async function login(username: string, password: string) {
    const u = await api.post<User>('/auth/login', { username, password })
    setUser(u)
  }

  async function setup(username: string, password: string) {
    const u = await api.post<User>('/setup', { username, password })
    setNeedsSetup(false)
    setUser(u)
  }

  async function logout() {
    await api.post('/auth/logout')
    setUser(null)
  }

  return (
    <AuthContext.Provider value={{ user, loading, needsSetup, login, setup, logout, refresh }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth doit être utilisé dans un AuthProvider')
  return ctx
}
