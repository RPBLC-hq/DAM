import Foundation
import Network
import NetworkExtension

struct FlowEndpoint: Equatable, Sendable {
    var host: String
    var port: UInt16

    var authority: String {
        if host.contains(":") && !(host.hasPrefix("[") && host.hasSuffix("]")) {
            return "[\(host)]:\(port)"
        }
        return "\(host):\(port)"
    }

    var isIPAddress: Bool {
        IPv4Address(host) != nil || IPv6Address(host) != nil
    }

    var isHostlessTLSCandidate: Bool {
        port == 443 && isIPAddress
    }

    func replacingHost(_ host: String) -> FlowEndpoint {
        FlowEndpoint(host: host, port: port)
    }

    init(host: String, port: UInt16) {
        self.host = host
        self.port = port
    }

    init?(tcpFlow: NEAppProxyTCPFlow) {
        let endpoint = tcpFlow.remoteFlowEndpoint
        guard case let .hostPort(endpointHost, endpointPort) = endpoint else {
            return nil
        }

        let host = tcpFlow.remoteHostname?.isEmpty == false
            ? tcpFlow.remoteHostname!
            : String(describing: endpointHost)
        self.host = host
        self.port = endpointPort.rawValue
    }
}
