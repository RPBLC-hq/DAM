export type AppDetectorSetting = {
  key: string
  label: string
  enabled: boolean
}

const DETECTOR_ORDER = ['email', 'phone', 'ssn', 'credit_card', 'api_key'] as const

const DETECTOR_FALLBACK_LABELS: Record<(typeof DETECTOR_ORDER)[number], string> = {
  email: 'Email',
  phone: 'Phone',
  ssn: 'SSN',
  credit_card: 'Credit card',
  api_key: 'API key',
}

export function normalizeDetectorSettings(
  detectors: AppDetectorSetting[] | undefined,
): AppDetectorSetting[] {
  if (!detectors) return []

  const supported = new Map(DETECTOR_ORDER.map((key) => [key, true] as const))
  const byKey = new Map(
    detectors
      .filter((detector) => supported.has(detector.key as (typeof DETECTOR_ORDER)[number]))
      .map((detector) => [detector.key, detector] as const),
  )

  return DETECTOR_ORDER.flatMap((key) => {
    const detector = byKey.get(key)
    if (!detector) return []
    return [
      {
        ...detector,
        label: detector.label || DETECTOR_FALLBACK_LABELS[key],
      },
    ]
  })
}
