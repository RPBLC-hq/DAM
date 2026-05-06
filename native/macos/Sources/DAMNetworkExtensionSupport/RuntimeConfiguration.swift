import Foundation

public struct DAMProxyRuntimeConfiguration: Equatable, Sendable {
    public static let defaultProxyHost = "127.0.0.1"
    public static let defaultProxyPort: UInt16 = 7828

    public static let defaultProtectedHosts = [
        "api.openai.com",
        "api.anthropic.com",
        "api.x.ai",
        "chatgpt.com",
    ]

    public static let defaultExcludedSigningIdentifiers = [
        "com.rpblc.dam",
        "com.rpblc.dam.daemon",
        "com.rpblc.dam.proxy",
        "com.rpblc.dam.tray",
        "com.rpblc.dam.network-extension",
        "com.rpblc.dam.helper",
    ]

    public var proxyHost: String
    public var proxyPort: UInt16
    public var protectedHosts: [String]
    public var excludedSigningIdentifiers: [String]

    public init(
        proxyHost: String = Self.defaultProxyHost,
        proxyPort: UInt16 = Self.defaultProxyPort,
        protectedHosts: [String] = Self.defaultProtectedHosts,
        excludedSigningIdentifiers: [String] = Self.defaultExcludedSigningIdentifiers
    ) {
        self.proxyHost = normalizeProxyHost(proxyHost)
        self.proxyPort = proxyPort
        self.protectedHosts = normalizeHosts(protectedHosts)
        self.excludedSigningIdentifiers = normalizeIdentifiers(excludedSigningIdentifiers)
    }

    public init(providerConfiguration: [String: Any]?) {
        let proxyHost = providerConfiguration?[DAMProviderConfigurationKey.proxyHost] as? String
        let proxyPort = providerConfiguration?[DAMProviderConfigurationKey.proxyPort] as? Int
        let protectedHosts = providerConfiguration?[DAMProviderConfigurationKey.protectedHosts] as? [String]
        let excludedSigningIdentifiers = providerConfiguration?[DAMProviderConfigurationKey.excludedSigningIdentifiers] as? [String]

        self.init(
            proxyHost: proxyHost ?? Self.defaultProxyHost,
            proxyPort: UInt16(clamping: proxyPort ?? Int(Self.defaultProxyPort)),
            protectedHosts: protectedHosts ?? Self.defaultProtectedHosts,
            excludedSigningIdentifiers: excludedSigningIdentifiers ?? Self.defaultExcludedSigningIdentifiers
        )
    }

    public var providerConfiguration: [String: Any] {
        [
            DAMProviderConfigurationKey.proxyHost: proxyHost,
            DAMProviderConfigurationKey.proxyPort: Int(proxyPort),
            DAMProviderConfigurationKey.protectedHosts: protectedHosts,
            DAMProviderConfigurationKey.excludedSigningIdentifiers: excludedSigningIdentifiers,
        ]
    }

    public func shouldBypassSource(signingIdentifier: String?) -> Bool {
        guard let signingIdentifier else {
            return false
        }
        let normalized = signingIdentifier.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !normalized.isEmpty else {
            return false
        }
        return excludedSigningIdentifiers.contains(normalized)
    }

    public func shouldProtect(host: String) -> Bool {
        let normalized = normalizeHost(host)
        guard !normalized.isEmpty else {
            return false
        }
        return protectedHosts.contains { protectedHost in
            normalized == protectedHost || normalized.hasSuffix(".\(protectedHost)")
        }
    }
}

public enum DAMProviderConfigurationKey {
    public static let proxyHost = "damProxyHost"
    public static let proxyPort = "damProxyPort"
    public static let protectedHosts = "damProtectedHosts"
    public static let excludedSigningIdentifiers = "damExcludedSigningIdentifiers"
}

public func normalizeHost(_ host: String) -> String {
    var trimmed = host.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    if trimmed.hasSuffix(".") {
        trimmed.removeLast()
    }
    return trimmed
}

func normalizeProxyHost(_ host: String) -> String {
    let normalized = host.trimmingCharacters(in: .whitespacesAndNewlines)
    return normalized.isEmpty ? DAMProxyRuntimeConfiguration.defaultProxyHost : normalized
}

func normalizeHosts(_ hosts: [String]) -> [String] {
    var result: [String] = []
    for host in hosts {
        let normalized = normalizeHost(host)
        if !normalized.isEmpty && !result.contains(normalized) {
            result.append(normalized)
        }
    }
    return result
}

func normalizeIdentifiers(_ identifiers: [String]) -> [String] {
    var result: [String] = []
    for identifier in identifiers {
        let normalized = identifier.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if !normalized.isEmpty && !result.contains(normalized) {
            result.append(normalized)
        }
    }
    return result
}
