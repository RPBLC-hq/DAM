import DAMNetworkExtensionSupport
import Foundation
import NetworkExtension

extension NETransparentProxyManager: @retroactive @unchecked Sendable {}

struct ManagerStore {
    func loadManagers() async throws -> [NETransparentProxyManager] {
        try await withCheckedThrowingContinuation { continuation in
            NETransparentProxyManager.loadAllFromPreferences { managers, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: managers ?? [])
                }
            }
        }
    }

    func save(_ manager: NETransparentProxyManager) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            manager.saveToPreferences { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
    }

    func remove(_ manager: NETransparentProxyManager) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            manager.removeFromPreferences { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
    }

    func reload(_ manager: NETransparentProxyManager) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            manager.loadFromPreferences { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
    }

    func manager(matching bundleIdentifier: String, in managers: [NETransparentProxyManager]) -> NETransparentProxyManager? {
        managers.first { manager in
            let provider = manager.protocolConfiguration as? NETunnelProviderProtocol
            return provider?.providerBundleIdentifier == bundleIdentifier
        }
    }
}
