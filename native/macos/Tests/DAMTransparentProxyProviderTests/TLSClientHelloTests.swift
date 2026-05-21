import Foundation
import XCTest

@testable import DAMTransparentProxyProvider

final class TLSClientHelloTests: XCTestCase {
    func testExtractsServerNameFromClientHello() {
        let hello = clientHello(serverName: "API.Anthropic.com")

        XCTAssertEqual(tlsClientHelloServerName(in: hello), .hostname("api.anthropic.com"))
    }

    func testReportsNeedMoreForPartialClientHello() {
        let hello = clientHello(serverName: "api.anthropic.com")

        XCTAssertEqual(tlsClientHelloServerName(in: hello.prefix(8)), .needMore)
    }

    func testRejectsNonTlsBytes() {
        XCTAssertEqual(tlsClientHelloServerName(in: Data("GET / HTTP/1.1\r\n\r\n".utf8)), .notClientHello)
    }

    private func clientHello(serverName: String) -> Data {
        let name = [UInt8](serverName.utf8)
        var serverNameExtension = [UInt8]()
        appendUInt16(3 + name.count, to: &serverNameExtension)
        serverNameExtension.append(0)
        appendUInt16(name.count, to: &serverNameExtension)
        serverNameExtension.append(contentsOf: name)

        var extensions = [UInt8]()
        appendUInt16(0, to: &extensions)
        appendUInt16(serverNameExtension.count, to: &extensions)
        extensions.append(contentsOf: serverNameExtension)

        var body = [UInt8]()
        body.append(contentsOf: [0x03, 0x03])
        body.append(contentsOf: Array(repeating: 0, count: 32))
        body.append(0)
        body.append(contentsOf: [0x00, 0x02, 0x13, 0x01])
        body.append(contentsOf: [0x01, 0x00])
        appendUInt16(extensions.count, to: &body)
        body.append(contentsOf: extensions)

        var handshake = [UInt8]()
        handshake.append(1)
        appendUInt24(body.count, to: &handshake)
        handshake.append(contentsOf: body)

        var record = [UInt8]()
        record.append(contentsOf: [22, 0x03, 0x01])
        appendUInt16(handshake.count, to: &record)
        record.append(contentsOf: handshake)
        return Data(record)
    }

    private func appendUInt16(_ value: Int, to bytes: inout [UInt8]) {
        bytes.append(UInt8((value >> 8) & 0xff))
        bytes.append(UInt8(value & 0xff))
    }

    private func appendUInt24(_ value: Int, to bytes: inout [UInt8]) {
        bytes.append(UInt8((value >> 16) & 0xff))
        bytes.append(UInt8((value >> 8) & 0xff))
        bytes.append(UInt8(value & 0xff))
    }
}
