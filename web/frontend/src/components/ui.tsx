import type { ReactNode } from 'react'

export function Card({ title, action, children }: { title?: string; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="rounded-xl border p-5" style={{ background: 'var(--surface)', borderColor: 'var(--border)' }}>
      {(title || action) && (
        <div className="flex items-center justify-between mb-4">
          {title && <h2 className="text-sm font-semibold" style={{ color: 'var(--text)' }}>{title}</h2>}
          {action}
        </div>
      )}
      {children}
    </div>
  )
}

export function ProgressBar({ percent, tone = 'accent' }: { percent: number; tone?: 'accent' | 'danger' | 'warning' | 'success' }) {
  const clamped = Math.max(0, Math.min(100, percent))
  const color =
    tone === 'danger' ? 'var(--danger)' : tone === 'warning' ? 'var(--warning)' : tone === 'success' ? 'var(--success)' : 'var(--accent)'
  return (
    <div className="h-2 rounded-full overflow-hidden" style={{ background: 'var(--surface-alt)' }}>
      <div className="h-full rounded-full transition-all" style={{ width: `${clamped}%`, background: color }} />
    </div>
  )
}

const badgeTones = {
  accent: { bg: 'var(--accent-soft)', fg: 'var(--accent)' },
  danger: { bg: 'var(--danger-soft)', fg: 'var(--danger)' },
  success: { bg: 'var(--success-soft)', fg: 'var(--success)' },
  warning: { bg: 'var(--warning-soft)', fg: 'var(--warning)' },
  neutral: { bg: 'var(--surface-alt)', fg: 'var(--text-muted)' },
}

export function Badge({ children, tone = 'neutral' }: { children: ReactNode; tone?: keyof typeof badgeTones }) {
  const { bg, fg } = badgeTones[tone]
  return (
    <span className="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium" style={{ background: bg, color: fg }}>
      {children}
    </span>
  )
}

export function StatTile({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div>
      <div className="text-xs font-medium uppercase tracking-wide" style={{ color: 'var(--text-muted)' }}>
        {label}
      </div>
      <div className="text-2xl font-semibold mt-1">{value}</div>
      {sub && (
        <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
          {sub}
        </div>
      )}
    </div>
  )
}

export function Button({
  children,
  onClick,
  variant = 'default',
  disabled,
  type = 'button',
}: {
  children: ReactNode
  onClick?: () => void
  variant?: 'default' | 'primary' | 'danger' | 'ghost'
  disabled?: boolean
  type?: 'button' | 'submit'
}) {
  const styles: Record<string, { bg: string; fg: string; border: string }> = {
    default: { bg: 'var(--surface-alt)', fg: 'var(--text)', border: 'var(--border)' },
    primary: { bg: 'var(--accent)', fg: '#fff', border: 'var(--accent)' },
    danger: { bg: 'var(--danger)', fg: '#fff', border: 'var(--danger)' },
    ghost: { bg: 'transparent', fg: 'var(--text-muted)', border: 'transparent' },
  }
  const s = styles[variant]
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium border transition-opacity disabled:opacity-50"
      style={{ background: s.bg, color: s.fg, borderColor: s.border }}
    >
      {children}
    </button>
  )
}

export function formatBytes(bytes: number): string {
  if (!bytes) return '0 o'
  const units = ['o', 'Ko', 'Mo', 'Go', 'To', 'Po']
  const i = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)))
  return `${(bytes / 1024 ** i).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  if (days > 0) return `${days} j ${hours} h`
  if (hours > 0) return `${hours} h ${minutes} min`
  return `${minutes} min`
}
