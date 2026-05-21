import Foundation

enum TLSClientHelloServerName: Equatable {
    case needMore
    case hostname(String)
    case noServerName
    case notClientHello
}

func tlsClientHelloServerName(in data: Data) -> TLSClientHelloServerName {
    let bytes = [UInt8](data)
    guard bytes.count >= 5 else {
        return .needMore
    }
    guard bytes[0] == 22 else {
        return .notClientHello
    }

    let recordLength = int16(bytes, 3)
    guard bytes.count >= 5 + recordLength else {
        return .needMore
    }
    guard bytes.count >= 9, bytes[5] == 1 else {
        return .notClientHello
    }

    let handshakeLength = int24(bytes, 6)
    let handshakeEnd = 9 + handshakeLength
    guard handshakeEnd <= bytes.count else {
        return .needMore
    }

    var offset = 9
    guard skip(&offset, 2 + 32, handshakeEnd) else {
        return .notClientHello
    }
    guard offset < handshakeEnd else {
        return .notClientHello
    }
    let sessionIDLength = Int(bytes[offset])
    guard skip(&offset, 1 + sessionIDLength, handshakeEnd) else {
        return .notClientHello
    }
    guard offset + 2 <= handshakeEnd else {
        return .notClientHello
    }
    let cipherSuiteLength = int16(bytes, offset)
    guard skip(&offset, 2 + cipherSuiteLength, handshakeEnd) else {
        return .notClientHello
    }
    guard offset < handshakeEnd else {
        return .notClientHello
    }
    let compressionMethodsLength = Int(bytes[offset])
    guard skip(&offset, 1 + compressionMethodsLength, handshakeEnd) else {
        return .notClientHello
    }
    guard offset + 2 <= handshakeEnd else {
        return .noServerName
    }
    let extensionsLength = int16(bytes, offset)
    offset += 2
    let extensionsEnd = min(offset + extensionsLength, handshakeEnd)

    while offset + 4 <= extensionsEnd {
        let extensionType = int16(bytes, offset)
        let extensionLength = int16(bytes, offset + 2)
        offset += 4
        guard offset + extensionLength <= extensionsEnd else {
            return .notClientHello
        }
        if extensionType == 0 {
            return serverName(in: bytes, offset: offset, end: offset + extensionLength)
        }
        offset += extensionLength
    }

    return .noServerName
}

private func serverName(in bytes: [UInt8], offset: Int, end: Int) -> TLSClientHelloServerName {
    var offset = offset
    guard offset + 2 <= end else {
        return .notClientHello
    }
    let listLength = int16(bytes, offset)
    offset += 2
    let listEnd = min(offset + listLength, end)
    while offset + 3 <= listEnd {
        let nameType = bytes[offset]
        let nameLength = int16(bytes, offset + 1)
        offset += 3
        guard offset + nameLength <= listEnd else {
            return .notClientHello
        }
        if nameType == 0 {
            let nameBytes = bytes[offset..<(offset + nameLength)]
            guard let host = String(bytes: nameBytes, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased(),
                  !host.isEmpty
            else {
                return .noServerName
            }
            return .hostname(host)
        }
        offset += nameLength
    }
    return .noServerName
}

private func int16(_ bytes: [UInt8], _ offset: Int) -> Int {
    (Int(bytes[offset]) << 8) | Int(bytes[offset + 1])
}

private func int24(_ bytes: [UInt8], _ offset: Int) -> Int {
    (Int(bytes[offset]) << 16) | (Int(bytes[offset + 1]) << 8) | Int(bytes[offset + 2])
}

private func skip(_ offset: inout Int, _ count: Int, _ end: Int) -> Bool {
    guard offset + count <= end else {
        return false
    }
    offset += count
    return true
}
