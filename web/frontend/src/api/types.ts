export interface User {
  id: number
  username: string
  role: 'admin' | 'user'
}

export interface SystemStats {
  cpuPercent: number
  loadAvg1: number
  loadAvg5: number
  loadAvg15: number
  memTotalBytes: number
  memUsedBytes: number
  memPercent: number
  swapTotalBytes: number
  swapUsedBytes: number
  uptimeSeconds: number
  temperatures?: { label: string; celsius: number }[]
  network: { name: string; bytesSent: number; bytesRecv: number }[]
}

export interface SystemInfo {
  hostname: string
  os: string
  platform: string
  kernel: string
  arch: string
}

export interface Filesystem {
  uuid: string
  label?: string
  mountPoint?: string
  totalBytes: number
  usedBytes: number
  devices: string[]
}

export interface Usage {
  deviceSizeBytes: number
  deviceAllocatedBytes: number
  deviceUnallocatedBytes: number
  dataUsedBytes: number
  dataTotalBytes: number
  metadataUsedBytes: number
  metadataTotalBytes: number
  freeEstimatedBytes: number
}

export interface Subvolume {
  id: number
  path: string
  parentId: number
}

export interface ScrubStatus {
  running: boolean
  rawOutput: string
}

export interface Container {
  id: string
  names: string[]
  image: string
  state: string
  status: string
  ports: { privatePort: number; publicPort: number; type: string }[]
  created: number
  labels: Record<string, string>
}

export interface DockerImage {
  id: string
  tags: string[]
  sizeBytes: number
  created: number
}

export interface DockerInfo {
  serverVersion: string
  containersRunning: number
  containersStopped: number
  images: number
  storageDriver: string
}

export interface FileEntry {
  name: string
  isDir: boolean
  sizeBytes: number
  modTime: string
}
