import XCTest

@testable import DAMNetworkExtensionSupport
@testable import DAMTransparentProxyProvider

final class NetworkRuleTests: XCTestCase {
    func testIncludedRulesAddUdpFallbackForProtectedHosts() {
        let configuration = DAMProxyRuntimeConfiguration(
            protectedHosts: [
                "claude.ai",
                "api.anthropic.com",
            ]
        )

        XCTAssertEqual(
            DAMTransparentProxyProvider.includedNetworkRules(for: configuration).count,
            6
        )
    }

    func testIncludedRulesDoNotAddUdpFallbackWhenScopeIsEmpty() {
        let configuration = DAMProxyRuntimeConfiguration(protectedHosts: [])

        XCTAssertEqual(
            DAMTransparentProxyProvider.includedNetworkRules(for: configuration).count,
            4
        )
    }
}
