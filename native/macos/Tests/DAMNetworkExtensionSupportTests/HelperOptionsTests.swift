import XCTest
@testable import DAMNetworkExtensionSupport

final class HelperOptionsTests: XCTestCase {
    func testParseInstallOptionsWithRuntimeConfiguration() throws {
        let options = try parseHelperOptions([
            "install",
            "--bundle-id", "com.rpblc.dam.network-extension",
            "--team-id", "TEAMID1234",
            "--display-name", "DAM Network Protection",
            "--proxy-host", "127.0.0.1",
            "--proxy-port", "7828",
            "--protect-host", "api.openai.com",
            "--protect-host", "api.anthropic.com",
            "--exclude-signing-id", "com.rpblc.dam.proxy",
        ])

        XCTAssertEqual(options.action, .install)
        XCTAssertEqual(options.bundleIdentifier, "com.rpblc.dam.network-extension")
        XCTAssertEqual(options.teamIdentifier, "TEAMID1234")
        XCTAssertEqual(options.displayName, "DAM Network Protection")
        XCTAssertEqual(options.runtimeConfiguration.proxyHost, "127.0.0.1")
        XCTAssertEqual(options.runtimeConfiguration.proxyPort, 7828)
        XCTAssertEqual(options.runtimeConfiguration.protectedHosts, [
            "api.openai.com",
            "api.anthropic.com",
        ])
        XCTAssertTrue(options.runtimeConfiguration.shouldBypassSource(signingIdentifier: "com.rpblc.dam.proxy"))
    }

    func testParseRequiresBundleIdentifier() {
        XCTAssertThrowsError(try parseHelperOptions(["status"])) { error in
            XCTAssertEqual(error as? DAMHelperArgumentError, .missingBundleIdentifier)
        }
    }

    func testDefaultProtectedHostsIncludeMvpTargets() {
        let configuration = DAMProxyRuntimeConfiguration()

        XCTAssertTrue(configuration.shouldProtect(host: "api.openai.com"))
        XCTAssertTrue(configuration.shouldProtect(host: "api.anthropic.com"))
        XCTAssertTrue(configuration.shouldProtect(host: "chatgpt.com"))
        XCTAssertFalse(configuration.shouldProtect(host: "example.com"))
    }
}
