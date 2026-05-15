import type { MessageKey } from '@/lib/i18n'
import type { ConnectState, SetupStep } from './types'

const stateMessageKeys: Record<ConnectState, MessageKey> = {
  protected: 'connect.protectedStatus',
  paused: 'connect.pausedStatus',
  disconnected: 'connect.disconnectedLede',
  degraded: 'connect.degradedStatus',
  needs_setup: 'connect.setupStatus',
}

const stepLabelKeys: Record<string, MessageKey> = {
  launch_at_login: 'connect.step.launch_at_login',
  ne_install: 'connect.step.ne_install',
  ne_config: 'connect.step.ne_config',
  ne_enable: 'connect.step.ne_enable',
  ne_start: 'connect.step.ne_start',
  linux_capture: 'connect.step.linux_capture',
  windows_capture: 'connect.step.windows_capture',
  ne_reboot: 'connect.step.ne_reboot',
  ca_install: 'connect.step.ca_install',
  system_proxy: 'connect.step.system_proxy',
  apply_profiles: 'connect.step.apply_profiles',
  daemon_start: 'connect.step.daemon_start',
}

const detailStepLabelKeys: Record<string, MessageKey> = {
  'ne_install:waiting_for_approval': 'connect.step.ne_approval',
  'ne_config:rolled_back': 'connect.step.ne_rolled_back',
  'ne_enable:rolled_back': 'connect.step.ne_rolled_back',
}

const stepActionKeys: Record<string, MessageKey> = {
  launch_at_login: 'connect.action.launch_at_login',
  ne_install: 'connect.action.ne_install',
  ne_config: 'connect.action.ne_config',
  ne_enable: 'connect.action.ne_enable',
  ne_start: 'connect.action.ne_start',
  linux_capture: 'connect.action.platform_capture',
  windows_capture: 'connect.action.platform_capture',
  ne_reboot: 'connect.action.ne_reboot',
  ca_install: 'connect.action.ca_install',
  system_proxy: 'connect.action.system_proxy',
  apply_profiles: 'connect.action.apply_profiles',
  daemon_start: 'connect.action.daemon_start',
}

const detailStepActionKeys: Record<string, MessageKey> = {
  'ne_install:waiting_for_approval': 'connect.action.ne_approval',
  'ne_config:rolled_back': 'connect.action.ne_rolled_back',
  'ne_enable:rolled_back': 'connect.action.ne_rolled_back',
}

const stepHintKeys: Record<string, MessageKey> = {
  launch_at_login: 'connect.hint.launch_at_login',
  ne_install: 'connect.hint.ne_install',
  ne_config: 'connect.hint.ne_config',
  ne_enable: 'connect.hint.ne_enable',
  ne_start: 'connect.hint.ne_start',
  linux_capture: 'connect.hint.linux_capture',
  windows_capture: 'connect.hint.windows_capture',
  ne_reboot: 'connect.hint.ne_reboot',
  ca_install: 'connect.hint.ca_install',
  system_proxy: 'connect.hint.system_proxy',
  apply_profiles: 'connect.hint.apply_profiles',
  daemon_start: 'connect.hint.daemon_start',
}

const detailStepHintKeys: Record<string, MessageKey> = {
  'ne_install:waiting_for_approval': 'connect.hint.ne_approval',
  'ne_config:rolled_back': 'connect.hint.ne_rolled_back',
  'ne_enable:rolled_back': 'connect.hint.ne_rolled_back',
}

const errorKeys: Record<string, MessageKey> = {
  not_implemented: 'connect.error.not_implemented',
  ne_pending_user_approval: 'connect.error.ne_pending_user_approval',
  ne_reboot_required: 'connect.error.ne_reboot_required',
  ca_install_denied: 'connect.error.ca_install_denied',
  daemon_already_running: 'connect.error.daemon_already_running',
  apply_modified_target: 'connect.error.apply_modified_target',
  apply_target_unwritable: 'connect.error.apply_target_unwritable',
  daemon_starting: 'connect.error.daemon_starting',
  setup_step_failed: 'connect.error.setup_step_failed',
  network_offline: 'connect.error.network_offline',
  invalid_request: 'connect.error.invalid_request',
}

export function stateMessageKey(state: ConnectState): MessageKey {
  return stateMessageKeys[state]
}

export function stepLabelKey(step: SetupStep): MessageKey {
  const detailKey = detailStepKey(step)
  if (detailKey && detailStepLabelKeys[detailKey]) return detailStepLabelKeys[detailKey]
  return stepLabelKeys[step.id] ?? 'connect.step.unknown'
}

export function stepActionKey(step?: SetupStep | string): MessageKey {
  if (!step) return 'connect.action.unknown'
  if (typeof step === 'string') return stepActionKeys[step] ?? 'connect.action.unknown'
  const detailKey = detailStepKey(step)
  if (detailKey && detailStepActionKeys[detailKey]) return detailStepActionKeys[detailKey]
  return stepActionKeys[step.id] ?? 'connect.action.unknown'
}

export function stepHintKey(step: SetupStep): MessageKey | undefined {
  const detailKey = detailStepKey(step)
  if (detailKey && detailStepHintKeys[detailKey]) return detailStepHintKeys[detailKey]
  return stepHintKeys[step.id]
}

export function errorMessageKey(code?: string): MessageKey {
  return code ? errorKeys[code] ?? 'connect.error.unknown' : 'connect.error.unknown'
}

function detailStepKey(step: SetupStep): string | undefined {
  return step.detail ? `${step.id}:${step.detail}` : undefined
}
