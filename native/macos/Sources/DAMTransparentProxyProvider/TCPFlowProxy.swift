import DAMNetworkExtensionSupport
import Foundation
import Network
import NetworkExtension

final class TCPFlowProxy: @unchecked Sendable {
    let id = UUID()

    private let flow: NEAppProxyTCPFlow
    private let endpoint: FlowEndpoint
    private let runtimeConfiguration: DAMProxyRuntimeConfiguration
    private let queue: DispatchQueue
    private let onFinish: @Sendable (UUID) -> Void

    private var connection: NWConnection?
    private var finished = false

    init(
        flow: NEAppProxyTCPFlow,
        endpoint: FlowEndpoint,
        runtimeConfiguration: DAMProxyRuntimeConfiguration,
        onFinish: @escaping @Sendable (UUID) -> Void
    ) {
        self.flow = flow
        self.endpoint = endpoint
        self.runtimeConfiguration = runtimeConfiguration
        self.queue = DispatchQueue(label: "com.rpblc.dam.network-extension.flow.\(id.uuidString)")
        self.onFinish = onFinish
    }

    func start() {
        flow.open(withLocalFlowEndpoint: nil) { [weak self] error in
            guard let self else {
                return
            }
            guard error == nil else {
                self.finish()
                return
            }
            self.connectToProxy()
        }
    }

    func cancel() {
        finish()
    }

    private func connectToProxy() {
        guard let proxyPort = NWEndpoint.Port(rawValue: runtimeConfiguration.proxyPort) else {
            finish()
            return
        }
        let connection = NWConnection(
            host: NWEndpoint.Host(runtimeConfiguration.proxyHost),
            port: proxyPort,
            using: .tcp
        )
        self.connection = connection
        connection.stateUpdateHandler = { [weak self] state in
            switch state {
            case .ready:
                self?.sendConnectPreface()
            case .failed, .cancelled:
                self?.finish()
            default:
                break
            }
        }
        connection.start(queue: queue)
    }

    private func sendConnectPreface() {
        let request = [
            "CONNECT \(endpoint.authority) HTTP/1.1",
            "Host: \(endpoint.authority)",
            "Proxy-Connection: keep-alive",
            "",
            "",
        ].joined(separator: "\r\n")

        connection?.send(content: Data(request.utf8), completion: .contentProcessed { [weak self] error in
            guard let self else {
                return
            }
            guard error == nil else {
                self.finish()
                return
            }
            self.readConnectResponse(buffer: Data())
        })
    }

    private func readConnectResponse(buffer: Data) {
        connection?.receive(minimumIncompleteLength: 1, maximumLength: 4096) { [weak self] content, _, isComplete, error in
            guard let self else {
                return
            }
            guard error == nil, let content else {
                self.finish()
                return
            }

            var nextBuffer = buffer
            nextBuffer.append(content)
            if let headerEnd = nextBuffer.range(of: Data("\r\n\r\n".utf8)) {
                let headerBytes = nextBuffer[..<headerEnd.lowerBound]
                let headers = String(decoding: headerBytes, as: UTF8.self)
                guard headers.hasPrefix("HTTP/1.1 200") || headers.hasPrefix("HTTP/1.0 200") else {
                    self.finish()
                    return
                }
                let bodyStart = headerEnd.upperBound
                if bodyStart < nextBuffer.endIndex {
                    let remaining = Data(nextBuffer[bodyStart...])
                    self.writeProxyDataToFlow(remaining) {
                        self.startPumps()
                    }
                } else {
                    self.startPumps()
                }
                return
            }

            if isComplete || nextBuffer.count > 64 * 1024 {
                self.finish()
                return
            }
            self.readConnectResponse(buffer: nextBuffer)
        }
    }

    private func startPumps() {
        pumpFlowToProxy()
        pumpProxyToFlow()
    }

    private func pumpFlowToProxy() {
        flow.readData { [weak self] data, error in
            guard let self else {
                return
            }
            guard error == nil, let data else {
                self.finish()
                return
            }
            if data.isEmpty {
                self.connection?.send(content: nil, contentContext: .defaultMessage, isComplete: true, completion: .contentProcessed { [weak self] _ in
                    self?.finish()
                })
                return
            }
            self.connection?.send(content: data, completion: .contentProcessed { [weak self] error in
                guard let self else {
                    return
                }
                guard error == nil else {
                    self.finish()
                    return
                }
                self.pumpFlowToProxy()
            })
        }
    }

    private func pumpProxyToFlow() {
        connection?.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] content, _, isComplete, error in
            guard let self else {
                return
            }
            guard error == nil else {
                self.finish()
                return
            }
            if let content, !content.isEmpty {
                self.writeProxyDataToFlow(content) {
                    if isComplete {
                        self.finish()
                    } else {
                        self.pumpProxyToFlow()
                    }
                }
                return
            }
            if isComplete {
                self.finish()
            } else {
                self.pumpProxyToFlow()
            }
        }
    }

    private func writeProxyDataToFlow(_ data: Data, completion: @escaping @Sendable () -> Void) {
        flow.write(data) { [weak self] error in
            guard error == nil else {
                self?.finish()
                return
            }
            completion()
        }
    }

    private func finish() {
        queue.async {
            guard !self.finished else {
                return
            }
            self.finished = true
            self.connection?.cancel()
            self.flow.closeReadWithError(nil)
            self.flow.closeWriteWithError(nil)
            self.onFinish(self.id)
        }
    }
}
