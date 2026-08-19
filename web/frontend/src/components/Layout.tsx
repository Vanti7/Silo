import { NavLink, Outlet } from 'react-router-dom'
import { LayoutDashboard, HardDrive, Box, FolderOpen, Users, LogOut, Sun, Moon, Server } from 'lucide-react'
import { useAuth } from '../context/AuthContext'
import { useTheme } from '../hooks/useTheme'

const navItems = [
  { to: '/', label: 'Tableau de bord', icon: LayoutDashboard, end: true },
  { to: '/storage', label: 'Stockage', icon: HardDrive },
  { to: '/docker', label: 'Docker', icon: Box },
  { to: '/files', label: 'Fichiers', icon: FolderOpen },
]

export function Layout() {
  const { user, logout } = useAuth()
  const { theme, toggleTheme } = useTheme()

  return (
    <div className="flex h-full">
      <aside className="w-60 shrink-0 border-r flex flex-col" style={{ borderColor: 'var(--border)', background: 'var(--surface)' }}>
        <div className="flex items-center gap-2 px-5 h-14 border-b" style={{ borderColor: 'var(--border)' }}>
          <Server size={20} style={{ color: 'var(--accent)' }} />
          <span className="font-semibold tracking-tight">silo</span>
        </div>

        <nav className="flex-1 px-3 py-4 space-y-1">
          {navItems.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors ${
                  isActive ? 'font-medium' : ''
                }`
              }
              style={({ isActive }) => ({
                background: isActive ? 'var(--accent-soft)' : 'transparent',
                color: isActive ? 'var(--accent)' : 'var(--text-muted)',
              })}
            >
              <Icon size={18} />
              {label}
            </NavLink>
          ))}

          {user?.role === 'admin' && (
            <NavLink
              to="/users"
              className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors"
              style={({ isActive }) => ({
                background: isActive ? 'var(--accent-soft)' : 'transparent',
                color: isActive ? 'var(--accent)' : 'var(--text-muted)',
              })}
            >
              <Users size={18} />
              Utilisateurs
            </NavLink>
          )}
        </nav>

        <div className="px-3 py-4 border-t space-y-1" style={{ borderColor: 'var(--border)' }}>
          <button
            onClick={toggleTheme}
            className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm"
            style={{ color: 'var(--text-muted)' }}
          >
            {theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
            {theme === 'dark' ? 'Mode clair' : 'Mode sombre'}
          </button>
          <div className="flex items-center justify-between px-3 py-1">
            <div className="text-sm">
              <div className="font-medium">{user?.username}</div>
              <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
                {user?.role === 'admin' ? 'Administrateur' : 'Utilisateur'}
              </div>
            </div>
            <button onClick={() => logout()} title="Déconnexion" style={{ color: 'var(--text-muted)' }}>
              <LogOut size={18} />
            </button>
          </div>
        </div>
      </aside>

      <main className="flex-1 overflow-auto">
        <div className="max-w-6xl mx-auto p-6">
          <Outlet />
        </div>
      </main>
    </div>
  )
}
