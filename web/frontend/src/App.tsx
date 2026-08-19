import { Navigate, Route, Routes } from 'react-router-dom'
import { useAuth } from './context/AuthContext'
import { Layout } from './components/Layout'
import { Setup } from './pages/Setup'
import { Login } from './pages/Login'
import { Dashboard } from './pages/Dashboard'
import { Storage } from './pages/Storage'
import { Docker } from './pages/Docker'
import { Files } from './pages/Files'
import { Users } from './pages/Users'

function LoadingScreen() {
  return <div className="h-full flex items-center justify-center" style={{ color: 'var(--text-muted)' }}>Chargement…</div>
}

export function App() {
  const { user, loading, needsSetup } = useAuth()

  if (loading) return <LoadingScreen />

  if (needsSetup) {
    return (
      <Routes>
        <Route path="*" element={<Setup />} />
      </Routes>
    )
  }

  if (!user) {
    return (
      <Routes>
        <Route path="*" element={<Login />} />
      </Routes>
    )
  }

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Dashboard />} />
        <Route path="/storage" element={<Storage />} />
        <Route path="/docker" element={<Docker />} />
        <Route path="/files" element={<Files />} />
        <Route
          path="/users"
          element={user.role === 'admin' ? <Users /> : <Navigate to="/" replace />}
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  )
}
