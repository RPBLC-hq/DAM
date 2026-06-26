import { useQuery } from '@tanstack/react-query'

import { api } from '@/lib/api/client'

import { connectObserverRefetchInterval } from './refresh'
import type { ConnectView } from './types'

export const CONNECT_QUERY_KEY = ['connect'] as const

export function useConnectViewQuery({ ownsPolling = false }: { ownsPolling?: boolean } = {}) {
  return useQuery({
    queryKey: CONNECT_QUERY_KEY,
    queryFn: ({ signal }) => api<ConnectView>('/connect', { signal }),
    // Counts include dam-log rows written by dam-proxy, which is outside
    // dam-web's in-process event bus. Keep exactly one mounted observer as
    // the interval owner; other shared surfaces read the same cached query.
    refetchInterval: connectObserverRefetchInterval(ownsPolling),
  })
}
