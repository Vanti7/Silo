// Client HTTP minimal pour l'API silo. L'authentification repose sur un
// cookie de session httpOnly (voir internal/auth côté backend) : chaque
// requête inclut les credentials, aucun jeton n'est manipulé en JS.

const BASE = '/api/v1'

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...init?.headers,
    },
  })

  if (!res.ok) {
    let message = res.statusText
    try {
      const body = await res.json()
      if (body?.error) message = body.error
    } catch {
      // corps non-JSON : on garde le statusText
    }
    throw new ApiError(res.status, message)
  }

  if (res.status === 204) return undefined as T
  const contentType = res.headers.get('content-type') ?? ''
  if (!contentType.includes('application/json')) return undefined as T
  return res.json() as Promise<T>
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body?: unknown) =>
    request<T>(path, { method: 'POST', body: body !== undefined ? JSON.stringify(body) : undefined }),
  patch: <T>(path: string, body?: unknown) =>
    request<T>(path, { method: 'PATCH', body: body !== undefined ? JSON.stringify(body) : undefined }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
}

export function downloadUrl(root: string, path: string): string {
  return `${BASE}/files/download?root=${encodeURIComponent(root)}&path=${encodeURIComponent(path)}`
}

export function logsUrl(id: string, tail = '200'): string {
  return `${BASE}/docker/containers/${encodeURIComponent(id)}/logs?tail=${tail}`
}
