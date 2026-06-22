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

    func testFailOpenHealthMissPassesNewFlowsWithoutCancelingActiveFlows() {
        let configuration = DAMProxyRuntimeConfiguration(routingFailurePolicy: .failOpen)
        let action = DAMTransparentProxyProvider.flowAction(
            proxyHealthState: nil,
            configuration: configuration
        )

        XCTAssertEqual(action, .passThrough)
        XCTAssertFalse(DAMTransparentProxyProvider.shouldCancelActiveFlows(for: action))
    }

    func testFailClosedHealthMissBlocksAndCancelsActiveFlows() {
        let configuration = DAMProxyRuntimeConfiguration(routingFailurePolicy: .failClosed)
        let action = DAMTransparentProxyProvider.flowAction(
            proxyHealthState: nil,
            configuration: configuration
        )

        XCTAssertEqual(action, .block)
        XCTAssertTrue(DAMTransparentProxyProvider.shouldCancelActiveFlows(for: action))
    }
}
